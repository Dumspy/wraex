//! The [`Transport`] the app sends requests with: `main/src/proxy.ts` without
//! the proxy. Every request carries [`USER_AGENT`]; hosts are the trade proxy's
//! [`ALLOWED_HOSTS`] plus the update check's [`UPDATE_HOST`], the item art's
//! [`CDN_HOST`] and the map screenshots' [`SCREENSHOT_HOST`]. A release setup is
//! downloaded by [`Fetch::fetch`] from [`DOWNLOAD_HOSTS`] (plus [`UPDATE_HOST`]).
//! Non-2xx answers come back as responses (the trade site's errors are JSON bodies),
//! and the body is decoded (gzip) like `net.request` does.
//!
//! Cookies behave like Electron's persistent default session, which upstream's
//! `net.request({ useSessionCookies: true })` uses: one jar for every request,
//! matching RFC 6265's domain, path, expiry and `Secure` rules, with the
//! `Partitioned` attribute removed before storing a `Set-Cookie` (Cloudflare sets
//! such cookies) as upstream's `onHeadersReceived` does. When the transport is
//! given a file, the expiring cookies are saved to it and loaded at startup;
//! session cookies live only for the run. A transport without a file (tests, the
//! headless replay runs) keeps the jar in memory.
//!
//! Item art is binary, so it goes through [`HttpTransport::get_bytes`] rather than
//! [`Transport::send`], whose body is the decoded text the trade answers are. A
//! release's setup is larger still and must not sit in memory, so it is streamed
//! through [`Fetch::fetch`].

use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cookie_store::CookieStore;
use ureq::Agent;
use url::Url;
use wraex_core::images::{CDN_HOST, SCREENSHOT_HOST};
use wraex_core::trade::api::{ALLOWED_HOSTS, HttpRequest, HttpResponse, Method, Transport, USER_AGENT};
use wraex_core::updates::source::Fetch;
use wraex_core::updates::{DOWNLOAD_HOSTS, UPDATE_HOST};

/// One session's cookies, as a browser keeps them: RFC 6265 matching over
/// [`CookieStore`], and optionally a JSON file the expiring ones are saved to.
#[derive(Debug)]
struct CookieJar {
    /// `None` keeps the jar in memory only: tests and headless runs touch no file.
    file: Option<PathBuf>,
    store: Mutex<CookieStore>,
}

impl CookieJar {
    /// An empty jar, or one loaded from `file` ([`cookie_store::serde::json::load`]
    /// skips cookies that have expired).
    fn new(file: Option<PathBuf>) -> Self {
        let store = file.as_deref().map(load_store).unwrap_or_default();
        Self { file, store: Mutex::new(store) }
    }

    /// The `Cookie` header for `url`: every matching cookie, `name=value` pairs joined
    /// with `; `, as RFC 6265's cookie-string.
    fn header(&self, url: &str) -> Option<String> {
        let url = Url::parse(url).ok()?;
        let store = self.store.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut cookies = String::new();
        for (name, value) in store.get_request_values(&url) {
            if !cookies.is_empty() {
                cookies.push_str("; ");
            }
            cookies.push_str(name);
            cookies.push('=');
            cookies.push_str(value);
        }
        (!cookies.is_empty()).then_some(cookies)
    }

    /// Stores the `Set-Cookie` headers of a response from `url`, after dropping
    /// `Partitioned`, then saves the expiring ones when the jar has a file.
    fn store(&self, url: &str, set_cookies: &[String]) {
        let Ok(url) = Url::parse(url) else { return };
        let mut store = self.store.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for header in set_cookies {
            // An unparseable cookie is the site's problem, not the request's: ignore it,
            // as a browser does, and never log its value.
            let _ = store.parse(&without_partitioned(header), &url);
        }
        if let Some(path) = &self.file {
            save_store(&store, path);
        }
    }
}

