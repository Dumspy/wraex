//! `TradeListing.vue` / `TradeBulk.vue`: the listings under the filters block.
//!
//! A search is a [`Query`]: a trade search plus the fetches that turn the matching ids into
//! rows ([`listing`]), or the bulk exchange ([`bulk`]). Where it runs depends on
//! [`crate::trade::Mode`]: the app sends over HTTPS on a worker and lands the answer in
//! `poll`; scenes, headless renders and tests answer from the recorded screenshots on the
//! spot, so a `--snap` shows the finished table.
//!
//! Like the reference, the newest search wins: an answer carries the generation it was
//! started for and is dropped when a newer search replaced it.

pub mod bulk;
pub mod links;
pub mod listing;
pub mod online;

use std::sync::Arc;

use rune::{El, Latest, Waker};
use wraex_core::relative_time::EpochMillis;
use wraex_core::trade::BulkRequest;
use wraex_core::trade::api::{Clock, SearchResult};
use wraex_poe2::filters::ItemFilters;
use wraex_poe2::parser::item::Item;
use wraex_poe2::prices::Prices;
use wraex_poe2::results::PricingResult;
use wraex_poe2::trade::{Api, TradeRequest, api_to_satisfy_search};

use crate::poe2::price_check::PriceCheck;
use crate::poe2::price_check::view::Project;
use crate::trade::{self, Mode, Shared};

/// What a search needs: the request, sent on the thread that runs it. The prices travel
/// with it: the rows' normalised price column is computed from poe.ninja's database, which
/// is small enough to copy per search and keeps the worker free of locks.
#[derive(Debug, Clone)]
pub enum Query {
    /// `requestTradeResultList` + `requestResults`.
    Trade { league: String, request: Result<TradeRequest, String>, prices: Prices },
    /// `execBulkSearch` with the currencies the item trades for.
    Bulk { league: String, request: BulkRequest, prices: Prices },
    /// `TradeBulk.vue`'s lazy load of a currency the optimistic search left out.
    BulkLazy { league: String, request: BulkRequest, prices: Prices },
}

/// A finished search: `TradeListing.vue`'s `list` and `fetchResults`, or `TradeBulk.vue`'s
/// three currency groups.
#[derive(Debug, Clone)]
pub enum Outcome {
    Trade {
        list: SearchResult,
        rows: Vec<PricingResult>,
    },
    Bulk(Vec<Option<bulk::BulkSearch>>),
    /// The lazy load of one currency (`have`), whose total replaces the optimistic
    /// guess of another one.
    BulkLazy {
        search: bulk::BulkSearch,
        other: BulkCurrency,
    },
}

/// What a worker hands back.
#[derive(Debug, Clone)]
pub struct Answer {
    pub generation: u64,
    pub outcome: Result<Outcome, String>,
}

/// Which of the three exchange tables is shown (`xchgExalted`/`xchgChaos`/`xchgStable`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulkCurrency {
    Exalted,
    Chaos,
    Divine,
}

impl BulkCurrency {
    /// `Exalted Orb`, `Chaos Orb`, `Divine Orb`: the `have` currency each table asks for.
    pub const fn have(self) -> &'static str {
        match self {
            BulkCurrency::Exalted => "exalted",
            BulkCurrency::Chaos => "chaos",
            BulkCurrency::Divine => "divine",
        }
    }

    /// The `/images` file beside the table's total: EE2's `exa.png`, `chaos.png`, `divine.png`
    /// (`TradeBulk.vue`'s three `<img>`s; the exalted orb's art is not called `exalted`).
    pub const fn icon(self) -> &'static str {
        match self {
            BulkCurrency::Exalted => "exa.png",
            BulkCurrency::Chaos => "chaos.png",
            BulkCurrency::Divine => "divine.png",
        }
    }

    /// The `have` tag of an exchange group.
    fn of_tag(tag: &str) -> Self {
        match tag {
            "exalted" => BulkCurrency::Exalted,
            "divine" => BulkCurrency::Divine,
            _ => BulkCurrency::Chaos,
        }
    }

    /// The three tables, in template order.
    pub const ALL: [BulkCurrency; 3] = [BulkCurrency::Exalted, BulkCurrency::Chaos, BulkCurrency::Divine];
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
    /// What a currency button rebuilds when its group was not part of the optimistic
    /// search (`TradeBulk.vue`'s `listedLazy`).
    pub lazy_request: Option<LazyBulk>,
}

