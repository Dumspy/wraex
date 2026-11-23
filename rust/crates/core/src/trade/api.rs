//! The trade site's HTTP API, as both references call it through their proxy
//! (`requestTradeResultList`, `requestResults` and the exchange request in
//! `pathofexile-trade.ts`/`pathofexile-bulk.ts`; `main/src/proxy.ts`).
//!
//! [`TradeClient`] does what those functions do around a request: refuse long
//! queues, wait for the rate limits, send, sync the limits from the response
//! headers, turn `{ error }` bodies into errors, and cache results for five
//! minutes. Sending is a [`Transport`] and time a [`Clock`], so the logic
//! runs in tests without a network or real waiting.

use std::collections::HashMap;

use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::rate_limit::{Millis, RateLimits, RetryAfter, prevent_queue_creation};

/// The User-Agent the app identifies itself with. Upstream sends Electron's
/// `app.userAgentFallback`, which names the app and its version; this does the
/// same for Wraex.
pub const USER_AGENT: &str = concat!("wraex/", env!("CARGO_PKG_VERSION"), " (+https://github.com/Dumspy/wraex)");

/// Results stay cached this long (`CACHE_TTL`, seconds).
pub const CACHE_TTL_SECONDS: u64 = 300;

/// Which trade API: PoE1's `/api/trade`, PoE2's `/api/trade2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSite {
    Trade,
    Trade2,
}

impl TradeSite {
    fn path(self) -> &'static str {
        match self {
            TradeSite::Trade => "trade",
            TradeSite::Trade2 => "trade2",
        }
    }
}

/// `poeWebApi` (PoE1): the trade site host for the app's language.
pub fn poe1_web_api(language: &str, use_intl_site: bool) -> Option<&'static str> {
    if use_intl_site {
        return Some("www.pathofexile.com");
    }
    match language {
        "en" => Some("www.pathofexile.com"),
        "ru" => Some("ru.pathofexile.com"),
        "cmn-Hant" => Some("pathofexile.tw"),
        "ko" => Some("poe.kakaogames.com"),
        _ => None,
    }
}

/// `poeWebApi` (PoE2): by language, realm and `preferredTradeSite`.
pub fn poe2_web_api(language: &str, realm: &str, preferred_trade_site: &str) -> Option<&'static str> {
    if preferred_trade_site == "www" {
        return Some("www.pathofexile.com");
    }
    match language {
        "en" => Some("www.pathofexile.com"),
        "ru" => Some("ru.pathofexile.com"),
        "cmn-Hant" if realm == "pc-garena" => Some("pathofexile.tw"),
        "cmn-Hant" => Some("www.pathofexile.com"),
        "ko" => Some("poe.kakaogames.com"),
        "ja" => Some("jp.pathofexile.com"),
        "de" => Some("de.pathofexile.com"),
        "es" => Some("es.pathofexile.com"),
        "pt" => Some("br.pathofexile.com"),
        "fr" => Some("fr.pathofexile.com"),
        _ => None,
    }
}

/// `PROXY_HOSTS` (`main/src/proxy.ts`): the hosts the app may talk to.
pub const ALLOWED_HOSTS: [&str; 6] = [
    "www.pathofexile.com",
    "ru.pathofexile.com",
    "pathofexile.tw",
    "poe.kakaogames.com",
    "poe.ninja",
    "www.poeprices.info",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

/// A request for the [`Transport`]: `url` is `https://` plus what upstream
/// passes to `Host.proxy`.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(&'static str, String)>,
    pub body: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct HttpResponse {
    pub status: u16,
    /// Header names in lower case.
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl HttpResponse {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).map(String::as_str)
    }
}

/// Sends a request (blocking). The app's implementation adds [`USER_AGENT`].
pub trait Transport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String>;
}

/// The current time, and waiting.
pub trait Clock {
    fn now(&self) -> Millis;
    fn sleep(&self, millis: Millis);
}

