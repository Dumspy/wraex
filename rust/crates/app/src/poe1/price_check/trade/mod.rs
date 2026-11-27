//! `TradeListing.vue` / `TradeBulk.vue`: the listings under the filters block.
//!
//! A search is a [`Query`]: the trade site's search plus the fetches that turn the
//! matching ids into rows (`listing`), or the bulk exchange (`bulk`). Where it runs
//! depends on [`crate::trade::Mode`]: the app sends over HTTPS on a worker and lands
//! the answer in `poll`; scenes, headless renders and tests answer from the recorded
//! screenshots on the spot, so a `--snap` shows the finished table.
//!
//! Like the reference, the newest search wins: an answer carries the generation it was
//! started for and is dropped when a newer search replaced it.

pub mod bulk;
pub mod links;
pub mod listing;
pub mod market_ratio;
pub mod online;

use std::sync::Arc;

use rune::{El, Latest, Waker};
use wraex_core::relative_time::EpochMillis;
use wraex_core::trade::BulkRequest;
use wraex_core::trade::api::{Clock, SearchResult};
use wraex_core::trade::rate_limit::{RateLimiter, RateLimits};
use wraex_poe1::parser::item::Item;
use wraex_poe1::results::{BulkSearch, PricingResult, bulk_searches, pricing_results};
use wraex_poe1::trade::{Api, TradeRequest, api_to_satisfy_search};

use crate::poe1::price_check::PriceCheck;
use crate::poe1::price_check::view::Project;
use crate::trade::{self, Mode, Shared};

/// What a search needs: the request, sent on the thread that runs it.
#[derive(Debug, Clone)]
pub enum Query {
    /// `requestTradeResultList` + `requestResults`.
    Trade { league: String, request: Result<TradeRequest, String>, collapse_merchant: bool },
    /// `execBulkSearch` with the currencies the item trades for.
    Bulk { league: String, request: BulkRequest },
    /// `TradeBulk.vue`'s lazy load of the currency the optimistic search left out.
    BulkLazy { league: String, request: BulkRequest },
}

/// A finished search: `TradeListing.vue`'s `list` and `fetchResults`, or
/// `TradeBulk.vue`'s two currency groups.
#[derive(Debug, Clone)]
pub enum Outcome {
    Trade {
        list: SearchResult,
        rows: Vec<PricingResult>,
        collapse_merchant: bool,
    },
    Bulk(Vec<Option<BulkSearch>>),
    /// The lazy load of one currency (`have`), whose total replaces the optimistic
    /// guess of the other one.
    BulkLazy {
        search: BulkSearch,
        other: BulkCurrency,
    },
}

/// What a worker hands back.
#[derive(Debug, Clone)]
pub struct Answer {
    pub generation: u64,
    pub outcome: Result<Outcome, String>,
}

/// Which of the two exchange tables is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkCurrency {
    Chaos,
    Stable,
}

/// The price-check window's listings.
pub struct Trade {
    mode: Mode,
    client: Shared,
    latest: Latest<Answer>,
    generation: u64,
    /// `TradeListing.vue`'s / `TradeBulk.vue`'s `error`.
    pub error: Option<String>,
    /// `searchResult` of a trade search.
    pub list: Option<SearchResult>,
    /// `fetchResults`, in fetch order (before grouping).
    pub rows: Vec<PricingResult>,
    /// `TradeBulk.vue`'s `result`.
    pub bulk: Option<BulkResults>,
    /// `selectedCurr`.
    pub selected: BulkCurrency,
    /// `collapseMerchant` of the search that produced [`Trade::rows`].
    pub collapse_merchant: bool,
    /// What a currency button rebuilds when its group was not part of the optimistic
    /// search (`TradeBulk.vue`'s `listedLazy`).
    pub lazy_request: Option<LazyBulk>,
}

/// `{ xchgChaos, xchgStable }`.
#[derive(Debug, Clone, Default)]
pub struct BulkResults {
    pub chaos: Option<BulkSearch>,
    pub stable: Option<BulkSearch>,
}

impl BulkResults {
    fn get(&self, which: BulkCurrency) -> Option<&BulkSearch> {
        match which {
            BulkCurrency::Chaos => self.chaos.as_ref(),
            BulkCurrency::Stable => self.stable.as_ref(),
        }
    }

    fn get_mut(&mut self, which: BulkCurrency) -> Option<&mut BulkSearch> {
        match which {
            BulkCurrency::Chaos => self.chaos.as_mut(),
            BulkCurrency::Stable => self.stable.as_mut(),
        }
    }

    fn set(&mut self, which: BulkCurrency, search: BulkSearch) {
        match which {
            BulkCurrency::Chaos => self.chaos = Some(search),
            BulkCurrency::Stable => self.stable = Some(search),
        }
    }
}