/// `cookie` without its `Partitioned` attribute, matching upstream's
/// `onHeadersReceived`: `net.request` cannot name a partition key, so the cookie
/// would otherwise be dropped and Cloudflare's challenge would never pass.
fn without_partitioned(cookie: &str) -> String {
    cookie.split(';').filter(|part| !part.trim().eq_ignore_ascii_case("partitioned")).collect::<Vec<_>>().join(";")
}

/// The jar in `path`, or an empty one when the file does not exist or cannot be read.
/// Logs the count only: a cookie's name and value never reach the log.
fn load_store(path: &Path) -> CookieStore {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return CookieStore::default(),
        Err(error) => {
            crate::log!("wraex: cannot read the cookie file {}: {error}", path.display());
            return CookieStore::default();
        }
    };
    match cookie_store::serde::json::load(BufReader::new(file)) {
        Ok(store) => {
            crate::log!("wraex: {} cookies loaded", store.iter_unexpired().count());
            store
        }
        Err(_) => {
            // The parser's message can quote the file, so it is not logged.
            crate::log!("wraex: ignoring the unreadable cookie file {}", path.display());
            CookieStore::default()
        }
    }
}

/// Writes the unexpired, persistent cookies to `path` (session cookies are not saved);
/// the temporary file keeps a crash from leaving half a jar behind.
fn save_store(store: &CookieStore, path: &Path) {
    if let Some(folder) = path.parent()
        && let Err(error) = std::fs::create_dir_all(folder)
    {
        crate::log!("wraex: cannot create {}: {error}", folder.display());
        return;
    }
    let temporary = path.with_extension("json.tmp");
    let written = (|| -> std::io::Result<()> {
        let mut writer = BufWriter::new(std::fs::File::create(&temporary)?);
        cookie_store::serde::json::save(store, &mut writer).map_err(std::io::Error::other)?;
        writer.flush()?;
        std::fs::rename(&temporary, path)
    })();
    if let Err(error) = written {
        crate::log!("wraex: cannot save the cookie file {}: {error}", path.display());
        let _ = std::fs::remove_file(&temporary);
    }
}

/// Blocking HTTPS. Call it from a worker thread, not the UI thread.
#[derive(Debug, Clone)]
pub struct HttpTransport {
    agent: Agent,
    cookies: Arc<CookieJar>,
}

/// Downloading a setup is a few tens of MB, which the agent's 30 s global timeout would
/// cut off on a slow line; the download overrides it per request instead of raising the
/// timeout for the trade requests too.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

impl Default for HttpTransport {
    /// No cookie file: the jar lives as long as the transport. Tests and the headless
    /// replay runs use this; the live app passes [`HttpTransport::new`] its session file.
    fn default() -> Self {
        Self::new(None)
    }
}

/// The host of an `https://` URL.
fn host(url: &str) -> Option<&str> {
    let rest = url.strip_prefix("https://")?;
    Some(rest.split(['/', '?']).next().unwrap_or(rest))
}

/// [`ALLOWED_HOSTS`] plus [`UPDATE_HOST`], [`CDN_HOST`] and [`SCREENSHOT_HOST`].
fn allowed(host: &str) -> bool {
    ALLOWED_HOSTS.contains(&host) || host == UPDATE_HOST || host == CDN_HOST || host == SCREENSHOT_HOST
}

impl HttpTransport {
    /// A transport whose session cookies are saved to and loaded from `cookie_file`
    /// (`None`: memory only).
    pub fn new(cookie_file: Option<PathBuf>) -> Self {
        let config = Agent::config_builder()
            .http_status_as_error(false)
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Self { agent: config.into(), cookies: Arc::new(CookieJar::new(cookie_file)) }
    }

