//! The PoE2 price check: `PriceCheckWindow.vue` and `CheckedItem.vue` state.
//!
//! EE2's window differs from APT's in its chips (no search-sub, "not catalysed" or "split"
//! chip; runes, spirit and ascendancy points instead, and an item-editor button), in the
//! stats block (a flat `StatFilter[]` with elemental-dps option buttons and a roll slider,
//! where APT has `FilterOrGroup`s) and in the bulk table (three currencies). Everything
//! under it — the trade client, the recorded answers, the palette — is shared with the
//! PoE1 window.

pub mod filters;
pub mod prices;
pub mod trade;
pub mod view;

use std::collections::HashMap;
use std::sync::OnceLock;

use rune::{Latest, TextEdit, Waker};
use wraex_core::Rarity;
use wraex_game::ParseError;
use wraex_poe2::data::{Data, TradeData, source};
use wraex_poe2::filters::create_item_filters::{CreateOptions, category_to_trade_id};
use wraex_poe2::filters::{
    CreatePresetsOptions, FilterError, FilterNumeric, FilterPreset, ItemFilters, Presets, StatFilter, create_presets,
};
use wraex_poe2::parser::item::Item;
use wraex_poe2::parser::parse_clipboard;
use wraex_poe2::prices::Prices;
use wraex_poe2::trade::{Api, api_to_satisfy_search, create_bulk_request, create_trade_request};

use crate::trade::{Mode, Site};

/// `#price-window`'s width (`PriceCheckWindow.vue`): `width: 28.75rem`.
pub const WIDTH: f32 = 28.75 * crate::theme::REM;

/// The league a fresh install prices in. EE2 fills in `leagues.selectedId` from the trade
/// site's league list; the reference screenshots (`tools/golden/screens`) select Standard.
pub const DEFAULT_LEAGUE: &str = "Standard";

/// `getTradeEndpoint()` for `en` (`poeWebApi`).
pub const TRADE_HOST: &str = "www.pathofexile.com";

/// `PriceCheckWidget.apiLatencySeconds`: every rate-limit window is that much longer.
pub const API_LATENCY_SECONDS: f64 = 2.0;

/// What the title bar shows before a league is selected (`PriceCheckWindow.vue`'s `title`).
pub const APP_NAME: &str = "Exiled Exchange 2";

/// The trade site data EE2 fetches at startup (`TradeData.ts`).
///
/// The parser and `createPresets` read the trade site's stat ids, so the data has to match
/// what the reference saw. A replay reads the answers the screenshot harness recorded
/// (`corpus/screens/poe2/proxy`); the app fetches them from the trade site. When that fails
/// the app goes on without them, as upstream's `TradeData.load` keeps its empty sets.
pub fn data(mode: Mode) -> &'static Data {
    static REPLAY: OnceLock<Data> = OnceLock::new();
    static LIVE: OnceLock<Data> = OnceLock::new();
    match mode {
        Mode::Replay => REPLAY.get_or_init(|| load_with(Some(recorded_trade_data()))),
        Mode::Live => LIVE.get_or_init(|| {
            let trade = live_trade_data()
                .inspect_err(|error| wraex_platform::log!("wraex: fetching the trade site's data failed ({error})"))
                .ok();
            load_with(trade)
        }),
    }
}

fn load_with(trade: Option<TradeData>) -> Data {
    Data::load(&source(), "en", trade).expect("load `en` game data from the assets root")
}

/// `www.pathofexile.com/api/trade2/data/{items,stats}`, as the screenshots recorded them.
fn recorded_trade_data() -> TradeData {
    let dir = format!("{}/poe2/proxy", crate::replay::CORPUS);
    let read = |what: &str| {
        let path = format!("{dir}/www.pathofexile.com__api__trade2__data__{what}.GET.json");
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
    };
    TradeData::from_json(&read("items"), &read("stats")).expect("EE2's recorded trade data parses")
}

/// `loadItemData`/`loadStatData`: the trade site's items and stats over HTTPS.
fn live_trade_data() -> Result<TradeData, String> {
    use wraex_core::trade::api::{HttpRequest, Method, Transport};

    let transport = crate::trade::live_transport();
    let get = |what: &str| -> Result<String, String> {
        let request = HttpRequest {
            method: Method::Get,
            url: format!("https://{TRADE_HOST}/api/trade2/data/{what}"),
            headers: Vec::new(),
            body: None,
        };
        let response = transport.send(&request)?;
        if response.status != 200 {
            return Err(format!("GET /api/trade2/data/{what} answered {}", response.status));
        }
        Ok(response.body)
    };
    TradeData::from_json(&get("items")?, &get("stats")?).map_err(|e| e.to_string())
}

/// The options `CheckedItem.vue` hands `createPresets`, at a fresh install's defaults:
/// `PriceCheckWidget.initInstance`'s `collapseListings: 'api'`, `activateStockFilter: false`,
/// `searchStatRange: 10`, `defaultAllSelected: false`, `rememberCurrency`/`rememberListingType`
/// off (so no currency and the `securable` listing type), plus `AppConfig().useIntlSite`
/// (`false`) and the selected league.
pub fn preset_options() -> CreatePresetsOptions {
    preset_options_for(DEFAULT_LEAGUE)
}

