//! The trade API client the app sends with, chosen at startup.
//!
//! A window sends over HTTPS (`wraex_platform::http::HttpTransport`) from a worker
//! thread. A headless run (`--snap`, `--scene`, `--script`) and the tests instead
//! replay the answers recorded for the reference screenshots (`crate::replay`), so
//! they need no network and their listing ages are frozen. Both paths go through the
//! same [`wraex_core::trade::api::TradeClient`], so rate limits, caching and error
//! handling are the app's real ones.

use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wraex_core::trade::api::{ALLOWED_HOSTS, Clock, HttpRequest, HttpResponse, TradeClient, TradeSite, Transport};
use wraex_platform::http::HttpTransport;

use crate::replay::{ReplayClock, ReplayTransport};

/// Where a request goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The real app: HTTPS.
    Live,
    /// Recorded answers for the screenshots.
    Replay,
}

impl Mode {
    /// The mode a run starts in: replay whenever Rune renders headlessly.
    pub fn for_run() -> Self {
        if rune::is_headless() { Self::Replay } else { Self::Live }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Self::for_run()
    }
}

/// [`HttpTransport`] for the app, recorded answers otherwise.
#[derive(Debug, Clone)]
pub enum TargetTransport {
    Live(HttpTransport),
    Replay(ReplayTransport),
}

impl Transport for TargetTransport {
    fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
        match self {
            Self::Live(transport) => transport.send(request),
            Self::Replay(transport) => transport.send(request),
        }
    }
}

/// The wall clock for the app; the frozen recording time for a replay.
#[derive(Debug)]
pub enum TargetClock {
    Live(LiveClock),
    Replay(ReplayClock),
}

impl Clock for TargetClock {
    fn now(&self) -> u64 {
        match self {
            Self::Live(clock) => clock.now(),
            Self::Replay(clock) => clock.now(),
        }
    }

    fn sleep(&self, millis: u64) {
        match self {
            Self::Live(clock) => clock.sleep(millis),
            // Nothing to wait for: the answers are already on disk.
            Self::Replay(clock) => clock.sleep(millis),
        }
    }
}

/// Milliseconds since the Unix epoch; `sleep` really waits.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveClock;

impl Clock for LiveClock {
    fn now(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
    }

    fn sleep(&self, millis: u64) {
        std::thread::sleep(Duration::from_millis(millis));
    }
}

/// The client one window searches with. Shared with the worker thread that runs a
/// search, so its cache and rate limits outlive a search.
pub type Client = TradeClient<TargetTransport, TargetClock>;

/// A client that sends with [`mode`], to `host`, allowing `latency_seconds` on top of
/// every rate-limit window (`PasswordCheckWidget.apiLatencySeconds`).
pub fn client(mode: Mode, host: &str, latency_seconds: f64) -> Client {
    debug_assert!(ALLOWED_HOSTS.contains(&host), "{host} is not one of upstream's proxy hosts");
    let (transport, clock) = target(mode);
    TradeClient::new(transport, clock, host, TradeSite::Trade, latency_seconds)
}

/// Which game's trade site a client talks to, and which game's recordings it replays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Site {
    /// `/api/trade`, `corpus/screens/poe1/proxy`.
    Poe1,
    /// `/api/trade2`, `corpus/screens/poe2/proxy`.
    Poe2,
}

/// The one live transport. Its cookie jar is the app's session, as Electron's default
/// session is for upstream: a Cloudflare cookie set on one answer goes out with the next
/// request, and the expiring cookies are saved beside the settings. A headless run never
/// reaches it (it replays), so no scene or test touches the file.
pub(crate) fn live_transport() -> HttpTransport {
    static LIVE: OnceLock<HttpTransport> = OnceLock::new();
    LIVE.get_or_init(|| HttpTransport::new(crate::settings::cookies_path())).clone()
}

/// The transport and clock a run sends with.
///
/// HTTPS and the wall clock in a window, the recorded answers and the frozen recording time
/// headless. The price data comes over the same [transport](Transport), so a scene replays
/// poe.ninja too.
pub fn target(mode: Mode) -> (TargetTransport, TargetClock) {
    target_for(mode, Site::Poe1)
}

/// [`target`] for one game's trade site and recordings.
pub fn target_for(mode: Mode, site: Site) -> (TargetTransport, TargetClock) {
    match (mode, site) {
        (Mode::Live, _) => (TargetTransport::Live(live_transport()), TargetClock::Live(LiveClock)),
        (Mode::Replay, Site::Poe1) => {
            (TargetTransport::Replay(ReplayTransport::poe1()), TargetClock::Replay(ReplayClock::poe1()))
        }
        (Mode::Replay, Site::Poe2) => {
            (TargetTransport::Replay(ReplayTransport::poe2()), TargetClock::Replay(ReplayClock::poe2()))
        }
    }
}

/// A client that sends to one game's trade site.
pub fn client_for(mode: Mode, host: &str, site: Site, latency_seconds: f64) -> Client {
    debug_assert!(ALLOWED_HOSTS.contains(&host), "{host} is not one of upstream's proxy hosts");
    let (transport, clock) = target_for(mode, site);
    let trade_site = match site {
        Site::Poe1 => TradeSite::Trade,
        Site::Poe2 => TradeSite::Trade2,
    };
    TradeClient::new(transport, clock, host, trade_site, latency_seconds)
}

/// A client behind the lock a worker and the UI thread take turns on.
pub type Shared = Arc<Mutex<Client>>;

pub fn shared(mode: Mode, host: &str, latency_seconds: f64) -> Shared {
    shared_site(mode, host, Site::Poe1, latency_seconds)
}

/// [`shared`] for one game's trade site.
pub fn shared_site(mode: Mode, host: &str, site: Site, latency_seconds: f64) -> Shared {
    Arc::new(Mutex::new(client_for(mode, host, site, latency_seconds)))
}

/// Milliseconds since the epoch, as a window's requests see them: the wall clock in a
/// window, the frozen recording time in a replay.
pub fn now(mode: Mode) -> u64 {
    match mode {
        Mode::Live => LiveClock.now(),
        Mode::Replay => ReplayClock::poe2().now(),
    }
}

/// Locks a [`Shared`] client, ignoring a panic in another search (the state is plain
/// data and stays consistent).
pub fn lock(client: &Shared) -> std::sync::MutexGuard<'_, Client> {
    client.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replay_client_answers_without_a_network() {
        let client = client(Mode::Replay, "www.pathofexile.com", 2.0);
        assert!(matches!(client.transport, TargetTransport::Replay(_)));
        assert!(matches!(client.clock, TargetClock::Replay(_)));
    }
}