    /// The `Cookie` header for `url`: the jar's matching cookies followed by any the
    /// caller set itself.
    fn cookie_header(&self, url: &str, headers: &[(&'static str, String)]) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(cookies) = self.cookies.header(url) {
            parts.push(cookies);
        }
        for (name, value) in headers {
            if name.eq_ignore_ascii_case("cookie") {
                parts.push(value.clone());
            }
        }
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    /// Stores a response's `Set-Cookie` headers in the session jar.
    fn remember_cookies(&self, url: &str, set_cookies: Vec<String>) {
        if !set_cookies.is_empty() {
            self.cookies.store(url, &set_cookies);
        }
    }

    /// A `GET` whose body is bytes, for an image: same agent, same [`USER_AGENT`], same
    /// host allowlist and same cookie jar as [`Transport::send`]. Only a 2xx answer is
    /// returned; anything else is an error, because an error page is not a picture.
    pub fn get_bytes(&self, url: &str) -> Result<Vec<u8>, String> {
        // The page's hosts, the update check's, and the item CDN's.
        let host = host(url).ok_or_else(|| format!("not an https URL: {url}"))?;
        if !allowed(host) {
            return Err(format!("host not allowed: {host}"));
        }
        let mut request = self.agent.get(url);
        if let Some(cookies) = self.cookie_header(url, &[]) {
            request = request.header("cookie", cookies);
        }
        let mut response = request.call().map_err(|e| e.to_string())?;
        let status = response.status();
        self.remember_cookies(url, set_cookies(response.headers()));
        if !status.is_success() {
            return Err(format!("{status} from {url}"));
        }
        response.body_mut().read_to_vec().map_err(|e| e.to_string())
    }
}

/// The values of a response's `Set-Cookie` headers, in order.
fn set_cookies(headers: &ureq::http::HeaderMap) -> Vec<String> {
    headers
        .get_all(ureq::http::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .map(str::to_owned)
        .collect()
}

impl Transport for HttpTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        // `PROXY_HOSTS`, plus the update check.
        let host = host(&request.url).ok_or_else(|| format!("not an https URL: {}", request.url))?;
        if !allowed(host) {
            return Err(format!("host not allowed: {host}"));
        }
        let cookies = self.cookie_header(&request.url, &request.headers);

        let result = match request.method {
            Method::Get => {
                let mut r = self.agent.get(&request.url);
                if let Some(cookies) = &cookies {
                    r = r.header("cookie", cookies);
                }
                for (name, value) in &request.headers {
                    if !name.eq_ignore_ascii_case("cookie") {
                        r = r.header(*name, value);
                    }
                }
                r.call()
            }
            Method::Post => {
                let mut r = self.agent.post(&request.url);
                if let Some(cookies) = &cookies {
                    r = r.header("cookie", cookies);
                }
                for (name, value) in &request.headers {
                    if !name.eq_ignore_ascii_case("cookie") {
                        r = r.header(*name, value);
                    }
                }
                r.send(request.body.as_deref().unwrap_or(""))
            }
        };
        let mut response = result.map_err(|e| e.to_string())?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some((name.as_str().to_ascii_lowercase(), value.to_str().ok()?.to_owned())))
            .collect();
        self.remember_cookies(&request.url, set_cookies(response.headers()));
        let body = response.body_mut().read_to_string().map_err(|e| e.to_string())?;
        Ok(HttpResponse { status, headers, body })
    }
}