/// [`preset_options`] for the league the app selected from the trade site's list
/// (`useLeagues`), instead of a fresh install's [`DEFAULT_LEAGUE`].
pub fn preset_options_for(league: &str) -> CreatePresetsOptions {
    CreatePresetsOptions {
        filters: CreateOptions {
            league: league.to_owned(),
            currency: None,
            listing_type: None,
            collapse_listings: "api".to_owned(),
            activate_stock_filter: false,
            exact: false,
            use_en: false,
        },
        search_stat_range: 10.0,
        default_all_selected: false,
    }
}

/// The price-check widget's state.
///
/// Built from the item text the game's copy key produced (`MAIN->CLIENT::item-text` in the
/// reference), so a scene or a test is just [`PriceCheck::from_text`].
pub struct PriceCheck {
    /// The clipboard text the window was opened with: the raw block the error view shows.
    pub text: String,
    /// `parseClipboard(text)`.
    pub item: Result<Item, ParseError>,
    /// `createPresets(item, …)`, one preset per way of searching the item. `None` when the
    /// item did not parse (the reference never reaches `CheckedItem` then).
    pub presets: Option<Result<Presets, FilterError>>,
    /// `doSearch` (`CheckedItem.vue`): the results replace the Search button once set.
    pub do_search: bool,
    /// `leagues.selectedId`: the title bar shows it.
    pub league: String,
    /// The widget is shown. Closing it hides the window, as `wm.hide` does upstream.
    pub open: bool,
    /// `FiltersBlock.vue`'s `statsVisibility.disabled`: Collapse hides the stat rows.
    pub stats_collapsed: bool,
    /// `FiltersBlock.vue`'s `showHidden`.
    pub show_hidden: bool,
    /// `FiltersBlock.vue`'s `showFilterSources`.
    pub show_sources: bool,
    /// The filter block's text fields, keyed by [`InputField::key`]. Built from the active
    /// preset by [`PriceCheck::sync_inputs`]; a field's text is the source of truth while a
    /// person types, and each change is parsed back into the filters.
    pub inputs: HashMap<String, TextEdit>,
    /// The listings under the filters block (`TradeListing.vue`/`TradeBulk.vue`).
    pub trade: trade::Trade,
    /// `usePoeninja`: the prices the trend row and the normalised price column read.
    pub prices: Prices,
    /// A live download of poe.ninja's overviews, landing in `poll`.
    prices_latest: Latest<Result<Vec<(&'static str, String)>, String>>,
    pub(crate) market: Option<wraex_core::leagues::SelectedLeague>,
    prices_requested: bool,
    /// `OnlineFilter.vue`'s popover is open.
    pub online_open: bool,
    /// `FilterModifier.vue`'s `.mods`: the row whose sources are shown while the pointer is
    /// on it (only with the Mods toggle on).
    pub source_hover: Option<usize>,
    /// The field that held the caret at the last poll, for the chip blur rule.
    focused_field: Option<String>,
    mode: Mode,
}

impl std::fmt::Debug for PriceCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PriceCheck")
            .field("item", &self.item.as_ref().map(|_| "ok"))
            .field("do_search", &self.do_search)
            .field("league", &self.league)
            .field("open", &self.open)
            .field("rows", &self.trade.rows.len())
            .field("prices", &self.prices.xchg_rate)
            .finish()
    }
}

impl PriceCheck {
    /// Parse `text` and build the presets the window opens with, searching over HTTPS when
    /// the reference would search by itself.
    pub fn from_text(text: &str) -> Self {
        Self::with_mode(text, Mode::for_run())
    }

    /// [`PriceCheck::from_text`] with the transport chosen: recorded answers for a scene,
    /// test or headless render.
    pub fn with_mode(text: &str, mode: Mode) -> Self {
        Self::with_mode_league(text, mode, DEFAULT_LEAGUE)
    }

    /// [`PriceCheck::with_mode`] in the league the app selected from the trade site's list.
    pub fn with_mode_league(text: &str, mode: Mode, league: &str) -> Self {
        Self::with_client(text, mode, league, Self::new_client(mode))
    }

    pub(crate) fn new_client(mode: Mode) -> crate::trade::Shared {
        crate::trade::shared_site(mode, TRADE_HOST, Site::Poe2, API_LATENCY_SECONDS)
    }

    pub(crate) fn with_client(text: &str, mode: Mode, league: &str, client: crate::trade::Shared) -> Self {
        // Item art follows the window's switch: a live window fetches from the CDN, a
        // replay reads the screenshot harness's recordings.
        crate::ui::art::set_mode(mode);
        let data = data(mode);
        let item = parse_clipboard(data, text).map_err(|fail| ParseError::Rejected { key: fail.0 });
        Self::build(text.to_owned(), item, mode, league, client)
    }

    /// `MAIN->CLIENT::item-text` with the item already parsed: what the item search sends when
    /// one of its picked entries is pressed (`WidgetItemSearch.vue`'s `starredItemClick`, which
    /// builds a `createVirtualItem` and `Host.selfDispatch`es it). `parseClipboard` is skipped;
    /// the text is the virtual item's own `rawText`.
    pub(crate) fn with_item(item: Item, mode: Mode, client: crate::trade::Shared) -> Self {
        crate::ui::art::set_mode(mode);
        let text = item.raw_text.clone();
        Self::build(text, Ok(item), mode, DEFAULT_LEAGUE, client)
    }