/// What a trade request can fail with.
#[derive(Debug, Clone, PartialEq)]
pub enum TradeError {
    /// `preventQueueCreation`: try again later.
    RetryAfter(RetryAfter),
    /// The site's `{ error: { code, message } }`.
    Api { code: i64, message: String },
    /// No connection, a bad body, ...
    Transport(String),
}

impl std::fmt::Display for TradeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TradeError::RetryAfter(r) => r.fmt(f),
            TradeError::Api { message, .. } => f.write_str(message),
            TradeError::Transport(e) => f.write_str(e),
        }
    }
}

impl std::error::Error for TradeError {}

/// `SearchResult` of a search: the query id and the matching listing ids.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub result: Vec<String>,
    pub total: f64,
    #[serde(default)]
    pub inexact: Option<bool>,
}

/// `SearchResult` of an exchange: listings by id, in the order the site sent
/// them (`Object.values` keeps it; `serde_json::Value` would sort the keys).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(bound = "L: Deserialize<'de>")]
pub struct ExchangeResult<L> {
    pub id: String,
    pub result: IndexMap<String, L>,
    pub total: f64,
}

/// `TradeResponse<T>`: the site answers errors as `{ error: { code, message } }`.
fn parse_body<T: DeserializeOwned>(body: &str, status: u16) -> Result<T, TradeError> {
    #[derive(Deserialize)]
    struct ApiError {
        #[serde(default)]
        code: Option<i64>,
        #[serde(default)]
        message: Option<String>,
    }
    #[derive(Deserialize)]
    struct MaybeError {
        #[serde(default)]
        error: Option<ApiError>,
    }
    let probe: MaybeError =
        serde_json::from_str(body).map_err(|e| TradeError::Transport(format!("HTTP {status}: {e}")))?;
    if let Some(error) = probe.error {
        return Err(TradeError::Api { code: error.code.unwrap_or(0), message: error.message.unwrap_or_default() });
    }
    serde_json::from_str(body).map_err(|e| TradeError::Transport(e.to_string()))
}

/// Percent-encodes a path segment like the WHATWG URL parser does with the
/// template strings upstream builds (spaces in league names, ...).
fn encode_path(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        if byte <= 0x20 || byte >= 0x7f || b"\"#<>?`{}/".contains(&byte) {
            out.push_str(&format!("%{byte:02X}"));
        } else {
            out.push(byte as char);
        }
    }
    out
}

fn json_post(url: String, body: String) -> HttpRequest {
    HttpRequest {
        method: Method::Post,
        url,
        headers: vec![("accept", "application/json".to_owned()), ("content-type", "application/json".to_owned())],
        body: Some(body),
    }
}

/// The rate limits per kind of request (`RATE_LIMIT_RULES`).
#[derive(Debug, Clone, PartialEq)]
pub struct Limits {
    pub search: RateLimits,
    pub exchange: RateLimits,
    pub fetch: RateLimits,
}

impl Default for Limits {
    fn default() -> Self {
        Self { search: RateLimits::initial(), exchange: RateLimits::initial(), fetch: RateLimits::initial() }
    }
}

/// A result cache with a time to live (`Cache.ts`), keyed by the request,
/// holding the response bodies.
#[derive(Debug, Clone, Default)]
struct Cache {
    entries: HashMap<String, (Millis, String)>,
}

impl Cache {
    fn get(&mut self, key: &str, now: Millis) -> Option<String> {
        self.entries.retain(|_, (expires, _)| *expires > now);
        self.entries.get(key).map(|(_, v)| v.clone())
    }

    fn set(&mut self, key: String, value: String, ttl_seconds: u64, now: Millis) {
        self.entries.insert(key, (now + ttl_seconds * 1000, value));
    }
}

