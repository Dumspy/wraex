//! The price data behind the window: `Prices.ts`'s `usePoeninja` with the app's
//! transport.
//!
//! The blob is one request per league, sent through the same [`Transport`] the trade
//! client uses, so a scene or a test replays the recorded answer and a window fetches
//! it over HTTPS. Gathering the blob runs on Rune's [`Latest`] worker, like a
//! search, and lands in `PriceCheck::poll`.

use rune::{Latest, Waker};
use wraex_core::leagues::SelectedLeague;
use wraex_core::trade::api::{Clock, Transport};
use wraex_poe1::prices::{DbQuery, NinjaPrice, Prices};

use crate::trade::{self, Mode, TargetClock, TargetTransport};

/// One request's answer: poe.ninja's blob, or the transport's error.
type Answer = Result<String, String>;

/// The league a price lookup runs for. Upstream's `leagues.selected`; the app has one
/// league so far (`DEFAULT_LEAGUE`), which the reference screenshots select.
fn selected_league(league: &str) -> SelectedLeague {
    SelectedLeague { id: league.to_owned(), realm: "pc-ggg".to_owned(), is_popular: true }
}

/// `usePoeninja`: the blob, the exchange rate and the lookups over it.
pub struct PriceList {
    mode: Mode,
    transport: TargetTransport,
    clock: TargetClock,
    prices: Prices,
    answer: Latest<Answer>,
    /// `isLoading`: a request is on its way (or, in a replay, was just asked for).
    loading: bool,
}

impl std::fmt::Debug for PriceList {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PriceList")
            .field("mode", &self.mode)
            .field("xchg_rate", &self.prices.xchg_rate)
            .field("empty", &self.prices.is_empty())
            .finish_non_exhaustive()
    }
}

impl PriceList {
    pub fn new(mode: Mode) -> Self {
        let (transport, clock) = trade::target(mode);
        Self { mode, transport, clock, prices: Prices::default(), answer: Latest::default(), loading: false }
    }

    /// The waker a landed answer uses to run `AppSpec::poll`.
    pub fn set_waker(&self, waker: Waker) {
        self.answer.set_waker(waker);
    }

    /// `queuePricesFetch`: the user looked at an item, so keep the prices fresh and
    /// load them when they are missing or stale.
    pub fn queue(&mut self, league: &str, force: bool) {
        if self.mode == Mode::Replay {
            self.queue_selected(Some(&selected_league(league)), force);
        }
    }

    pub fn queue_selected(&mut self, league: Option<&SelectedLeague>, force: bool) {
        let now = self.clock.now();
        self.prices.mark_interest(now);
        let Some(request) = self.prices.request(league, force, now) else { return };
        self.loading = true;
        match self.mode {
            // The blob is a file, so it lands now and the view that follows shows it.
            Mode::Replay => match self.transport.send(&request) {
                Ok(response) => self.apply(Ok(response.body)),
                Err(error) => self.apply(Err(error)),
            },
            Mode::Live => {
                let transport = self.transport.clone();
                self.answer.request((), move |()| transport.send(&request).map(|response| response.body));
            }
        }
    }

    /// Takes a landed answer, if any: `poll` returns true when it applied one.
    pub fn poll(&mut self) -> bool {
        let Some(answer) = self.answer.take() else { return false };
        self.apply(answer);
        true
    }

    fn apply(&mut self, answer: Answer) {
        self.loading = false;
        if let Ok(blob) = answer {
            self.prices.loaded(&blob, self.clock.now());
        }
    }

    /// `xchgRate`: chaos per divine, once a price for one arrived.
    pub const fn xchg_rate(&self) -> Option<f64> {
        self.prices.xchg_rate
    }

    /// `initialLoading`: a request is out and nothing is cached yet.
    pub const fn initial_loading(&self) -> bool {
        self.loading && self.prices.is_empty()
    }

    /// `findPriceByQuery`
    pub fn find_price(&self, query: &DbQuery) -> Option<NinjaPrice> {
        self.prices.find_price(query)
    }

    /// The loaded prices, for the arithmetic built on them (`autoCurrency`, ...).
    pub const fn prices(&self) -> &Prices {
        &self.prices
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A replay answers from `corpus/screens/poe1/proxy/`, and the divine rate is 891
    /// chaos once rounded.
    #[test]
    fn a_replay_loads_the_recorded_blob() {
        let mut prices = PriceList::new(Mode::Replay);
        prices.queue("Standard", false);
        assert_eq!(prices.xchg_rate(), Some(890.9));
        let divine = prices.find_price(&DbQuery { ns: "ITEM", name: "Divine Orb", variant: None }).expect("a price");
        assert_eq!(divine.info.chaos, 890.9);
        assert_eq!(divine.info.graph.len(), 7);
        // Fresh prices are not asked for again.
        prices.queue("Standard", false);
        assert!(!prices.initial_loading());
    }
}