    /// A window with no item: the live app's idle state before a hotkey brings one. Nothing is
    /// parsed, so the trade data is not fetched either.
    pub fn empty(mode: Mode, league: &str) -> Self {
        crate::ui::art::set_mode(mode);
        Self::build(
            String::new(),
            Err(ParseError::Rejected { key: "no item".into() }),
            mode,
            league,
            Self::new_client(mode),
        )
    }

    /// The window over a parsed item, whatever produced it.
    fn build(
        text: String,
        item: Result<Item, ParseError>,
        mode: Mode,
        league: &str,
        client: crate::trade::Shared,
    ) -> Self {
        let presets = item.as_ref().ok().map(|item| create_presets(data(mode), item, &preset_options_for(league)));
        let do_search = match (&item, &presets) {
            (Ok(item), Some(Ok(presets))) => initial_do_search(item, presets),
            _ => false,
        };
        let prices = prices::load(mode);
        let mut check = Self {
            text,
            item,
            presets,
            do_search,
            league: league.to_owned(),
            open: true,
            stats_collapsed: false,
            show_hidden: false,
            show_sources: false,
            inputs: HashMap::new(),
            trade: trade::Trade::new(mode, client),
            prices,
            prices_latest: Latest::default(),
            market: None,
            prices_requested: false,
            online_open: false,
            source_hover: None,
            focused_field: None,
            mode,
        };
        // `CheckedItem.vue`'s watch forces online listings before a bulk search.
        if check
            .item()
            .is_some_and(|item| api_to_satisfy_search(item, check.item_stats(), check.filters()) == Api::Bulk)
            && let Some(filters) = check.filters_mut()
        {
            filters.trade.listing_type = "online".to_owned();
        }
        check.sync_inputs();
        // `CheckedItem.vue`'s `watch(..., { immediate: true })` starts the search the
        // reference makes by itself (a unique, a trial coin, ...).
        if check.do_search {
            check.search();
        }
        check
    }

    /// The transport this window was built with.
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The waker a landed live answer uses to run `AppSpec::poll`.
    pub fn set_waker(&self, waker: Waker) {
        self.trade.set_waker(waker.clone());
        self.prices_latest.set_waker(waker);
    }