/// The trade API client: one per game and site.
#[derive(Debug)]
pub struct TradeClient<T, C> {
    pub transport: T,
    pub clock: C,
    pub host: String,
    pub site: TradeSite,
    /// The price-check widget's `apiLatencySeconds`, added to every limit window.
    pub latency_seconds: f64,
    pub limits: Limits,
    cache: Cache,
}

impl<T: Transport, C: Clock> TradeClient<T, C> {
    pub fn new(transport: T, clock: C, host: &str, site: TradeSite, latency_seconds: f64) -> Self {
        Self {
            transport,
            clock,
            host: host.to_owned(),
            site,
            latency_seconds,
            limits: Limits::default(),
            cache: Cache::default(),
        }
    }

    fn api_url(&self, rest: &str) -> String {
        format!("https://{}/api/{}/{rest}", self.host, self.site.path())
    }

    /// Waits for `limits`, sends, and syncs `limits` from the response.
    fn send(
        &mut self,
        kind: fn(&mut Limits) -> &mut RateLimits,
        request: &HttpRequest,
    ) -> Result<HttpResponse, TradeError> {
        // `RateLimiter.waitMulti`
        loop {
            let delay = kind(&mut self.limits).delay(self.clock.now());
            if delay == 0 {
                break;
            }
            self.clock.sleep(delay);
        }
        kind(&mut self.limits).acquire(self.clock.now());

        let response = self.transport.send(request).map_err(TradeError::Transport)?;
        let now = self.clock.now();
        kind(&mut self.limits).adjust(|name| response.header(name), self.latency_seconds, now);
        Ok(response)
    }

    /// `requestTradeResultList`: a search, `POST /api/trade[2]/search/{league}`.
    pub fn search(&mut self, body: &impl Serialize, league: &str) -> Result<SearchResult, TradeError> {
        let body = serde_json::to_string(body).map_err(|e| TradeError::Transport(e.to_string()))?;
        let key = serde_json::json!(["search", body, league]).to_string();
        if let Some(cached) = self.cache.get(&key, self.clock.now()) {
            return parse_body(&cached, 200);
        }

        let now = self.clock.now();
        let Limits { search, fetch, .. } = &mut self.limits;
        prevent_queue_creation(&mut [(1, search), (1, fetch)], now).map_err(TradeError::RetryAfter)?;

        let request = json_post(self.api_url(&format!("search/{}", encode_path(league))), body);
        let response = self.send(|l| &mut l.search, &request)?;
        let result = parse_body(&response.body, response.status)?;
        self.cache.set(key, response.body, CACHE_TTL_SECONDS, self.clock.now());
        Ok(result)
    }

    /// `requestResults`' request: listings by id, `GET /api/trade[2]/fetch/{ids}?query={id}`.
    /// Missing listings (`null`) are left out.
    pub fn fetch<L: DeserializeOwned>(&mut self, query_id: &str, result_ids: &[String]) -> Result<Vec<L>, TradeError> {
        #[derive(Deserialize)]
        #[serde(bound = "L: Deserialize<'de>")]
        struct Fetched<L> {
            result: Vec<Option<L>>,
        }
        let listings = |f: Fetched<L>| f.result.into_iter().flatten().collect();

        let key = serde_json::json!(["fetch", result_ids]).to_string();
        if let Some(cached) = self.cache.get(&key, self.clock.now()) {
            return parse_body(&cached, 200).map(listings);
        }

        let request = HttpRequest {
            method: Method::Get,
            url: self.api_url(&format!("fetch/{}?query={}", result_ids.join(","), encode_path(query_id))),
            headers: Vec::new(),
            body: None,
        };
        let response = self.send(|l| &mut l.fetch, &request)?;
        let fetched: Fetched<L> = parse_body(&response.body, response.status)?;
        self.cache.set(key, response.body, CACHE_TTL_SECONDS, self.clock.now());
        Ok(listings(fetched))
    }