impl std::fmt::Debug for Trade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Trade")
            .field("mode", &self.mode)
            .field("generation", &self.generation)
            .field("error", &self.error)
            .field("list", &self.list)
            .field("rows", &self.rows.len())
            .field("bulk", &self.bulk)
            .field("selected", &self.selected)
            .finish()
    }
}

impl Trade {
    pub fn new(mode: Mode, client: Shared) -> Self {
        Self {
            mode,
            client,
            latest: Latest::default(),
            generation: 0,
            error: None,
            list: None,
            rows: Vec::new(),
            bulk: None,
            selected: BulkCurrency::Chaos,
            collapse_merchant: false,
            lazy_request: None,
        }
    }

    /// The waker a landed answer uses to run `AppSpec::poll`.
    pub fn set_waker(&self, waker: Waker) {
        self.latest.set_waker(waker);
    }

    /// A `--snap` scene (`…-rate-limited`): fill the search limit as a burst of searches
    /// just did, so the next search is refused with `Retry after 60 seconds` and the app's
    /// real error path draws the error box's Retry/Browser buttons.
    pub fn simulate_search_rate_limit(&mut self) {
        let mut client = trade::lock(&self.client);
        let now = client.clock.now();
        let mut limits = RateLimits::initial();
        limits.limiters = vec![RateLimiter::new(1, 60.0)];
        limits.acquire(now);
        client.limits.search = limits;
    }

    /// Takes a landed answer, if any: `poll` returns true when it applied one.
    pub fn poll(&mut self) -> bool {
        let Some(answer) = self.latest.take() else { return false };
        self.apply(answer);
        true
    }

    /// Starts `query`, replacing whatever was shown (`useTradeApi`'s `search`).
    pub fn begin(&mut self, query: Query) {
        self.generation += 1;
        let generation = self.generation;
        self.error = None;
        // A lazy load fills one currency of the table already on screen.
        if !matches!(query, Query::BulkLazy { .. }) {
            self.list = None;
            self.rows.clear();
            self.bulk = None;
        }
        match self.mode {
            // Scenes and tests: the answers are files, so land them now and the view
            // that follows the click already shows the table.
            Mode::Replay => {
                let answer = execute(&self.client, generation, &query);
                self.apply(answer);
            }
            Mode::Live => {
                let client = Arc::clone(&self.client);
                self.latest
                    .request((generation, query), move |(generation, query)| execute(&client, generation, &query));
            }
        }
    }

    fn apply(&mut self, answer: Answer) {
        if answer.generation != self.generation {
            return;
        }
        match answer.outcome {
            Err(error) => {
                self.error = Some(error);
                self.list = None;
                self.rows.clear();
                self.bulk = None;
            }
            Ok(Outcome::Trade { list, rows, collapse_merchant }) => {
                self.list = Some(list);
                self.rows = rows;
                self.bulk = None;
                self.collapse_merchant = collapse_merchant;
            }
            Ok(Outcome::Bulk(groups)) => {
                let mut results = BulkResults::default();
                for group in groups.into_iter().flatten() {
                    let which = match group.have_tag.as_str() {
                        "divine" => BulkCurrency::Stable,
                        _ => BulkCurrency::Chaos,
                    };
                    results.set(which, group);
                }
                // `watch(result)`: show the currency with more offers, chaos on a tie.
                self.selected = match (results.chaos.as_ref(), results.stable.as_ref()) {
                    (_, None) => BulkCurrency::Chaos,
                    (None, Some(_)) => BulkCurrency::Stable,
                    (Some(chaos), Some(stable)) => {
                        if stable.total > chaos.total {
                            BulkCurrency::Stable
                        } else {
                            BulkCurrency::Chaos
                        }
                    }
                };
                self.bulk = Some(results);
                self.list = None;
                self.rows.clear();
            }
            Ok(Outcome::BulkLazy { search, other }) => {
                let Some(results) = self.bulk.as_mut() else { return };
                let which = match search.have_tag.as_str() {
                    "divine" => BulkCurrency::Stable,
                    _ => BulkCurrency::Chaos,
                };
                let total = search.total;
                results.set(which, search);
                if let Some(other) = results.get_mut(other) {
                    other.total -= total;
                }
            }
        }
    }

    /// `apiToSatisfySearch` for the current item and active preset.
    pub fn api(&self, check: &PriceCheck) -> Api {
        check.item().map_or(Api::Trade, |item| api_to_satisfy_search(item, check.item_stats()))
    }

    /// `TradeBulk.vue`'s click on a currency: switch to it, loading it when the
    /// optimistic search did not ask for it.
    pub fn select_currency(&mut self, which: BulkCurrency) {
        self.selected = which;
        let missing = self.bulk.as_ref().is_some_and(|results| results.get(which).is_none());
        let Some(request) = self.lazy_request.clone() else { return };
        if missing {
            self.begin(Query::BulkLazy { league: request.league.clone(), request: request.for_currency(which) });
        }
    }
}

