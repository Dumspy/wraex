//! The trade API client the app sends with, chosen at startup.
//!
//! A window sends over HTTPS (`wraex_platform::http::HttpTransport`) from a worker
//! thread. A headless run (`--snap`, `--scene`, `--script`) and the tests instead
//! replay the answers recorded for the reference screenshots (`crate::replay`), so
//! they need no network and their listing ages are frozen. Both paths go through the
//! same [`wraex_core::trade::api::TradeClient`], so rate limits, caching and error
//! handling are the app's real ones.

use std::sync::{Arc, Mutex};
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
        if rune::is_headless() { Mode::Replay } else { Mode::Live }
    }
}

impl Default for Mode {
    fn default() -> Self {
        Mode::for_run()
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
            TargetTransport::Live(transport) => transport.send(request),
            TargetTransport::Replay(transport) => transport.send(request),
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
            TargetClock::Live(clock) => clock.now(),
            TargetClock::Replay(clock) => clock.now(),
        }
    }

    fn sleep(&self, millis: u64) {
        match self {
            TargetClock::Live(clock) => clock.sleep(millis),
            // Nothing to wait for: the answers are already on disk.
            TargetClock::Replay(clock) => clock.sleep(millis),
        }
    }
}

/// Milliseconds since the Unix epoch; `sleep` really waits.
#[derive(Debug, Clone, Copy, Default)]
pub struct LiveClock;

impl Clock for LiveClock {
    fn now(&self) -> u64 {
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
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
    let (transport, clock) = match mode {
        Mode::Live => (TargetTransport::Live(HttpTransport::default()), TargetClock::Live(LiveClock)),
        Mode::Replay => (TargetTransport::Replay(ReplayTransport::poe1()), TargetClock::Replay(ReplayClock::poe1())),
    };
    TradeClient::new(transport, clock, host, TradeSite::Trade, latency_seconds)
}

/// A client behind the lock a worker and the UI thread take turns on.
pub type Shared = Arc<Mutex<Client>>;

pub fn shared(mode: Mode, host: &str, latency_seconds: f64) -> Shared {
    Arc::new(Mutex::new(client(mode, host, latency_seconds)))
}

/// Locks a [`Shared`] client, ignoring a panic in another search (the state is plain
/// data and stays consistent).
pub fn lock(client: &Shared) -> std::sync::MutexGuard<'_, Client> {
    client.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
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