    /// The startup prefetch's answer, applied before the window asks: the shell fetches
    /// poe.ninja once at launch (`usePoeninja` does it for the whole app), so a price check that
    /// opens later shows its rates without waiting for its own request.
    pub fn apply_price_overviews(&mut self, overviews: &[(&'static str, String)]) {
        if overviews.is_empty() {
            return;
        }
        self.prices_requested = true;
        let overviews: Vec<(&str, &str)> = overviews.iter().map(|(kind, body)| (*kind, body.as_str())).collect();
        let now = crate::trade::now(self.mode);
        self.prices.loaded(data(self.mode), &overviews, now);
    }

    /// `AppSpec::poll`: applies a landed search or price answer and the blur rule; true
    /// when something changed.
    pub fn poll(&mut self) -> bool {
        let landed = self.trade.poll();
        let priced = self.poll_prices();
        let blurred = self.sync_input_focus();
        landed || priced || blurred
    }

    /// `usePoeninja`'s download, landing from the worker: the overviews are parsed into
    /// [`PriceCheck::prices`] once they are all here.
    fn poll_prices(&mut self) -> bool {
        let Some(answer) = self.prices_latest.take() else {
            // Nothing landed. A live window asks once; a replay loaded its prices up front.
            if self.mode == Mode::Live
                && !self.prices_requested
                && self.prices.is_empty()
                && self.item().is_some()
                && let Some(league) = self.market.clone()
            {
                self.prices_requested = true;
                self.prices_latest.request(league, |league| prices::fetch(&league));
            }
            return false;
        };
        match answer {
            Ok(overviews) => {
                let overviews: Vec<(&str, &str)> =
                    overviews.iter().map(|(kind, body)| (*kind, body.as_str())).collect();
                let now = crate::trade::now(self.mode);
                self.prices.loaded(data(self.mode), &overviews, now);
            }
            Err(error) => wraex_platform::log!("wraex: fetching poe.ninja prices failed: {error}"),
        }
        true
    }

    /// `FilterBtnNumeric.vue`'s `@blur="inputMinBlur"`: Rune keeps `TextEdit::focused` for
    /// us but has no per-field blur handler, so a change of the focused field between polls
    /// is the blur.
    fn sync_input_focus(&mut self) -> bool {
        let focused = self.inputs.iter().find(|(_, edit)| edit.focused).map(|(key, _)| key.clone());
        let previous = std::mem::replace(&mut self.focused_field, focused.clone());
        match previous {
            Some(previous) if focused.as_deref() != Some(previous.as_str()) => self.blur_input(&previous),
            _ => false,
        }
    }

    /// An empty or non-numeric chip min field shows `0` and turns its filter off.
    fn blur_input(&mut self, key: &str) -> bool {
        let Some(which) =
            NumericFilter::ALL.into_iter().find(|which| InputField::Numeric(*which, NumericPart::Min).key() == key)
        else {
            return false;
        };
        if self.input(key).is_none_or(|edit| parse_number(&edit.text).is_some()) {
            return false;
        }
        if let Some(edit) = self.inputs.get_mut(key) {
            edit.replace_text("0");
        }
        if let Some(filter) = self.numeric_mut(which) {
            filter.value = 0.0;
            filter.disabled = true;
        }
        self.filters_changed();
        true
    }

    /// The current search's trade-site URL (`makeTradeLink`).
    pub fn trade_link(&self) -> Option<String> {
        trade::links::link(self)
    }

    /// `CheckedItem.vue`'s `@reset`: hide the results, back to the Search row.
    pub fn reset_search(&mut self) {
        self.do_search = false;
    }

    /// `CheckedItem.vue`'s deep watch on `[item, doSearch, itemStats, itemFilters]`: once the
    /// results are up, any change to the stats or the filters hides them again ("force user to
    /// press Search button on change"). A trade-option change is the exception: the next watch
    /// (see [`PriceCheck::after_trade_change`]) starts the search itself.
    fn filters_changed(&mut self) {
        self.do_search = false;
    }

    /// `OnlineFilter.vue`'s popover.
    pub fn toggle_online_filter(&mut self) {
        self.online_open = !self.online_open;
    }

    /// `OnlineFilter.vue`'s listing-type radio (`filters.trade.listingType`).
    pub fn set_listing_type(&mut self, value: &str) {
        if let Some(filters) = self.filters_mut() {
            filters.trade.listing_type = value.to_owned();
        }
        self.after_trade_change();
    }

    /// `OnlineFilter.vue`'s listed-time radio.
    pub fn set_listed(&mut self, listed: Option<&str>) {
        if let Some(filters) = self.filters_mut() {
            filters.trade.listed = listed.map(str::to_owned);
        }
        self.after_trade_change();
    }

    /// `OnlineFilter.vue`'s currency radio.
    pub fn set_currency(&mut self, currency: Option<&str>) {
        if let Some(filters) = self.filters_mut() {
            filters.trade.currency = currency.map(str::to_owned);
        }
        self.after_trade_change();
    }

    /// `TradeBulk.vue`'s currency buttons.
    pub fn select_currency(&mut self, which: trade::BulkCurrency) {
        self.trade.select_currency(which);
    }

    /// A trade filter changed: `CheckedItem.vue`'s second watch
    /// (`watch([item, JSON.stringify(itemFilters.trade)])`) sets `doSearch = true` again, which
    /// the search watch turns into a new search. The deep watch has just hidden the old results
    /// ([`PriceCheck::filters_changed`]); this puts them back with the new options.
    fn after_trade_change(&mut self) {
        self.do_search = true;
        self.search();
    }

    /// `CheckedItem.vue`'s `execSearch`: build the request and hand it to the trade state.
    fn search(&mut self) {
        let Some(item) = self.item().cloned() else { return };
        let filters = self.filters().clone();
        let stats = self.item_stats().to_vec();
        let league = filters.trade.league.clone();
        let query = match api_to_satisfy_search(&item, &stats, &filters) {
            Api::Trade => {
                self.trade.lazy_request = None;
                let request =
                    create_trade_request(data(self.mode), &filters, &stats, &item).map_err(|error| error.to_string());
                trade::Query::Trade { league, request, prices: self.prices.clone() }
            }
            Api::Bulk => {
                // `CheckedItem.vue` forces online listings before a bulk search.
                let mut filters = filters.clone();
                filters.trade.listing_type = "online".to_owned();
                let ref_name = item.info().ref_name.clone().unwrap_or_default();
                let request = create_bulk_request(&filters, &item, optimistic_have(&ref_name));
                self.trade.lazy_request =
                    Some(trade::LazyBulk { league: league.clone(), filters, item, prices: self.prices.clone() });
                trade::Query::Bulk { league, request, prices: self.prices.clone() }
            }
        };
        self.trade.begin(query);
    }

    /// `PriceCheckWindow.vue`'s `title`: `leagues.selectedId.value || 'Exiled Exchange 2'`.
    pub fn title(&self) -> &str {
        if self.league.is_empty() { APP_NAME } else { &self.league }
    }

    /// The active preset (`CheckedItem.vue`'s `presets.presets.find(p => p.id === active)`).
    pub fn active_preset(&self) -> Option<&FilterPreset> {
        let presets = self.presets.as_ref()?.as_ref().ok()?;
        presets.presets.iter().find(|preset| preset.id == presets.active)
    }

    /// `itemFilters`: the filters of the active preset.
    pub fn filters(&self) -> &ItemFilters {
        static EMPTY: OnceLock<ItemFilters> = OnceLock::new();
        self.active_preset().map_or_else(|| EMPTY.get_or_init(ItemFilters::default), |preset| &preset.filters)
    }

    /// `itemStats`: the active preset's `stats`.
    pub fn item_stats(&self) -> &[StatFilter] {
        self.active_preset().map_or(&[], |preset| preset.stats.as_slice())
    }

    /// The parsed item, when the text was one.
    pub fn item(&self) -> Option<&Item> {
        self.item.as_ref().ok()
    }

    /// `FilterName.vue`'s `toggleAccuracy`.
    pub fn toggle_accuracy(&mut self) {
        if let Some(filters) = self.filters_mut()
            && let Some(relaxed) = &mut filters.search_relaxed
        {
            relaxed.disabled = Some(!relaxed.disabled.unwrap_or(false));
        }
        self.filters_changed();
    }

    /// `FilterName.vue`'s corrupted switch.
    pub fn toggle_corrupted(&mut self) {
        if let Some(corrupted) = self.filters_mut().and_then(|filters| filters.corrupted.as_mut()) {
            corrupted.value = !corrupted.value;
        }
        self.filters_changed();
    }

    /// The Search button (and Enter in a field): `doSearch = true`, which fetches the
    /// listings instead, and the search itself.
    pub fn start_search(&mut self) {
        self.do_search = true;
        self.search();
    }

    /// The close button (`closePriceCheck`): upstream hides the widget.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// `FiltersBlock.vue`'s `selectPreset`: switch the active preset, which replaces the rows
    /// (and so their text fields).
    pub fn select_preset(&mut self, id: &str) {
        let Some(Ok(presets)) = self.presets.as_mut() else { return };
        if !presets.presets.iter().any(|preset| preset.id == id) {
            return;
        }
        presets.active = id.to_owned();
        self.sync_inputs();
        // `CheckedItem.vue`'s watch on the active preset: the results belong to the old
        // one, so the Search button comes back.
        self.do_search = false;
    }

    /// `statsVisibility.disabled = !statsVisibility.disabled` (Collapse, and the chip with
    /// "N of M, stats").
    pub fn toggle_stats_collapsed(&mut self) {
        self.stats_collapsed = !self.stats_collapsed;
    }

    /// `showHidden = !showHidden`.
    pub fn toggle_show_hidden(&mut self) {
        self.show_hidden = !self.show_hidden;
        self.sync_inputs();
    }

    /// `showFilterSources = !showFilterSources`.
    pub fn toggle_show_sources(&mut self) {
        self.show_sources = !self.show_sources;
    }

    /// `FilterModifier.vue`'s `toggleFilter`: the checkbox.
    pub fn toggle_stat(&mut self, index: usize) {
        if let Some(filter) = self.stat_mut(index) {
            filter.disabled = !filter.disabled;
        }
        self.filters_changed();
    }

    /// `FilterModifierItemIsElemental.vue`'s `select`: choosing an element enables the filter
    /// and swaps in that element's roll (`additionalInfo.elementalInfo`).
    pub fn select_element(&mut self, index: usize, which: ElementalOption) {
        let Some(data) = self.stat(index).and_then(|filter| filter.additional_info.as_ref()) else { return };
        let Some(info) = data.elemental_info.as_ref() else { return };
        let roll = info.get(which.stat_ref()).cloned();
        if let Some(filter) = self.stat_mut(index) {
            filter.option = Some(wraex_poe2::filters::FilterOption { value: Some(which as u8 as f64) });
            filter.disabled = false;
            if let Some(roll) = roll {
                filter.roll = Some(roll);
            }
        }
        self.filters_changed();
    }

    /// `FilterModifier.vue`'s `inputFocus`: an emptied field takes the roll's default.
    pub fn apply_input(&mut self, field: InputField, text: &str) {
        let value = parse_number(text);
        match field {
            InputField::Numeric(which, NumericPart::Min) => {
                if let Some(filter) = self.numeric_mut(which) {
                    filter.value = value.unwrap_or(0.0);
                    filter.disabled = false;
                }
            }
            InputField::Numeric(which, NumericPart::Max) => {
                if let Some(filter) = self.numeric_mut(which) {
                    filter.max = value;
                    filter.disabled = false;
                }
            }
            InputField::Stat(index, NumericPart::Min) => {
                if let Some(roll) = self.stat_mut(index).and_then(|filter| filter.roll.as_mut()) {
                    roll.min = value;
                }
                if let Some(filter) = self.stat_mut(index) {
                    filter.disabled = false;
                }
            }
            InputField::Stat(index, NumericPart::Max) => {
                if let Some(roll) = self.stat_mut(index).and_then(|filter| filter.roll.as_mut()) {
                    roll.max = value;
                }
                if let Some(filter) = self.stat_mut(index) {
                    filter.disabled = false;
                }
            }
        }
        self.filters_changed();
    }

    /// The numeric field's chip (a `FilterBtnNumeric.vue`).
    pub fn numeric(&self, which: NumericFilter) -> Option<&FilterNumeric> {
        numeric(self.filters(), which)
    }

    /// `FilterBtnNumeric.vue`'s `filter.disabled = !filter.disabled`.
    pub fn toggle_numeric(&mut self, which: NumericFilter) {
        if let Some(filter) = self.numeric_mut(which) {
            filter.disabled = !filter.disabled;
        }
        self.filters_changed();
    }

    /// `FilterBtnLogical.vue`'s `toggle` for the chips that are not `FilterNumeric`s.
    pub fn toggle_logical(&mut self, which: LogicalToggle) {
        let Some(filters) = self.filters_mut() else { return };
        match which {
            LogicalToggle::UnidentifiedTier => {
                if let Some(tier) = filters.unidentified_tier.as_mut() {
                    tier.disabled = !tier.disabled;
                }
            }
            LogicalToggle::Unidentified => {
                if let Some(unidentified) = filters.unidentified.as_mut() {
                    unidentified.disabled = !unidentified.disabled;
                }
            }
            LogicalToggle::UltimatumHint => {
                if let Some(hint) = filters.ultimatum_hint.as_mut() {
                    hint.disabled = !hint.disabled;
                }
            }
            // Read-only chips (`readonly` in `FiltersBlock.vue`'s `filter-btn-logical`).
            LogicalToggle::Blighted | LogicalToggle::RarityMagic | LogicalToggle::RarityNormal => {}
        }
        self.filters_changed();
    }

    /// `FilterBtnLogical.vue`'s toggle for the chips backed by a `{ disabled }` filter.
    pub fn set_item_flag(&mut self, which: ItemFlag) {
        let Some(filters) = self.filters_mut() else { return };
        match which {
            ItemFlag::Veiled => flip(filters.veiled.as_mut().map(|filter| &mut filter.disabled)),
            ItemFlag::Foil => flip(filters.foil.as_mut().map(|filter| &mut filter.disabled)),
            ItemFlag::Mirrored => flip(filters.mirrored.as_mut().map(|filter| &mut filter.disabled)),
            ItemFlag::Sanctified => flip(filters.sanctified.as_mut().map(|filter| &mut filter.disabled)),
        }
        self.filters_changed();
    }

    /// The active preset's stat at `index`.
    pub fn stat(&self, index: usize) -> Option<&StatFilter> {
        self.item_stats().get(index)
    }

    /// The text field keyed `key`, when the active preset built one.
    pub fn input(&self, key: &str) -> Option<&TextEdit> {
        self.inputs.get(key)
    }

    /// The text field keyed `key`, to hand Rune's `.on_edit(..)`.
    pub fn input_mut(&mut self, key: &str) -> &mut TextEdit {
        self.inputs.entry(key.to_owned()).or_default()
    }

    /// Create a text field for every number the active preset shows, seeded from the filters,
    /// and drop the ones another preset or a smaller filter list left behind. Called when the
    /// window opens and when the preset changes; typing then keeps the field's own text.
    pub fn sync_inputs(&mut self) {
        let mut wanted: Vec<(String, String)> = Vec::new();
        if let Some(filters) = self.active_preset().map(|preset| &preset.filters) {
            for which in NumericFilter::ALL {
                if let Some(filter) = numeric(filters, which) {
                    wanted.push((InputField::Numeric(which, NumericPart::Min).key(), number(filter.value)));
                    if filter.max.is_some() {
                        wanted.push((
                            InputField::Numeric(which, NumericPart::Max).key(),
                            filter.max.map(number).unwrap_or_default(),
                        ));
                    }
                }
            }
        }
        for (index, filter) in self.item_stats().iter().enumerate() {
            if let Some(roll) = &filter.roll {
                wanted
                    .push((InputField::Stat(index, NumericPart::Min).key(), roll.min.map(number).unwrap_or_default()));
                wanted
                    .push((InputField::Stat(index, NumericPart::Max).key(), roll.max.map(number).unwrap_or_default()));
            }
        }
        self.inputs.retain(|key, _| wanted.iter().any(|(wanted, _)| wanted == key));
        for (key, text) in wanted {
            self.inputs.entry(key).or_insert_with(|| TextEdit::new(text));
        }
    }

    fn active_preset_mut(&mut self) -> Option<&mut FilterPreset> {
        let presets = self.presets.as_mut()?.as_mut().ok()?;
        let active = presets.active.clone();
        presets.presets.iter_mut().find(|preset| preset.id == active)
    }

    fn stat_mut(&mut self, index: usize) -> Option<&mut StatFilter> {
        self.active_preset_mut()?.stats.get_mut(index)
    }

    fn numeric_mut(&mut self, which: NumericFilter) -> Option<&mut FilterNumeric> {
        let filters = self.filters_mut()?;
        match which {
            NumericFilter::SocketNumber => filters.socket_number.as_mut(),
            NumericFilter::LinkedSockets => filters.linked_sockets.as_mut(),
            NumericFilter::MapTier => filters.map_tier.as_mut(),
            NumericFilter::AscendancyPoints => filters.awarded_ascendancy_points.as_mut(),
            NumericFilter::AreaLevel => filters.area_level.as_mut(),
            NumericFilter::WingsRevealed => filters.heist_wings_revealed.as_mut(),
            NumericFilter::SentinelCharge => filters.sentinel_charge.as_mut(),
            NumericFilter::ItemLevel => filters.item_level.as_mut(),
            NumericFilter::RequiresLevel => filters.requires.as_mut()?.level.as_mut(),
            NumericFilter::StackSize => filters.stack_size.as_mut(),
            NumericFilter::WhiteSockets => filters.white_sockets.as_mut(),
            NumericFilter::AugmentSockets => filters.augment_sockets.as_mut(),
            NumericFilter::GemLevel => filters.gem_level.as_mut(),
            NumericFilter::Quality => filters.quality.as_mut(),
        }
    }

    fn filters_mut(&mut self) -> Option<&mut ItemFilters> {
        Some(&mut self.active_preset_mut()?.filters)
    }
}

/// A chip the port toggles through a `{ disabled }` filter rather than a `FilterNumeric`
/// (`veiled`, `foil`, `mirrored`, `sanctified`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemFlag {
    Veiled,
    Foil,
    Mirrored,
    Sanctified,
}

/// `ItemIsElementalModifier`: which element an `item.elemental_dps` option selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElementalOption {
    Any = 0,
    Fire = 1,
    Cold = 2,
    Lightning = 3,
}