/// The bulk request a cargo-style lazy load rebuilds (`execBulkSearch([have])`).
#[derive(Debug, Clone)]
pub struct LazyBulk {
    pub league: String,
    /// The filters and item, kept so a click can rebuild the request.
    pub filters: wraex_poe1::filters::ItemFilters,
    pub item: Item,
}

impl LazyBulk {
    fn for_currency(&self, which: BulkCurrency) -> BulkRequest {
        let have = match which {
            BulkCurrency::Chaos => "chaos",
            BulkCurrency::Stable => "divine",
        };
        wraex_poe1::trade::create_bulk_request(&self.filters, &self.item, vec![have.to_owned()])
    }
}

/// Runs a query: on a worker in the app, on the caller's thread for a replay.
fn execute(client: &Shared, generation: u64, query: &Query) -> Answer {
    let outcome = run(client, query);
    Answer { generation, outcome }
}

fn run(client: &Shared, query: &Query) -> Result<Outcome, String> {
    let mut client = trade::lock(client);
    let now = client.clock.now() as EpochMillis;
    match query {
        Query::Trade { league, request, collapse_merchant } => {
            let request = request.clone()?;
            let list = client.search(&request, league).map_err(|error| error.to_string())?;
            let rows = fetch_rows(&mut client, &list, now, *collapse_merchant)?;
            Ok(Outcome::Trade { list, rows, collapse_merchant: *collapse_merchant })
        }
        Query::Bulk { league, request } => {
            let answer = client
                .exchange::<wraex_poe1::results::ExchangeListing>(request, league)
                .map_err(|error| error.to_string())?;
            let groups = bulk_searches(&answer, &request.query.have, "", now);
            Ok(Outcome::Bulk(groups))
        }
        Query::BulkLazy { league, request } => {
            let answer = client
                .exchange::<wraex_poe1::results::ExchangeListing>(request, league)
                .map_err(|error| error.to_string())?;
            let groups = bulk_searches(&answer, &request.query.have, "", now);
            let search =
                groups.into_iter().flatten().next().ok_or_else(|| "The exchange answer had no offers".to_owned())?;
            let other = match search.have_tag.as_str() {
                "divine" => BulkCurrency::Chaos,
                _ => BulkCurrency::Stable,
            };
            Ok(Outcome::BulkLazy { search, other })
        }
    }
}

/// `requestResults`, first ten and next ten, then `fetchMore` in tens until the table
/// has enough rows (`MIN_NOT_GROUPED`, `MIN_GROUPED`, `API_FETCH_LIMIT`).
fn fetch_rows(
    client: &mut trade::Client,
    list: &SearchResult,
    now: EpochMillis,
    collapse_merchant: bool,
) -> Result<Vec<PricingResult>, String> {
    const MIN_NOT_GROUPED: usize = 7;
    const MIN_GROUPED: usize = 10;
    const API_FETCH_LIMIT: usize = 100;

    let ids = &list.result;
    let mut rows = Vec::new();
    let fetch =
        |client: &mut trade::Client, rows: &mut Vec<PricingResult>, from: usize, to: usize| -> Result<(), String> {
            if from >= to {
                return Ok(());
            }
            let listings = client.fetch(&list.id, &ids[from..to]).map_err(|error| error.to_string())?;
            rows.extend(pricing_results(&listings, "", now));
            Ok(())
        };
    fetch(client, &mut rows, 0, ids.len().min(10))?;
    fetch(client, &mut rows, 10.min(ids.len()), ids.len().min(20))?;

    let mut fetched = 20;
    loop {
        let grouped = listing::grouped(&rows, collapse_merchant);
        let total_grouped = grouped.len();
        let total_not_grouped = grouped.iter().filter(|row| row.listed_times <= 2).count();
        let short = total_not_grouped < MIN_NOT_GROUPED || total_grouped < MIN_GROUPED;
        if !short || fetched >= ids.len() || fetched >= API_FETCH_LIMIT {
            break;
        }
        let end = ids.len().min(fetched + 10);
        fetch(client, &mut rows, fetched, end)?;
        fetched += 10;
    }
    Ok(rows)
}