/// `{ xchgExalted, xchgChaos, xchgStable }`.
#[derive(Debug, Clone, Default)]
pub struct BulkResults {
    pub exalted: Option<bulk::BulkSearch>,
    pub chaos: Option<bulk::BulkSearch>,
    pub divine: Option<bulk::BulkSearch>,
}

impl BulkResults {
    pub fn get(&self, which: BulkCurrency) -> Option<&bulk::BulkSearch> {
        match which {
            BulkCurrency::Exalted => self.exalted.as_ref(),
            BulkCurrency::Chaos => self.chaos.as_ref(),
            BulkCurrency::Divine => self.divine.as_ref(),
        }
    }

    fn get_mut(&mut self, which: BulkCurrency) -> Option<&mut bulk::BulkSearch> {
        match which {
            BulkCurrency::Exalted => self.exalted.as_mut(),
            BulkCurrency::Chaos => self.chaos.as_mut(),
            BulkCurrency::Divine => self.divine.as_mut(),
        }
    }

    fn set(&mut self, which: BulkCurrency, search: bulk::BulkSearch) {
        match which {
            BulkCurrency::Exalted => self.exalted = Some(search),
            BulkCurrency::Chaos => self.chaos = Some(search),
            BulkCurrency::Divine => self.divine = Some(search),
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
            selected: BulkCurrency::Exalted,
            lazy_request: None,
        }
    }