impl ElementalOption {
    pub const ALL: [ElementalOption; 4] =
        [ElementalOption::Any, ElementalOption::Fire, ElementalOption::Cold, ElementalOption::Lightning];

    /// The i18n key of its button (`FilterModifierItemIsElemental.vue`).
    pub const fn label(self) -> &'static str {
        match self {
            ElementalOption::Any => "any",
            ElementalOption::Fire => "fire",
            ElementalOption::Cold => "cold",
            ElementalOption::Lightning => "lightning",
        }
    }

    /// `INTERNAL_TRADE_IDS[12 + value]`: the affix pseudo stat whose roll the option selects.
    pub const fn stat_ref(self) -> &'static str {
        match self {
            ElementalOption::Any => "item.has_elemental_affix",
            ElementalOption::Fire => "item.has_elemental_fire_affix",
            ElementalOption::Cold => "item.has_elemental_cold_affix",
            ElementalOption::Lightning => "item.has_elemental_lightning_affix",
        }
    }
}

/// Which `FilterNumeric` of `ItemFilters` a chip edits; `FiltersBlock.vue`'s
/// `filter-btn-numeric` list, in template order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericFilter {
    SocketNumber,
    LinkedSockets,
    MapTier,
    AscendancyPoints,
    AreaLevel,
    WingsRevealed,
    SentinelCharge,
    ItemLevel,
    RequiresLevel,
    StackSize,
    WhiteSockets,
    AugmentSockets,
    GemLevel,
    Quality,
}