    /// The bulk exchange search, `POST /api/trade[2]/exchange/{league}`.
    pub fn exchange<L: DeserializeOwned>(
        &mut self,
        body: &impl Serialize,
        league: &str,
    ) -> Result<ExchangeResult<L>, TradeError> {
        let body = serde_json::to_string(body).map_err(|e| TradeError::Transport(e.to_string()))?;
        let key = serde_json::json!(["exchange", body, league]).to_string();
        if let Some(cached) = self.cache.get(&key, self.clock.now()) {
            return parse_body(&cached, 200);
        }

        let now = self.clock.now();
        prevent_queue_creation(&mut [(1, &mut self.limits.exchange)], now).map_err(TradeError::RetryAfter)?;

        let request = json_post(self.api_url(&format!("exchange/{}", encode_path(league))), body);
        let response = self.send(|l| &mut l.exchange, &request)?;
        let result = parse_body(&response.body, response.status)?;
        self.cache.set(key, response.body, CACHE_TTL_SECONDS, self.clock.now());
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};

    use serde_json::json;

    use super::*;

    #[derive(Debug, Default)]
    struct FakeClock(Cell<Millis>);

    impl Clock for &FakeClock {
        fn now(&self) -> Millis {
            self.0.get()
        }
        fn sleep(&self, millis: Millis) {
            self.0.set(self.0.get() + millis);
        }
    }

    /// Answers every request with `body` and the given rate-limit headers.
    #[derive(Debug, Default)]
    struct FakeSite {
        sent: RefCell<Vec<HttpRequest>>,
        body: String,
        headers: Vec<(&'static str, &'static str)>,
    }