    /// The waker a landed answer uses to run `AppSpec::poll`.
    pub fn set_waker(&self, waker: Waker) {
        self.latest.set_waker(waker);
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
                let answer = execute(self.mode, &self.client, generation, &query);
                self.apply(answer);
            }
            Mode::Live => {
                let client = Arc::clone(&self.client);
                let mode = self.mode;
                self.latest.request((generation, query), move |(generation, query)| {
                    execute(mode, &client, generation, &query)
                });
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
            Ok(Outcome::Trade { list, rows }) => {
                self.list = Some(list);
                self.rows = rows;
                self.bulk = None;
            }
            Ok(Outcome::Bulk(groups)) => {
                let mut results = BulkResults::default();
                for group in groups.into_iter().flatten() {
                    results.set(BulkCurrency::of_tag(&group.have_tag), group);
                }
                // `watch(result)`: show the currency with more offers, exalted (the core
                // currency) on a tie.
                self.selected = match (results.divine.as_ref(), results.exalted.as_ref()) {
                    (None, _) => BulkCurrency::Exalted,
                    (Some(divine), Some(exalted)) => {
                        if divine.total > exalted.total {
                            BulkCurrency::Divine
                        } else {
                            BulkCurrency::Exalted
                        }
                    }
                    (Some(_), None) => BulkCurrency::Divine,
                };
                self.bulk = Some(results);
                self.list = None;
                self.rows.clear();
            }
            Ok(Outcome::BulkLazy { search, other }) => {
                let Some(results) = self.bulk.as_mut() else { return };
                let which = BulkCurrency::of_tag(&search.have_tag);
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
        check.item().map_or(Api::Trade, |item| api_to_satisfy_search(item, check.item_stats(), check.filters()))
    }

    /// `TradeBulk.vue`'s click on a currency: switch to it, loading it when the
    /// optimistic search did not ask for it.
    pub fn select_currency(&mut self, which: BulkCurrency) {
        self.selected = which;
        self.load_missing(which);
    }

    /// `listedLazy`: a currency the optimistic search left out (`loadedOnDemand`) is
    /// asked for on its own when its table is shown.
    pub fn load_missing(&mut self, which: BulkCurrency) {
        let missing = self.bulk.as_ref().is_some_and(|results| results.get(which).is_none());
        let Some(request) = self.lazy_request.clone() else { return };
        if missing {
            self.begin(Query::BulkLazy {
                league: request.league.clone(),
                request: request.for_currency(which),
                prices: request.prices.clone(),
            });
        }
    }
}

/// The bulk request a cargo-style lazy load rebuilds (`execBulkSearch([have])`).
#[derive(Debug, Clone)]
pub struct LazyBulk {
    pub league: String,
    /// The filters and item, kept so a click can rebuild the request.
    pub filters: ItemFilters,
    pub item: Item,
    pub prices: Prices,
}

impl LazyBulk {
    fn for_currency(&self, which: BulkCurrency) -> BulkRequest {
        wraex_poe2::trade::create_bulk_request(&self.filters, &self.item, vec![which.have().to_owned()])
    }
}

/// Runs a query: on a worker in the app, on the caller's thread for a replay.
fn execute(mode: Mode, client: &Shared, generation: u64, query: &Query) -> Answer {
    let outcome = run(mode, client, query);
    Answer { generation, outcome }
}

fn run(mode: Mode, client: &Shared, query: &Query) -> Result<Outcome, String> {
    let data = crate::poe2::price_check::data(mode);
    let mut client = trade::lock(client);
    let now = client.clock.now() as EpochMillis;
    match query {
        Query::Trade { league, request, prices } => {
            let request = request.clone()?;
            let list = client.search(&request, league).map_err(|error| error.to_string())?;
            let rows = fetch_rows(&mut client, data, prices, &list, now)?;
            Ok(Outcome::Trade { list, rows })
        }
        Query::Bulk { league, request, .. } => {
            let answer =
                client.exchange::<bulk::ExchangeListing>(request, league).map_err(|error| error.to_string())?;
            Ok(Outcome::Bulk(bulk::searches(&answer, &request.query.have, "", now)))
        }
        Query::BulkLazy { league, request, .. } => {
            let answer =
                client.exchange::<bulk::ExchangeListing>(request, league).map_err(|error| error.to_string())?;
            let search = bulk::searches(&answer, &request.query.have, "", now)
                .into_iter()
                .flatten()
                .next()
                .ok_or_else(|| "The exchange answer had no offers".to_owned())?;
            let other = match search.have_tag.as_str() {
                "exalted" => BulkCurrency::Chaos,
                "divine" => BulkCurrency::Exalted,
                _ => BulkCurrency::Exalted,
            };
            Ok(Outcome::BulkLazy { search, other })
        }
    }
}

/// `requestResults`, first ten and next ten, then `fetchMore` in tens until the table
/// has enough rows (`MIN_NOT_GROUPED`, `MIN_GROUPED`, `API_FETCH_LIMIT`).
fn fetch_rows(
    client: &mut trade::Client,
    data: &wraex_poe2::data::Data,
    prices: &Prices,
    list: &SearchResult,
    now: EpochMillis,
) -> Result<Vec<PricingResult>, String> {
    const MIN_NOT_GROUPED: usize = 7;
    const MIN_GROUPED: usize = 10;
    const API_FETCH_LIMIT: usize = 100;

    let ids = &list.result;
    let mut prices = prices.clone();
    let mut rows = Vec::new();
    let fetch = |client: &mut trade::Client,
                 prices: &mut Prices,
                 rows: &mut Vec<PricingResult>,
                 from: usize,
                 to: usize|
     -> Result<(), String> {
        if from >= to {
            return Ok(());
        }
        let listings = client.fetch(&list.id, &ids[from..to]).map_err(|error| error.to_string())?;
        rows.extend(wraex_poe2::results::pricing_results(data, prices, &listings, "", now));
        Ok(())
    };
    fetch(client, &mut prices, &mut rows, 0, ids.len().min(10))?;
    fetch(client, &mut prices, &mut rows, 10.min(ids.len()), ids.len().min(20))?;

    let mut fetched = 20;
    loop {
        let grouped = listing::grouped(&rows);
        let total_grouped = grouped.len();
        let total_not_grouped = grouped.iter().filter(|row| row.listed_times <= 2).count();
        let short = total_not_grouped < MIN_NOT_GROUPED || total_grouped < MIN_GROUPED;
        if !short || fetched >= ids.len() || fetched >= API_FETCH_LIMIT {
            break;
        }
        let end = ids.len().min(fetched + 10);
        fetch(client, &mut prices, &mut rows, fetched, end)?;
        fetched += 10;
    }
    Ok(rows)
}

/// `TradeListing.vue`/`TradeBulk.vue`'s `ui-error-box`: the site's error, with Retry and
/// Browser (which opens the search on the trade site and hides the results).
pub(super) fn error_box<S: 'static>(error: &str, project: Project<S>) -> El {
    let retry = crate::ui::button("trade-retry", &crate::poe2::i18n::t("Retry"))
        .min_w(crate::theme::rem(4.0))
        .focusable(true)
        .on_press(move |s: &mut S| project(s).start_search());
    let browser = crate::ui::button("trade-browser", &crate::poe2::i18n::t("Browser"))
        .min_w(crate::theme::rem(4.0))
        .focusable(true)
        .on_press_tx(move |s: &mut S, tx: &mut rune::Tx| {
            if let Some(url) = project(s).trade_link() {
                let _ = tx.open_url(&url);
            }
            project(s).reset_search();
        });
    rune::col().key("trade-error").w_full().mb(crate::theme::rem(1.0)).child(
        crate::ui::error_box(&crate::poe2::i18n::t("trade_result.error"), &format!("Error: {error}"))
            .child(crate::ui::actions([retry, browser])),
    )
}