impl NumericFilter {
    /// The template's order.
    pub const ALL: [NumericFilter; 14] = [
        NumericFilter::SocketNumber,
        NumericFilter::LinkedSockets,
        NumericFilter::MapTier,
        NumericFilter::AscendancyPoints,
        NumericFilter::AreaLevel,
        NumericFilter::WingsRevealed,
        NumericFilter::SentinelCharge,
        NumericFilter::ItemLevel,
        NumericFilter::RequiresLevel,
        NumericFilter::StackSize,
        NumericFilter::WhiteSockets,
        NumericFilter::AugmentSockets,
        NumericFilter::GemLevel,
        NumericFilter::Quality,
    ];

    /// The key its chip and fields are named with.
    pub const fn key(self) -> &'static str {
        match self {
            NumericFilter::SocketNumber => "gem_sockets",
            NumericFilter::LinkedSockets => "linked_sockets",
            NumericFilter::MapTier => "map_tier",
            NumericFilter::AscendancyPoints => "ascendancy_points",
            NumericFilter::AreaLevel => "area_level",
            NumericFilter::WingsRevealed => "heist_wings_revealed",
            NumericFilter::SentinelCharge => "sentinel_charge",
            NumericFilter::ItemLevel => "item_level",
            NumericFilter::RequiresLevel => "requires_level",
            NumericFilter::StackSize => "stock",
            NumericFilter::WhiteSockets => "white_sockets",
            NumericFilter::AugmentSockets => "augment_sockets",
            NumericFilter::GemLevel => "gem_level",
            NumericFilter::Quality => "quality",
        }
    }
}

