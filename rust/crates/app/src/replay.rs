//! Recorded trade answers, for scenes, headless renders and tests.
//!
//! The reference screenshots (`tools/golden/screens`) replay every request from files
//! under `tools/golden/corpus/screens/<game>/proxy/`, and their page clock is frozen at
//! `<game>/recorded-at.txt` so listing ages match. This module ports the naming
//! (`proxyFileName` in `tools/golden/screens/host.mjs`), the lookup and the frozen clock,
//! so a scene can search through the real [`wraex_core::trade::api::TradeClient`] with no
//! network.
//!
//! The README's "Proxy fixtures" section is the contract:
//!
//! - the name is the URL after `/proxy/` with `/` -> `__`, `?` -> `--`, `&` -> `-`,
//!   `=` -> `_` and anything outside `[A-Za-z0-9._-]` -> `_`;
//! - a request with a body appends `--body<8 hex of sha1(body)>`;
//! - a name over 100 characters is cut to 64 and gets `--len<length>-<16 hex of
//!   sha1(name)>`, so a whole path fits Windows' `MAX_PATH` (260 characters), not
//!   just one component under 255 bytes;
//! - lookup is `<name>.<METHOD>.json`, then `<name>.json`.
//!
//! A missing recording is an error: the app never falls back to the network, so a
//! scene that changes a request shows the failure instead of a live price.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha1::{Digest, Sha1};
use wraex_core::relative_time::{EpochMillis, parse_iso};
use wraex_core::trade::api::{Clock, HttpRequest, HttpResponse, Method, Transport};

/// `MAX_NAME`: the longest file name the harness writes before truncating.
pub const MAX_NAME: usize = 100;

/// `CUT_NAME`: how much of a longer name is kept, before `--len<length>-<hash>`.
pub const CUT_NAME: usize = 64;

/// The name a request's recording has, as `proxyFileName` builds it. `raw_url` is
/// the URL after `/proxy/` (no `https://`, no leading slash).
pub fn proxy_file_name(raw_url: &str, body: Option<&str>) -> String {
    let (pathname, query) = match raw_url.split_once('?') {
        Some((pathname, query)) => (pathname, query),
        None => (raw_url, ""),
    };
    let mut name = sanitize(pathname);
    if !query.is_empty() {
        name.push_str("--");
        name.push_str(&sanitize(query));
    }
    if let Some(body) = body.filter(|body| !body.is_empty()) {
        name.push_str("--body");
        name.push_str(prefix(&sha1_hex(body), 8));
    }
    if name.len() > MAX_NAME {
        let hash = sha1_hex(&name);
        name = format!("{}--len{}-{}", prefix(&name, CUT_NAME), name.len(), prefix(&hash, 16));
    }
    name
}

/// The first `len` bytes of `text`, or all of it when it is shorter. Every string this module
/// cuts is ASCII (a sanitised URL, or a SHA-1 hex digest), so `len` is always a char boundary.
fn prefix(text: &str, len: usize) -> &str {
    text.get(..len).unwrap_or(text)
}

fn sanitize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '/' => out.push_str("__"),
            '?' => out.push_str("--"),
            '&' => out.push('-'),
            '=' => out.push('_'),
            c if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') => out.push(c),
            _ => out.push('_'),
        }
    }
    out
}

fn sha1_hex(text: &str) -> String {
    let digest = Sha1::digest(text.as_bytes());
    let mut out = String::with_capacity(40);
    for byte in digest {
        for nibble in [byte >> 4, byte & 0x0f] {
            out.push(char::from_digit(u32::from(nibble), 16).unwrap_or('0'));
        }
    }
    out
}

/// The path a recording lives at, from a full request URL (`https://host/...`).
pub fn fixture_path(root: &Path, request: &HttpRequest) -> Result<PathBuf, String> {
    let raw = request.url.strip_prefix("https://").ok_or_else(|| format!("not an https URL: {}", request.url))?;
    let method = match request.method {
        Method::Get => "GET",
        Method::Post => "POST",
    };
    let name = proxy_file_name(raw, request.body.as_deref());
    let with_method = root.join(format!("{name}.{method}.json"));
    if with_method.is_file() {
        return Ok(with_method);
    }
    let plain = root.join(format!("{name}.json"));
    if plain.is_file() {
        return Ok(plain);
    }
    Err(format!("no recorded answer for {method} {raw} (looked for {name}.{method}.json, {name}.json)"))
}