/// `CheckedItem.vue`'s `trade-listing`/`trade-bulk`: the table for the active API, in the
/// listing's own scroll viewport (`TradeListing.vue`'s `layout-column overflow-y-auto`), so a
/// long table scrolls inside the card instead of drawing past its background.
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

    /// The searches the reference screenshots recorded are the searches this app sends.
    /// The replay keys on a hash of the body, so a difference here is a parity bug — and
    /// because EE2's key order is not the port's struct order, it is a byte comparison,
    /// not a JSON one.
    #[test]
    fn the_recorded_searches_are_the_requests_this_app_sends() {
        let proxy = std::path::Path::new(crate::replay::CORPUS).join("poe2/proxy");
        let cases = [
            ("currency-djinn-barya", "filters.preset_exact"),
            ("ee2-rare-item", "filters.preset_pseudo"),
            ("ee2-unique-item", "filters.preset_pseudo"),
            ("ee2-rare-map", "filters.preset_pseudo"),
        ];
        for (slug, preset) in cases {
            let check = PriceCheck::with_mode(&crate::corpus_poe2(slug), Mode::Replay);
            assert_eq!(check.active_preset().map(|preset| preset.id.as_str()), Some(preset), "{slug}");
            let request = wraex_poe2::trade::create_trade_request(
                crate::poe2::price_check::data(Mode::Replay),
                check.filters(),
                check.item_stats(),
                check.item().expect("the item"),
            )
            .expect("a trade request");
            let body = wraex_core::js::stringify(&request);
            let name = crate::replay::proxy_file_name("www.pathofexile.com/api/trade2/search/Standard", Some(&body));
            let file = proxy.join(format!("{name}.POST.json"));
            assert!(file.is_file(), "{slug}: no recording for {name}\n{body}");
        }
    }

    /// The app's path: a request runs on a worker and `poll` applies the newest answer.
    #[test]
    fn a_background_search_lands_in_poll() {
        let check = PriceCheck::with_mode(&crate::corpus_poe2("ee2-rare-item"), Mode::Replay);
        let request = wraex_poe2::trade::create_trade_request(
            crate::poe2::price_check::data(Mode::Replay),
            check.filters(),
            check.item_stats(),
            check.item().expect("the item"),
        )
        .expect("a trade request");
        let client = crate::trade::shared_site(Mode::Replay, "www.pathofexile.com", crate::trade::Site::Poe2, 2.0);
        let mut trade = Trade::new(Mode::Live, client);
        trade.begin(Query::Trade { league: "Standard".into(), request: Ok(request), prices: check.prices.clone() });

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
        let client = crate::trade::shared_site(Mode::Replay, "www.pathofexile.com", crate::trade::Site::Poe2, 2.0);
        let answer = execute(
            Mode::Replay,
            &client,
            1,
            &Query::Trade { league: "Standard".into(), request: Err("no trade id".into()), prices: Prices::default() },
        );
        assert_eq!(answer.outcome.unwrap_err(), "no trade id");
    }
}