/// `FiltersBlock.vue`'s `filter-btn-logical` list (minus the stats chip's own collapse state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalToggle {
    /// `filters.mapBlighted`, read-only.
    Blighted,
    /// `filters.rarity.value === 'magic'`, read-only.
    RarityMagic,
    /// `filters.rarity.value === 'normal'`, read-only.
    RarityNormal,
    UnidentifiedTier,
    Unidentified,
    UltimatumHint,
}

/// Which half of a min/max pair an input edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericPart {
    Min,
    Max,
}

impl NumericPart {
    const fn key(self) -> &'static str {
        match self {
            NumericPart::Min => "min",
            NumericPart::Max => "max",
        }
    }
}

/// A text field in the filter block. Its key both names the element (`stat-0-min`,
/// `btn-item_level-min`) and finds the [`TextEdit`] in [`PriceCheck::inputs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    /// A chip's number (`FilterBtnNumeric.vue`).
    Numeric(NumericFilter, NumericPart),
    /// A modifier row's roll (`FilterModifier.vue`'s `.rollInput`).
    Stat(usize, NumericPart),
}

impl InputField {
    /// The element key and the [`PriceCheck::inputs`] key.
    pub fn key(self) -> String {
        match self {
            InputField::Numeric(which, part) => format!("btn-{}-{}", which.key(), part.key()),
            InputField::Stat(index, part) => format!("stat-{index}-{}", part.key()),
        }
    }
}