/// `TradeListing.vue`/`TradeBulk.vue`'s `ui-error-box`: the site's error, with Retry and
/// Browser (which opens the search on the trade site and hides the results).
pub(super) fn error_box<S: 'static>(error: &str, project: Project<S>) -> El {
    let message = if error.contains("JSON") {
        crate::poe1::i18n::t("app.leagues_failed_help")
    } else {
        format!("Error: {error}")
    };
    let retry = crate::ui::button("trade-retry", &crate::poe1::i18n::t("Retry"))
        .focusable(true)
        .on_press(move |s: &mut S| project(s).start_search());
    let browser = crate::ui::button("trade-browser", &crate::poe1::i18n::t("Browser")).focusable(true).on_press_tx(
        move |s: &mut S, tx: &mut rune::Tx| {
            if let Some(url) = project(s).trade_link()
                && let Err(error) = tx.open_url(&url)
            {
                wraex_platform::log!("wraex: cannot open the trade link {url}: {error}");
            }
            project(s).reset_search();
        },
    );
    rune::col().key("trade-error").w_full().mt(crate::theme::rem(0.5)).child(
        crate::ui::error_box(&crate::poe1::i18n::t("trade_result.error"), &message)
            .child(crate::ui::actions([retry, browser])),
    )
}

/// `CheckedItem.vue`'s `trade-listing`/`trade-bulk`: the table for the active API.
///
/// `TradeListing.vue` and `TradeBulk.vue` put their table in a `layout-column
/// overflow-y-auto`, so a long table scrolls under the boxes below it instead of pushing
/// them out of the window.
pub fn view<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let content = match check.trade.api(check) {
        Api::Trade => listing::view(check, project),
        Api::Bulk => bulk::view(check, project),
    };
    rune::col()
        .key("trade-scroll")
        .w_full()
        .shrink()
        .min_h(0.0)
        .scroll(rune::ScrollSpec { stick_bottom: false })
        .child(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The searches the reference screenshots recorded are the searches this app sends:
    /// the replay keys on a hash of the body, so a difference here is a parity bug.
    #[test]
    fn the_recorded_searches_are_the_requests_this_app_sends() {
        let proxy = std::path::Path::new(crate::replay::CORPUS).join("poe1/proxy");
        for slug in ["adv-rare-ring-mirrored-catalyst", "adv-unique-timeless-jewel", "real-map-rare-t16-max-res"] {
            let check = PriceCheck::with_mode(&crate::corpus(slug), Mode::Replay);
            let request = wraex_poe1::trade::create_trade_request(
                crate::poe1::price_check::data(),
                check.item_filters().expect("filters"),
                check.item_stats(),
            )
            .expect("a trade request");
            let body = wraex_core::js::stringify(&request);
            let name = crate::replay::proxy_file_name("www.pathofexile.com/api/trade/search/Standard", Some(&body));
            let file = proxy.join(format!("{name}.POST.json"));
            assert!(file.is_file(), "{slug}: no recording for {name}\n{body}");
        }

        // The bulk exchange: `TradeBulk.vue` overrides the currency for the two orbs.
        let check = PriceCheck::with_mode(&crate::corpus("currency-divine-orb"), Mode::Replay);
        let request = wraex_poe1::trade::create_bulk_request(
            check.item_filters().expect("filters"),
            check.item().expect("the item"),
            vec!["chaos".to_owned()],
        );
        let body = wraex_core::js::stringify(&request);
        let name = crate::replay::proxy_file_name("www.pathofexile.com/api/trade/exchange/Standard", Some(&body));
        let file = proxy.join(format!("{name}.POST.json"));
        assert!(file.is_file(), "currency-divine-orb: no recording for {name}\n{body}");
    }

    /// The app's path: a request runs on a worker and `poll` applies the newest answer.
    #[test]
    fn a_background_search_lands_in_poll() {
        let check = PriceCheck::with_mode(&crate::corpus("adv-rare-ring-mirrored-catalyst"), Mode::Replay);
        let request = wraex_poe1::trade::create_trade_request(
            crate::poe1::price_check::data(),
            check.item_filters().expect("filters"),
            check.item_stats(),
        )
        .expect("a trade request");
        let client = crate::trade::shared(Mode::Replay, "www.pathofexile.com", 2.0);
        let mut trade = Trade::new(Mode::Live, client);
        trade.begin(Query::Trade {
            league: "Standard".into(),
            request: Ok(request),
            collapse_merchant: check.item_filters().expect("filters").trade.collapse_merchant,
        });

        let mut landed = false;
        for _ in 0..300 {
            if trade.poll() {
                landed = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(landed, "the worker's answer reached poll");
        assert_eq!(trade.list.as_ref().map(|list| list.total), Some(10000.0));
        assert_eq!(trade.rows.len(), 20);
        assert!(trade.error.is_none(), "{:?}", trade.error);
    }

    #[test]
    fn a_trade_query_reports_a_request_error() {
        let client = trade::shared(Mode::Replay, "www.pathofexile.com", 2.0);
        let answer = execute(
            &client,
            1,
            &Query::Trade { league: "Standard".into(), request: Err("no trade id".into()), collapse_merchant: false },
        );
        assert_eq!(answer.outcome.unwrap_err(), "no trade id");
    }
}