/// `{ "status": ..., "headers": { ... } }`, written next to a recorded answer.
#[derive(Debug, serde::Deserialize)]
struct Meta {
    status: u16,
    #[serde(default)]
    headers: std::collections::HashMap<String, String>,
}

fn read_meta(answer: &Path) -> Option<Meta> {
    let name = answer.file_name()?.to_str()?;
    let stem = name.strip_suffix(".json")?;
    let text = std::fs::read_to_string(answer.with_file_name(format!("{stem}.meta.json"))).ok()?;
    serde_json::from_str(&text).ok()
}

/// Answers requests from the recorded files (`--record` in the reference harness is
/// not ported: a miss is an error).
#[derive(Debug, Clone)]
pub struct ReplayTransport {
    root: PathBuf,
}

impl ReplayTransport {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// The recorded answers of one game (`tools/golden/corpus/screens/<game>/proxy`).
    pub fn poe1() -> Self {
        Self::new(format!("{CORPUS}/poe1/proxy"))
    }

    /// PoE2's recorded answers (`tools/golden/corpus/screens/poe2/proxy`).
    pub fn poe2() -> Self {
        Self::new(format!("{CORPUS}/poe2/proxy"))
    }
}

impl Transport for ReplayTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        let path = fixture_path(&self.root, request)?;
        let body = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let (status, headers) = match read_meta(&path) {
            Some(meta) => (meta.status, meta.headers),
            None => (200, HashMap::default()),
        };
        Ok(HttpResponse { status, headers, body })
    }
}

/// A clock frozen at the moment the screenshots were recorded, advanced only by the
/// waits a rate limit asks for. Listing ages therefore match the reference exactly.
#[derive(Debug)]
pub struct ReplayClock {
    now: AtomicU64,
}

impl ReplayClock {
    pub fn new(base: EpochMillis) -> Self {
        Self { now: AtomicU64::new(base.max(0).cast_unsigned()) }
    }

    /// The clock at `tools/golden/corpus/screens/poe1/recorded-at.txt`.
    pub fn poe1() -> Self {
        Self::new(recorded_at("poe1"))
    }

    /// The clock at `tools/golden/corpus/screens/poe2/recorded-at.txt`.
    pub fn poe2() -> Self {
        Self::new(recorded_at("poe2"))
    }
}

impl Clock for ReplayClock {
    fn now(&self) -> u64 {
        self.now.load(Ordering::Relaxed)
    }

    fn sleep(&self, millis: u64) {
        self.now.fetch_add(millis, Ordering::Relaxed);
    }
}

/// Where the screenshots' corpus lives: `tools/golden/corpus/screens`.
pub const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tools/golden/corpus/screens");