/// Streams a release asset to `sink`. The host allowlist is the download's own
/// ([`DOWNLOAD_HOSTS`], plus [`UPDATE_HOST`]) because `github.com` is not a trade host;
/// the redirect GitHub answers with goes to `release-assets.githubusercontent.com` or
/// `objects.githubusercontent.com`, which ureq follows, and the SHA-256 check in
/// `wraex_core::updates::download` is what makes that safe.
impl Fetch for HttpTransport {
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u64, String> {
        let host = host(url).ok_or_else(|| format!("not an https URL: {url}"))?;
        if !DOWNLOAD_HOSTS.contains(&host) && host != UPDATE_HOST {
            return Err(format!("host not allowed: {host}"));
        }
        let mut request = self.agent.get(url).config().timeout_global(Some(DOWNLOAD_TIMEOUT)).build();
        request = request.header("accept", "application/octet-stream");
        if let Some(cookies) = self.cookie_header(url, &[]) {
            request = request.header("cookie", cookies);
        }
        let mut response = request.call().map_err(|error| error.to_string())?;
        self.remember_cookies(url, set_cookies(response.headers()));
        if !response.status().is_success() {
            return Err(format!("{} from {url}", response.status()));
        }
        // `as_reader` streams; `read_to_vec` would stop at ureq's 10 MB body limit.
        let mut body = response.body_mut().as_reader();
        std::io::copy(&mut body, sink).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh cookie file under the temp dir, named for the test.
    fn temp_cookie_file(name: &str) -> PathBuf {
        let folder = std::env::temp_dir().join(format!("wraex-cookies-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder.join("cookies.json")
    }

    #[test]
    fn only_the_proxy_hosts_the_update_host_the_cdn_and_imgur_are_reached() {
        assert_eq!(host("https://www.pathofexile.com/api/trade/search/x"), Some("www.pathofexile.com"));
        assert_eq!(host("https://poe.ninja?x"), Some("poe.ninja"));
        assert_eq!(host("https://api.github.com/repos/Dumspy/wraex/releases/latest"), Some("api.github.com"));
        assert_eq!(host("https://web.poecdn.com/gen/image/x.png"), Some("web.poecdn.com"));
        assert_eq!(host("https://i.imgur.com/qZNQGdD.jpg"), Some("i.imgur.com"));
        assert!(allowed("www.pathofexile.com"));
        assert!(allowed(UPDATE_HOST));
        assert!(allowed(CDN_HOST));
        assert!(allowed(SCREENSHOT_HOST));
        assert!(!allowed("github.com"));
        assert!(!allowed("cdn.poecdn.com"));
        let request =
            HttpRequest { method: Method::Get, url: "https://example.com/".into(), headers: Vec::new(), body: None };
        assert_eq!(HttpTransport::default().send(&request).unwrap_err(), "host not allowed: example.com");
        assert_eq!(
            HttpTransport::default().get_bytes("https://example.com/x.png").unwrap_err(),
            "host not allowed: example.com"
        );
    }

    /// A release download starts only at a release host, and the refusal comes before any
    /// network call (this test has none). GitHub redirects to its asset hosts; that hop is
    /// checked by the SHA-256, not by this allowlist.
    #[test]
    fn a_download_starts_only_at_a_release_host() {
        assert!(DOWNLOAD_HOSTS.contains(&"github.com"));
        assert!(!allowed("github.com"), "the trade transport still does not send there");

        let mut sink = Vec::new();
        let error = HttpTransport::default()
            .fetch("https://release-assets.githubusercontent.com/x/wraex-setup.exe", &mut sink)
            .unwrap_err();
        assert_eq!(error, "host not allowed: release-assets.githubusercontent.com");
        assert!(sink.is_empty(), "nothing was written");

        let error = HttpTransport::default().fetch("https://example.com/setup.exe", &mut sink).unwrap_err();
        assert_eq!(error, "host not allowed: example.com");
    }

    /// Cloudflare's challenge cookie carries `Partitioned`, which upstream strips before
    /// the session stores it.
    #[test]
    fn partitioned_is_stripped_and_the_cookie_is_stored() {
        assert_eq!(
            without_partitioned("__cf_bm=abc; Path=/; HttpOnly; Partitioned; SameSite=None"),
            "__cf_bm=abc; Path=/; HttpOnly; SameSite=None"
        );
        assert_eq!(without_partitioned("a=1; partitioned"), "a=1");
        assert_eq!(without_partitioned("a=1; Partitioned=1"), "a=1; Partitioned=1");

        let jar = CookieJar::new(None);
        let url = "https://www.pathofexile.com/api/trade/search/Standard";
        jar.store(url, &["__cf_bm=abc; Path=/; Secure; HttpOnly; Partitioned; SameSite=None".to_owned()]);
        assert_eq!(
            jar.header("https://www.pathofexile.com/api/trade/fetch/abc"),
            Some("__cf_bm=abc".to_owned()),
            "the Partitioned cookie was stored and is sent back"
        );
    }

    /// A cookie set by one response goes to the next request of its own host only.
    #[test]
    fn a_cookie_is_sent_only_to_the_host_that_set_it() {
        let jar = CookieJar::new(None);
        jar.store("https://www.pathofexile.com/api/trade/search/x", &["cf=1; Path=/; Secure".to_owned()]);
        assert_eq!(jar.header("https://www.pathofexile.com/api/trade/fetch/y"), Some("cf=1".to_owned()));
        assert_eq!(jar.header("https://poe.ninja/"), None);
        assert_eq!(jar.header("https://pathofexile.com/"), None, "a host-only cookie is not for the parent");
    }

    /// `Domain` and `Path` narrow where a cookie is sent, as a browser's jar does.
    #[test]
    fn a_cookie_honours_its_domain_and_path() {
        let jar = CookieJar::new(None);
        jar.store(
            "https://www.pathofexile.com/api/trade/search/x",
            &["trade=1; Domain=pathofexile.com; Path=/api/trade; Secure".to_owned()],
        );
        assert_eq!(jar.header("https://www.pathofexile.com/api/trade/fetch/y"), Some("trade=1".to_owned()));
        assert_eq!(jar.header("https://ru.pathofexile.com/api/trade/search/y"), Some("trade=1".to_owned()));
        assert_eq!(jar.header("https://www.pathofexile.com/account/view-profile"), None);
        assert_eq!(jar.header("https://poe.ninja/api/trade"), None);
    }

    /// An expired cookie is not stored, and a deletion expires the one already there.
    #[test]
    fn cookies_expire_as_the_browser_does() {
        let jar = CookieJar::new(None);
        let url = "https://www.pathofexile.com/";
        jar.store(url, &["old=1; Path=/; Expires=Thu, 01 Jan 1970 00:00:00 GMT".to_owned()]);
        assert_eq!(jar.header(url), None, "an expiry in the past deletes the cookie");

        jar.store(url, &["live=1; Path=/; Max-Age=3600".to_owned()]);
        assert_eq!(jar.header(url), Some("live=1".to_owned()));
        jar.store(url, &["live=; Path=/; Max-Age=0".to_owned()]);
        assert_eq!(jar.header(url), None, "Max-Age=0 expires it");
    }

    /// The file round-trips the expiring cookies and leaves the session's out.
    #[test]
    fn persistence_keeps_the_expiring_cookies_and_not_the_session_ones() {
        let path = temp_cookie_file("round-trip");
        let url = "https://www.pathofexile.com/";
        let jar = CookieJar::new(Some(path.clone()));
        jar.store(
            url,
            &["keep=1; Path=/; Max-Age=86400; Secure; HttpOnly".to_owned(), "session=1; Path=/; Secure".to_owned()],
        );

        let saved = std::fs::read_to_string(&path).expect("the jar was saved");
        assert!(saved.contains("keep"), "the expiring cookie is on disk");
        assert!(!saved.contains("session"), "the session cookie is not");

        let loaded = CookieJar::new(Some(path.clone()));
        assert_eq!(loaded.header(url), Some("keep=1".to_owned()));
        let _ = std::fs::remove_dir_all(path.parent().expect("a folder"));
    }

    /// The one place a real image is fetched: run it by hand (`cargo test -p
    /// wraex-platform -- --ignored live_cdn`), since CI has no network. `art.rs`'s replay
    /// path, which every screenshot and test uses, reads the recordings instead.
    #[test]
    #[ignore = "needs the network"]
    fn live_cdn_fetch_returns_the_image_bytes() {
        let bytes = HttpTransport::default()
            .get_bytes(
                "https://web.poecdn.com/gen/image/WzI1LDE0LHsiZiI6IjJESXRlbXMvQ3VycmVuY3kvQ3VycmVuY3lNb2RWYWx1ZXMiLCJzY2FsZSI6MX1d/ec48896769/CurrencyModValues.png",
            )
            .expect("the divine orb's art");
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n", "a PNG, not an error page");
    }
}