/// `numeric`: the field the chip edits, when the preset has it.
fn numeric(filters: &ItemFilters, which: NumericFilter) -> Option<&FilterNumeric> {
    match which {
        NumericFilter::SocketNumber => filters.socket_number.as_ref(),
        NumericFilter::LinkedSockets => filters.linked_sockets.as_ref(),
        NumericFilter::MapTier => filters.map_tier.as_ref(),
        NumericFilter::AscendancyPoints => filters.awarded_ascendancy_points.as_ref(),
        NumericFilter::AreaLevel => filters.area_level.as_ref(),
        NumericFilter::WingsRevealed => filters.heist_wings_revealed.as_ref(),
        NumericFilter::SentinelCharge => filters.sentinel_charge.as_ref(),
        NumericFilter::ItemLevel => filters.item_level.as_ref(),
        NumericFilter::RequiresLevel => filters.requires.as_ref()?.level.as_ref(),
        NumericFilter::StackSize => filters.stack_size.as_ref(),
        NumericFilter::WhiteSockets => filters.white_sockets.as_ref(),
        NumericFilter::AugmentSockets => filters.augment_sockets.as_ref(),
        NumericFilter::GemLevel => filters.gem_level.as_ref(),
        NumericFilter::Quality => filters.quality.as_ref(),
    }
}

/// `!disabled` on an optional field.
fn flip(disabled: Option<&mut bool>) {
    if let Some(disabled) = disabled {
        *disabled = !*disabled;
    }
}

/// The field text upstream seeds an input with: `String(number)` (so `84`, not `84.0`).
fn number(value: f64) -> String {
    wraex_core::js::number_to_string(value)
}

/// `v-model.number`'s parse: an empty or non-numeric field has no number.
fn parse_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    text.parse::<f64>().ok().filter(|value| value.is_finite())
}

/// `bulk-api.ts`'s optimistic `have` list: at league start many players set the wrong
/// price, so the three orbs ask for the others separately.
fn optimistic_have(ref_name: &str) -> Vec<String> {
    let have: &[&str] = match ref_name {
        "Exalted Orb" => &["divine", "chaos"],
        "Chaos Orb" => &["divine", "exalted"],
        "Divine Orb" => &["exalted", "chaos"],
        _ => &["divine", "exalted", "chaos"],
    };
    have.iter().map(|s| (*s).to_owned()).collect()
}

/// `CheckedItem.vue`'s first `doSearch`, with the widget's defaults
/// (`smartInitialSearch: true`, `lockedInitialSearch: true`) and the reference's
/// `focusOverlay: false` (`advancedCheck`).
///
/// A unique, anything the trade site cannot search by its category, an unidentified or a
/// veiled item searches straight away; a rare bow or a waystone waits for the button.
fn initial_do_search(item: &Item, presets: &Presets) -> bool {
    let no_trade_category =
        item.category.as_ref().map(|category| category_to_trade_id(category).is_none()).unwrap_or(true);
    let category_is = |want: wraex_poe2::parser::meta::ItemCategory| item.category.as_ref() == Some(&want);
    let _ = presets;
    item.rarity == Some(Rarity::Unique)
        || category_is(wraex_poe2::parser::meta::ItemCategory::HeistBlueprint)
        || category_is(wraex_poe2::parser::meta::ItemCategory::SanctumRelic)
        || category_is(wraex_poe2::parser::meta::ItemCategory::Charm)
        || no_trade_category
        || item.is_unidentified
        || item.is_veiled == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_replay_window_parses_and_opens_with_its_prices() {
        let check = PriceCheck::with_mode(&crate::corpus_poe2("ee2-rare-item"), Mode::Replay);
        assert!(check.item.is_ok());
        assert_eq!(check.active_preset().map(|preset| preset.id.as_str()), Some("filters.preset_pseudo"));
        // `xchgRate` of the recorded prices, rounded: the title bar's `132`.
        assert_eq!(check.prices.xchg_rate, Some(131.7));
    }

    #[test]
    fn the_elemental_option_swaps_in_that_elements_roll() {
        let mut check = PriceCheck::with_mode(&crate::corpus_poe2("ee2-rare-item"), Mode::Replay);
        assert_eq!(check.stat(1).and_then(|stat| stat.option).and_then(|option| option.value), Some(0.0));
        check.select_element(1, ElementalOption::Lightning);
        let stat = check.stat(1).expect("the elemental dps row");
        assert_eq!(stat.option.and_then(|option| option.value), Some(3.0));
        assert!(!stat.disabled);
        // The roll is the affix pseudo stat's, so it differs from the "Any" row's.
        assert!(stat.roll.is_some());
    }

    /// `CheckedItem.vue`'s two watches: a stat change after a search hides the results (the
    /// Search button comes back), while a trade option restarts the search itself.
    #[test]
    fn a_stat_change_waits_for_search_and_a_trade_change_searches() {
        let mut check = PriceCheck::with_mode(&crate::corpus_poe2("ee2-unique-item"), Mode::Replay);
        assert!(check.do_search, "a unique searches by itself");
        assert!(!check.trade.rows.is_empty(), "the recorded listing rows");

        check.toggle_stat(0);
        assert!(!check.do_search, "the stat changed: the results are stale");

        check.set_listing_type("any");
        assert!(check.do_search, "the trade option change re-searched");
        assert!(!check.trade.rows.is_empty() || check.trade.error.is_some(), "a new request went out");
    }
}