/// `corpus/screens/<game>/recorded-at.txt`, the page clock the screenshots froze at.
/// A missing or unparsable file is the harness's fallback (`2026-01-01T12:00:00Z`).
pub fn recorded_at(game: &str) -> EpochMillis {
    // The harness's fallback date; `parse_iso` reads the exact form it writes, so this is
    // always the parsed date and never the epoch.
    let fallback = parse_iso("2026-01-01T12:00:00Z").unwrap_or_default();
    std::fs::read_to_string(format!("{CORPUS}/{game}/recorded-at.txt"))
        .ok()
        .and_then(|text| parse_iso(text.trim()))
        .unwrap_or(fallback)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use wraex_core::trade::api::{HttpRequest, Method, Transport};

    use super::*;

    fn corpus() -> &'static Path {
        Path::new(CORPUS)
    }

    /// The recorded search answer the fetch name is rebuilt from.
    #[derive(serde::Deserialize)]
    struct SearchAnswer {
        id: String,
        result: Vec<String>,
    }

    /// The README's two examples, then the two suffixes.
    #[test]
    fn names_follow_the_harness() {
        assert_eq!(
            proxy_file_name("www.pathofexile.com/api/leagues?type=main&realm=pc", None),
            "www.pathofexile.com__api__leagues--type_main-realm_pc"
        );
        assert_eq!(
            proxy_file_name("www.pathofexile.com/api/trade2/data/leagues", None),
            "www.pathofexile.com__api__trade2__data__leagues"
        );
        // A body's first eight SHA-1 hex digits keep two searches of one URL apart.
        let body = r#"{"query":{"status":{"option":"securable"},"stats":[{"type":"and","filters":[]}],"filters":{}},"sort":{"price":"asc"}}"#;
        assert_eq!(
            proxy_file_name("www.pathofexile.com/api/trade/search/Standard", Some(body)),
            format!("www.pathofexile.com__api__trade__search__Standard--body{}", prefix(&sha1_hex(body), 8))
        );
        // Over MAX_NAME: the first CUT_NAME characters, then the length and a hash of
        // the whole name.
        let long = format!("www.pathofexile.com/api/trade/fetch/{}?query={}", "a".repeat(300), "b".repeat(60));
        let (pathname, query) = long.split_once('?').expect("a query");
        let full = format!("{}--{}", sanitize(pathname), sanitize(query));
        let name = proxy_file_name(&long, None);
        assert!(name.len() <= MAX_NAME, "{name}");
        assert!(name.starts_with(prefix(&full, CUT_NAME)), "{name}");
        assert_eq!(name, format!("{}--len{}-{}", prefix(&full, CUT_NAME), full.len(), prefix(&sha1_hex(&full), 16)));
    }

    /// Windows' `MAX_PATH` (260 characters) is a whole-path limit, so every recording's
    /// file name has to stay far below the 255-byte component limit, suffix included.
    #[test]
    fn every_proxy_file_name_fits_a_windows_path() {
        let mut checked = 0;
        for game in ["poe1", "poe2"] {
            let dir = corpus().join(game).join("proxy");
            for entry in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("read {}: {e}", dir.display())) {
                let name = entry.unwrap().file_name().to_string_lossy().into_owned();
                assert!(name.len() <= 120, "{game}/proxy/{name} is {} characters", name.len());
                checked += 1;
            }
        }
        assert!(checked > 0, "no proxy recordings found");
    }

    /// The truncation rule agrees with the files on disk: a fetch of the timeless
    /// jewel's one id, rebuilt from the recorded search answer, resolves to the very
    /// file the harness wrote.
    #[test]
    fn a_recorded_fetch_name_matches_its_file() {
        let dir = corpus().join("poe1/proxy");
        let search: SearchAnswer = serde_json::from_str(
            &std::fs::read_to_string(
                dir.join("www.pathofexile.com__api__trade__search__Standard--body95b7a0dc.POST.json"),
            )
            .unwrap(),
        )
        .unwrap();
        let ids: Vec<&str> = search.result.iter().map(String::as_str).collect();
        let request = HttpRequest {
            method: Method::Get,
            url: format!("https://www.pathofexile.com/api/trade/fetch/{}?query={}", ids.join(","), search.id),
            headers: Vec::new(),
            body: None,
        };
        let path = fixture_path(&dir, &request).expect("the recorded fetch");
        assert!(path.is_file(), "{}", path.display());
        let raw = request.url.strip_prefix("https://").unwrap();
        let name = proxy_file_name(raw, None);
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), format!("{name}.GET.json"));
    }

    #[test]
    fn a_recorded_answer_replays_and_a_missing_one_errors() {
        let transport = ReplayTransport::poe1();
        let request = HttpRequest {
            method: Method::Get,
            url: "https://www.pathofexile.com/api/leagues?type=main&realm=pc".into(),
            headers: Vec::new(),
            body: None,
        };
        let response = transport.send(&request).expect("the hand-written leagues answer");
        assert_eq!(response.status, 200);
        assert!(response.body.contains("Standard"));

        let missing =
            HttpRequest { url: "https://www.pathofexile.com/api/trade/search/NoSuchLeague".into(), ..request };
        let error = transport.send(&missing).unwrap_err();
        assert!(error.contains("no recorded answer"), "{error}");
    }

    #[test]
    fn the_clock_is_frozen_at_the_recording() {
        let recorded = parse_iso("2026-11-24T10:11:38.653Z").expect("ISO with milliseconds");
        assert_eq!(recorded_at("poe1"), recorded);
        let clock = ReplayClock::poe1();
        assert_eq!(i64::try_from(clock.now()), Ok(recorded));
        clock.sleep(1000);
        assert_eq!(i64::try_from(clock.now()), Ok(recorded + 1000));
    }
}