    impl Transport for &FakeSite {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            self.sent.borrow_mut().push(request.clone());
            Ok(HttpResponse {
                status: 200,
                headers: self.headers.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                body: self.body.clone(),
            })
        }
    }

    fn site(body: serde_json::Value) -> FakeSite {
        FakeSite {
            body: body.to_string(),
            headers: vec![
                ("x-rate-limit-rules", "Ip"),
                ("x-rate-limit-ip", "5:10:60,15:60:300"),
                ("x-rate-limit-ip-state", "1:10:0,1:60:0"),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn searches_post_to_the_site_and_cache_the_result() {
        let site = site(json!({ "id": "abc", "result": ["1", "2"], "total": 2 }));
        let clock = FakeClock::default();
        let mut client = TradeClient::new(&site, &clock, "www.pathofexile.com", TradeSite::Trade2, 2.0);

        let body = json!({ "query": {} });
        let result = client.search(&body, "Standard Hardcore").unwrap();
        assert_eq!((result.id.as_str(), result.total), ("abc", 2.0));
        let sent = site.sent.borrow()[0].clone();
        assert_eq!(sent.url, "https://www.pathofexile.com/api/trade2/search/Standard%20Hardcore");
        assert_eq!(sent.method, Method::Post);
        assert_eq!(sent.body.as_deref(), Some(r#"{"query":{}}"#));

        // The server's limits replaced the initial one.
        let windows: Vec<f64> = client.limits.search.limiters.iter().map(|l| l.window).collect();
        assert_eq!(windows, [12.0, 62.0]);

        // Cached: no second request.
        client.search(&body, "Standard Hardcore").unwrap();
        assert_eq!(site.sent.borrow().len(), 1);
        // Until the TTL runs out.
        (&clock).sleep(CACHE_TTL_SECONDS * 1000);
        client.search(&body, "Standard Hardcore").unwrap();
        assert_eq!(site.sent.borrow().len(), 2);
    }

    #[test]
    fn a_full_limit_waits_and_a_long_queue_is_refused() {
        let site = site(json!({ "id": "abc", "result": [], "total": 0 }));
        let clock = FakeClock::default();
        let mut client = TradeClient::new(&site, &clock, "www.pathofexile.com", TradeSite::Trade, 0.0);
        client.limits.search = RateLimits::initial();
        client.search(&json!(1), "Standard").unwrap();
        // The server now allows 5 per 10 s with 1 used; a burst of four more fits.
        for i in 2..=5 {
            client.search(&json!(i), "Standard").unwrap();
        }
        assert_eq!((&clock).now(), 0);
        let err = client.search(&json!(6), "Standard").unwrap_err();
        assert!(matches!(err, TradeError::RetryAfter(_)), "{err:?}");
    }

    #[test]
    fn a_short_wait_sleeps_until_the_limit_frees_up() {
        let mut site = site(json!({ "result": [] }));
        site.headers =
            vec![("x-rate-limit-rules", "Ip"), ("x-rate-limit-ip", "1:1:60"), ("x-rate-limit-ip-state", "1:1:0")];
        let clock = FakeClock::default();
        let mut client = TradeClient::new(&site, &clock, "www.pathofexile.com", TradeSite::Trade, 0.0);
        client.fetch::<serde_json::Value>("q", &["1".to_owned()]).unwrap();
        (&clock).sleep(200);
        client.fetch::<serde_json::Value>("q", &["2".to_owned()]).unwrap();
        // The second fetch waited for the one-second window to pass.
        assert_eq!((&clock).now(), 1000);
        assert_eq!(site.sent.borrow().len(), 2);
    }

    #[test]
    fn fetches_drop_missing_listings() {
        let site = site(json!({ "result": [{ "id": "1" }, null, { "id": "3" }] }));
        let clock = FakeClock::default();
        let mut client = TradeClient::new(&site, &clock, "www.pathofexile.com", TradeSite::Trade, 2.0);
        let ids = ["1".to_owned(), "2".to_owned(), "3".to_owned()];
        let listings: Vec<serde_json::Value> = client.fetch("abc", &ids).unwrap();
        assert_eq!(listings, [json!({ "id": "1" }), json!({ "id": "3" })]);
        assert_eq!(site.sent.borrow()[0].url, "https://www.pathofexile.com/api/trade/fetch/1,2,3?query=abc");
    }

    #[test]
    fn site_errors_become_api_errors() {
        let site = site(json!({ "error": { "code": 2, "message": "Invalid query" } }));
        let clock = FakeClock::default();
        let mut client = TradeClient::new(&site, &clock, "www.pathofexile.com", TradeSite::Trade, 2.0);
        let err = client.exchange::<serde_json::Value>(&json!({}), "Standard").unwrap_err();
        assert_eq!(err, TradeError::Api { code: 2, message: "Invalid query".into() });
        assert_eq!(site.sent.borrow()[0].url, "https://www.pathofexile.com/api/trade/exchange/Standard");
    }

    #[test]
    fn exchange_listings_keep_the_site_order() {
        let site = FakeSite { body: r#"{"id":"q","total":2,"result":{"zz":1,"aa":2}}"#.into(), ..Default::default() };
        let clock = FakeClock::default();
        let mut client = TradeClient::new(&site, &clock, "www.pathofexile.com", TradeSite::Trade, 2.0);
        let result: ExchangeResult<u32> = client.exchange(&json!({}), "Standard").unwrap();
        assert_eq!(result.result.keys().collect::<Vec<_>>(), ["zz", "aa"]);
    }

    #[test]
    fn hosts_follow_language_and_realm() {
        assert_eq!(poe1_web_api("ru", false), Some("ru.pathofexile.com"));
        assert_eq!(poe1_web_api("ru", true), Some("www.pathofexile.com"));
        assert_eq!(poe2_web_api("cmn-Hant", "pc-garena", "default"), Some("pathofexile.tw"));
        assert_eq!(poe2_web_api("cmn-Hant", "pc-ggg", "default"), Some("www.pathofexile.com"));
        assert_eq!(poe2_web_api("pt", "pc-ggg", "default"), Some("br.pathofexile.com"));
        assert!(USER_AGENT.starts_with("wraex/"));
    }
}
