//! `web/price-check/PriceCheckWindow.vue`: the PoE1 price-check window.
//!
//! The window frame, the title bar and its exchange-rate popover, the item header
//! (`FilterName.vue`), the trend row (`PriceTrend.vue`), the filters block
//! (`FiltersBlock.vue`) and the listings ([`trade`]) are ported. The prices those rows
//! read come from poe.ninja through [`prices`] (`usePoeninja`).

pub mod filters;
pub mod prices;
pub mod trade;
pub mod view;

use std::collections::HashMap;
use std::sync::OnceLock;

use rune::{TextEdit, Waker};
use wraex_core::Rarity;
use wraex_game::ParseError;
use wraex_poe1::data::{Data, reference_source};
use wraex_poe1::filters::create_item_filters::{CreateOptions, category_to_trade_id};
use wraex_poe1::filters::{
    CreatePresetsOptions, FilterError, FilterNumeric, FilterOrGroup, FilterPreset, ItemFilters, Presets, StatFilter,
    create_presets,
};
use wraex_poe1::parser::item::Item;
use wraex_poe1::parser::meta::ItemCategory;
use wraex_poe1::parser::parse_clipboard;
use wraex_poe1::prices::NinjaPrice;
use wraex_poe1::trade::{Api, api_to_satisfy_search, create_bulk_request, create_trade_request};
use wraex_poe1::trends::DetailsId;

use crate::trade::Mode;

/// `#price-window`'s width (`PriceCheckWindow.vue`): `width: 28.75rem`.
pub const WIDTH: f32 = 28.75 * crate::theme::REM;

/// The league a fresh install prices in. Upstream fills in `leagues.selectedId` from the trade
/// site's league list; the reference screenshots (`tools/golden/screens`) select Standard.
pub const DEFAULT_LEAGUE: &str = "Standard";

/// `getTradeEndpoint()` for `en` (`poeWebApi`).
pub const TRADE_HOST: &str = "www.pathofexile.com";

/// `PriceCheckWidget.apiLatencySeconds`: every rate-limit window is that much longer.
pub const API_LATENCY_SECONDS: f64 = 2.0;

/// What the title bar shows before a league is selected (`PriceCheckWindow.vue`'s `title`).
pub const APP_NAME: &str = "Awakened PoE Trade";

/// The `en` game data, loaded once (`init("en")` in the reference).
pub fn data() -> &'static Data {
    static DATA: OnceLock<Data> = OnceLock::new();
    DATA.get_or_init(|| Data::load(&reference_source(), "en").expect("load `en` game data from ref/apt"))
}

/// The options `CheckedItem.vue` hands `createPresets`, at a fresh install's defaults:
/// `PriceCheckWidget.initInstance`'s `merchantOnly: true`, `collapseListings: 'api'`,
/// `activateStockFilter: false`, `searchStatRange: 10`, `defaultCurrency: null`, plus
/// `AppConfig().useIntlSite` (`false`) and the selected league.
pub fn preset_options() -> CreatePresetsOptions {
    preset_options_for(DEFAULT_LEAGUE)
}

/// [`preset_options`] for a league the app selected from the trade site's list
/// (`useLeagues`), instead of a fresh install's [`DEFAULT_LEAGUE`].
pub fn preset_options_for(league: &str) -> CreatePresetsOptions {
    CreatePresetsOptions {
        filters: CreateOptions {
            league: league.to_owned(),
            merchant_only: true,
            currency: None,
            collapse_listings: "api".to_owned(),
            activate_stock_filter: false,
            exact: false,
            use_en: false,
        },
        search_stat_range: 10.0,
    }
}

/// The price-check widget's state.
///
/// Built from the item text the game's copy key produced (`MAIN->CLIENT::item-text` in the
/// reference), so a scene or a test is just [`PriceCheck::from_text`].
#[derive(Debug)]
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
    /// poe.ninja prices for the league (`usePoeninja`): the exchange rate, the trend
    /// rows, the market ratio and the stack values all read them.
    pub prices: prices::PriceList,
    /// The widget is shown. Closing it hides the window, as `wm.hide` does upstream.
    pub open: bool,
    /// `FiltersBlock.vue`'s `statsVisibility.disabled`: Collapse hides the stat rows (the chip
    /// that says "N of M, stats" and the Collapse button both flip it).
    pub stats_collapsed: bool,
    /// `FiltersBlock.vue`'s `showHidden`: show the rows whose `hidden` reason usually keeps
    /// them out of the way.
    pub show_hidden: bool,
    /// `FiltersBlock.vue`'s `showFilterSources`: show each modifier's source (`SourceInfo.vue`).
    pub show_sources: bool,
    /// The filter block's text fields, keyed by [`InputField::key`]. Built from the active
    /// preset by [`PriceCheck::sync_inputs`]; the field's text is the source of truth while a
    /// person types, and each change is parsed back into the filters.
    pub inputs: HashMap<String, TextEdit>,
    /// The listings under the filters block (`TradeListing.vue`/`TradeBulk.vue`).
    pub trade: trade::Trade,
    /// `OnlineFilter.vue`'s popover is open.
    pub online_open: bool,
    /// `PriceCheckWindow.vue`'s exchange-rate popover (`UiPopover` on the `⇄ 891` button).
    pub exchange_open: bool,
    /// `FilterModifier.vue`'s `.mods`: the row whose sources are shown while the pointer is
    /// on it (only with the Mods toggle on).
    pub source_hover: Option<StatPath>,
    /// The field that held the caret at the last poll. Rune has no focus-lost callback on a
    /// text field, so `poll` compares `TextEdit::focused` from frame to frame and runs the
    /// reference's blur rule for the field that lost it.
    focused_field: Option<String>,
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
        // Item art follows the window's switch: a live window fetches from the CDN, a
        // replay reads the screenshot harness's recordings.
        crate::ui::art::set_mode(mode);
        let data = data();
        let item = parse_clipboard(data, text).map_err(|fail| ParseError::Rejected { key: fail.0.into() });
        Self::build(text.to_owned(), item, mode, league)
    }

    /// `MAIN->CLIENT::item-text` with the item already parsed: what the item search sends when
    /// one of its picked entries is pressed (`WidgetItemSearch.vue`'s `selectedItemClick`, which
    /// builds a `createVirtualItem` and `Host.selfDispatch`es it). `parseClipboard` is skipped;
    /// the text is the virtual item's own `rawText`.
    pub fn with_item(item: Item, mode: Mode) -> Self {
        crate::ui::art::set_mode(mode);
        let text = item.raw_text.clone();
        Self::build(text, Ok(item), mode, DEFAULT_LEAGUE)
    }

    /// A window with no item: the live app's idle state before a hotkey brings one. Nothing is
    /// parsed, so the game data is not loaded either.
    pub fn empty(mode: Mode, league: &str) -> Self {
        crate::ui::art::set_mode(mode);
        Self::build(String::new(), Err(ParseError::Rejected { key: "no item".into() }), mode, league)
    }

    /// The window over a parsed item, whatever produced it.
    fn build(text: String, item: Result<Item, ParseError>, mode: Mode, league: &str) -> Self {
        let presets = item.as_ref().ok().map(|item| create_presets(data(), item, &preset_options_for(league)));
        let do_search = match (&item, &presets) {
            (Ok(item), Some(Ok(presets))) => initial_do_search(item, presets),
            _ => false,
        };
        let mut check = Self {
            text,
            item,
            presets,
            do_search,
            league: league.to_owned(),
            prices: prices::PriceList::new(mode),
            open: true,
            stats_collapsed: false,
            show_hidden: false,
            show_sources: false,
            inputs: HashMap::new(),
            trade: trade::Trade::new(mode, crate::trade::shared(mode, TRADE_HOST, API_LATENCY_SECONDS)),
            online_open: false,
            exchange_open: false,
            source_hover: None,
            focused_field: None,
        };
        check.sync_inputs();
        // `PriceCheckWindow.vue`'s `if (item.value.isOk()) queuePricesFetch()`: the trend
        // row, the title bar's rate and the market ratio all need the prices.
        if check.item.is_ok() {
            let league = check.league.clone();
            check.prices.queue(&league, false);
        }
        // `CheckedItem.vue`'s `watch(..., { immediate: true })` starts the search the
        // reference makes by itself (a unique, bulk currency, ...).
        if check.do_search {
            check.search();
        }
        check
    }

    /// The waker a landed live answer uses to run `AppSpec::poll`.
    pub fn set_waker(&self, waker: Waker) {
        self.trade.set_waker(waker.clone());
        self.prices.set_waker(waker);
    }

    /// `AppSpec::poll`: applies a landed search answer and the blur rule; true when
    /// something changed.
    pub fn poll(&mut self) -> bool {
        let landed = self.trade.poll();
        let priced = self.prices.poll();
        let blurred = self.sync_input_focus();
        landed || priced || blurred
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

    /// `OnlineFilter.vue`'s popover.
    pub fn toggle_online_filter(&mut self) {
        self.online_open = !self.online_open;
    }

    /// `PriceCheckWindow.vue`'s `stableOrbCost`: `Math.round(xchgRate)`, or none before
    /// the prices arrived.
    pub fn stable_orb_cost(&self) -> Option<f64> {
        self.prices.xchg_rate().map(|rate| (rate + 0.5).floor())
    }

    /// The `⇄ 891` button's popover.
    pub fn toggle_exchange(&mut self) {
        self.exchange_open = !self.exchange_open;
    }

    /// `getDetailsId(item)`: what the item is on poe.ninja.
    pub fn details_id(&self) -> Option<DetailsId> {
        wraex_poe1::trends::get_details_id(self.item()?)
    }

    /// `findPriceByQuery(getDetailsId(item))`: the item's entry in the price blob.
    pub fn price_info(&self) -> Option<NinjaPrice> {
        self.prices.find_price(&self.details_id()?.query())
    }

    /// `OnlineFilterCore.vue`'s offline toggle: it also opens the listed range up.
    pub fn set_offline(&mut self, offline: bool) {
        if let Some(filters) = self.item_filters_mut() {
            filters.trade.offline = offline;
            filters.trade.listed = if offline { Some("2months".to_owned()) } else { None };
        }
        self.after_trade_change();
    }

    /// `OnlineFilterCore.vue`'s listed-time radio.
    pub fn set_listed(&mut self, listed: Option<&str>) {
        if let Some(filters) = self.item_filters_mut() {
            filters.trade.listed = listed.map(str::to_owned);
        }
        self.after_trade_change();
    }

    /// `OnlineFilterCore.vue`'s merchant-only toggle.
    pub fn toggle_merchant_only(&mut self) {
        if let Some(filters) = self.item_filters_mut() {
            filters.trade.merchant_only = !filters.trade.merchant_only;
        }
        self.after_trade_change();
    }

    /// `OnlineFilterCore.vue`'s in-league toggle (bulk).
    pub fn toggle_online_in_league(&mut self) {
        if let Some(filters) = self.item_filters_mut() {
            filters.trade.online_in_league = !filters.trade.online_in_league;
        }
        self.after_trade_change();
    }

    /// `OnlineFilterCore.vue`'s currency radio.
    pub fn set_currency(&mut self, currency: Option<&str>) {
        if let Some(filters) = self.item_filters_mut() {
            filters.trade.currency = currency.map(str::to_owned);
        }
        self.after_trade_change();
    }

    /// `TradeBulk.vue`'s currency buttons.
    pub fn select_currency(&mut self, which: trade::BulkCurrency) {
        self.trade.select_currency(which);
    }

    /// A trade filter changed: `CheckedItem.vue` re-runs the search
    /// (`watch([item, JSON.stringify(itemFilters.trade)])`).
    fn after_trade_change(&mut self) {
        self.do_search = true;
        self.search();
    }

    /// `CheckedItem.vue`'s `execSearch`: build the request and hand it to the trade state.
    fn search(&mut self) {
        let Some(item) = self.item() else { return };
        let Some(filters) = self.item_filters().cloned() else { return };
        let stats = self.item_stats().to_vec();
        let league = filters.trade.league.clone();
        let query = match api_to_satisfy_search(item, &stats) {
            Api::Trade => {
                self.trade.lazy_request = None;
                let request = create_trade_request(data(), &filters, &stats).map_err(|error| error.to_string());
                trade::Query::Trade { league, request, collapse_merchant: filters.trade.collapse_merchant }
            }
            Api::Bulk => {
                // `TradeBulk.vue`'s optimistic search: at league start many players set the
                // wrong price, so chaos and divine are asked for separately.
                let have = match item.info().ref_name.as_str() {
                    "Chaos Orb" => vec!["divine".to_owned()],
                    "Divine Orb" => vec!["chaos".to_owned()],
                    _ => vec!["divine".to_owned(), "chaos".to_owned()],
                };
                let request = create_bulk_request(&filters, item, have);
                self.trade.lazy_request =
                    Some(trade::LazyBulk { league: league.clone(), filters: filters.clone(), item: item.clone() });
                trade::Query::Bulk { league, request }
            }
        };
        self.trade.begin(query);
    }

    /// `PriceCheckWindow.vue`'s `title`: `leagues.selectedId.value || 'Awakened PoE Trade'`.
    pub fn title(&self) -> &str {
        if self.league.is_empty() { APP_NAME } else { &self.league }
    }

    /// The active preset (`CheckedItem.vue`'s `presets.presets.find(p => p.id === active)`).
    pub fn active_preset(&self) -> Option<&FilterPreset> {
        let presets = self.presets.as_ref()?.as_ref().ok()?;
        presets.presets.iter().find(|preset| preset.id == presets.active)
    }

    /// `itemFilters`: the filters of the active preset.
    pub fn item_filters(&self) -> Option<&ItemFilters> {
        Some(&self.active_preset()?.filters)
    }

    /// `itemStats`: the active preset's `stats`.
    pub fn item_stats(&self) -> &[FilterOrGroup] {
        self.active_preset().map_or(&[], |preset| preset.stats.as_slice())
    }

    /// The parsed item, when the text was one.
    pub fn item(&self) -> Option<&Item> {
        self.item.as_ref().ok()
    }

    /// `FilterName.vue`'s `toggleAccuracy`.
    pub fn toggle_accuracy(&mut self) {
        if let Some(filters) = self.item_filters_mut()
            && let Some(relaxed) = &mut filters.search_relaxed
        {
            relaxed.disabled = Some(!relaxed.disabled.unwrap_or(false));
        }
    }

    /// `FilterName.vue`'s `toggleCorrupted`.
    pub fn toggle_corrupted(&mut self) {
        if let Some(corrupted) = self.item_filters_mut().and_then(|filters| filters.corrupted.as_mut()) {
            corrupted.value = !corrupted.value;
        }
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
    pub fn toggle_stat(&mut self, path: StatPath) {
        if let Some(filter) = self.stat_mut(path) {
            filter.disabled = !filter.disabled;
        }
    }

    /// `FilterModifier.vue`'s `smartToggle`: pressing a row's label.
    ///
    /// A group's label collapses an open, enabled group first; otherwise it toggles, and
    /// enabling a collapsed group opens it.
    pub fn smart_toggle_stat(&mut self, path: StatPath) {
        match path {
            StatPath::Meta(group) => {
                let Some(FilterOrGroup::Group(filter_group)) =
                    self.item_stats_mut().and_then(|stats| stats.get_mut(group))
                else {
                    return;
                };
                if !filter_group.meta.disabled && filter_group.expanded {
                    filter_group.expanded = false;
                } else {
                    filter_group.meta.disabled = !filter_group.meta.disabled;
                    if !filter_group.meta.disabled {
                        filter_group.expanded = true;
                    }
                }
            }
            _ => self.toggle_stat(path),
        }
    }

    /// `FilterModifier.vue`'s `toggleExpanded`.
    pub fn toggle_group_expanded(&mut self, group: usize) {
        if let Some(FilterOrGroup::Group(filter_group)) = self.item_stats_mut().and_then(|stats| stats.get_mut(group)) {
            filter_group.expanded = !filter_group.expanded;
        }
    }

    /// `FilterModifierOptions.vue`'s `handleOptionClick`: choosing another option enables the
    /// filter, choosing the checked one toggles it.
    pub fn click_stat_option(&mut self, path: StatPath, value: f64) {
        let Some(filter) = self.stat_mut(path) else { return };
        if filter.option.as_ref().is_some_and(|option| option.value == value) {
            filter.disabled = !filter.disabled;
        } else {
            filter.option = Some(wraex_poe1::filters::ValueFilter { value });
            filter.disabled = false;
        }
    }

    /// A modifier's min or max field changed: write the parsed number back into the filter and
    /// enable it, as focusing the input does upstream (`inputFocus` sets `disabled = false`).
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
            InputField::Stat(path, NumericPart::Min) => {
                if let Some(roll) = self.stat_mut(path).and_then(|filter| filter.roll.as_mut()) {
                    roll.min = value;
                }
                if let Some(filter) = self.stat_mut(path) {
                    filter.disabled = false;
                }
            }
            InputField::Stat(path, NumericPart::Max) => {
                if let Some(roll) = self.stat_mut(path).and_then(|filter| filter.roll.as_mut()) {
                    roll.max = value;
                }
                if let Some(filter) = self.stat_mut(path) {
                    filter.disabled = false;
                }
            }
        }
    }

    /// The numeric field's chip (a `FilterBtnNumeric.vue`).
    pub fn numeric(&self, which: NumericFilter) -> Option<&FilterNumeric> {
        numeric(self.item_filters()?, which)
    }

    /// `FilterBtnNumeric.vue`'s `filter.disabled = !filter.disabled`.
    pub fn toggle_numeric(&mut self, which: NumericFilter) {
        if let Some(filter) = self.numeric_mut(which) {
            filter.disabled = !filter.disabled;
        }
    }

    /// `FilterBtnLogical.vue`'s `toggle`.
    pub fn toggle_logical(&mut self, which: LogicalToggle) {
        let Some(filters) = self.item_filters_mut() else { return };
        match which {
            LogicalToggle::SearchSub => {
                let relaxed = filters.search_relaxed.as_ref().is_some_and(|filter| filter.disabled != Some(true));
                let search = if relaxed { filters.search_relaxed.as_mut() } else { Some(&mut filters.search_exact) };
                if let Some(sub) = search.and_then(|search| search.sub.as_mut()) {
                    sub.disabled = Some(!sub.disabled.unwrap_or(false));
                }
            }
            LogicalToggle::NotCatalysed => flip(filters.not_catalysed.as_mut().map(|filter| &mut filter.disabled)),
            LogicalToggle::Foil => flip(filters.foil.as_mut().map(|filter| &mut filter.disabled)),
            LogicalToggle::Unidentified => {
                flip(filters.unidentified.as_mut().map(|filter| &mut filter.disabled));
            }
            LogicalToggle::Veiled => flip(filters.veiled.as_mut().map(|filter| &mut filter.disabled)),
            LogicalToggle::Mirrored => flip(filters.mirrored.as_mut().map(|filter| &mut filter.disabled)),
            LogicalToggle::Split => flip(filters.split.as_mut().map(|filter| &mut filter.disabled)),
            LogicalToggle::RarityMagic => flip(filters.rarity.as_mut().map(|filter| &mut filter.disabled)),
            LogicalToggle::Influence(index) => {
                if let Some(influence) = filters.influences.as_mut().and_then(|list| list.get_mut(index)) {
                    influence.disabled = !influence.disabled;
                }
            }
        }
    }

    /// The active preset's stat at `path`.
    pub fn stat(&self, path: StatPath) -> Option<&StatFilter> {
        let stats = self.item_stats();
        match path {
            StatPath::Top(index) => match stats.get(index)? {
                FilterOrGroup::Stat(filter) => Some(filter),
                FilterOrGroup::Group(_) => None,
            },
            StatPath::Meta(group) => match stats.get(group)? {
                FilterOrGroup::Group(filter_group) => Some(&filter_group.meta),
                FilterOrGroup::Stat(_) => None,
            },
            StatPath::Child(group, index) => match stats.get(group)? {
                FilterOrGroup::Group(filter_group) => filter_group.stats.get(index),
                FilterOrGroup::Stat(_) => None,
            },
        }
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
        if let Some(filters) = self.item_filters() {
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
        for (path, filter) in stats_with_paths(self.item_stats()) {
            if let Some(roll) = &filter.roll {
                wanted.push((InputField::Stat(path, NumericPart::Min).key(), roll.min.map(number).unwrap_or_default()));
                wanted.push((InputField::Stat(path, NumericPart::Max).key(), roll.max.map(number).unwrap_or_default()));
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

    fn item_stats_mut(&mut self) -> Option<&mut Vec<FilterOrGroup>> {
        Some(&mut self.active_preset_mut()?.stats)
    }

    fn stat_mut(&mut self, path: StatPath) -> Option<&mut StatFilter> {
        let stats = self.item_stats_mut()?;
        match path {
            StatPath::Top(index) => match stats.get_mut(index)? {
                FilterOrGroup::Stat(filter) => Some(filter),
                FilterOrGroup::Group(_) => None,
            },
            StatPath::Meta(group) => match stats.get_mut(group)? {
                FilterOrGroup::Group(filter_group) => Some(&mut filter_group.meta),
                FilterOrGroup::Stat(_) => None,
            },
            StatPath::Child(group, index) => match stats.get_mut(group)? {
                FilterOrGroup::Group(filter_group) => filter_group.stats.get_mut(index),
                FilterOrGroup::Stat(_) => None,
            },
        }
    }

    fn numeric_mut(&mut self, which: NumericFilter) -> Option<&mut FilterNumeric> {
        let filters = self.item_filters_mut()?;
        match which {
            NumericFilter::LinkedSockets => filters.linked_sockets.as_mut(),
            NumericFilter::WhiteSockets => filters.white_sockets.as_mut(),
            NumericFilter::Quality => filters.quality.as_mut(),
            NumericFilter::GemLevel => filters.gem_level.as_mut(),
            NumericFilter::MapTier => filters.map_tier.as_mut(),
            NumericFilter::ItemLevel => filters.item_level.as_mut(),
            NumericFilter::StackSize => filters.stack_size.as_mut(),
            NumericFilter::AreaLevel => filters.area_level.as_mut(),
            NumericFilter::SentinelCharge => filters.sentinel_charge.as_mut(),
        }
    }

    fn item_filters_mut(&mut self) -> Option<&mut ItemFilters> {
        Some(&mut self.active_preset_mut()?.filters)
    }
}

/// A row's position in the active preset's `stats` (`FilterOrGroup`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatPath {
    /// A plain `StatFilter`.
    Top(usize),
    /// A `FilterGroup`'s own row (`group.meta`).
    Meta(usize),
    /// One of a `FilterGroup`'s rows (`group.stats[i]`).
    Child(usize, usize),
}

impl StatPath {
    /// The key the view names the row's elements with: `0`, `1m` (the second group's own row)
    /// or `1-0` (its first child).
    pub fn key(self) -> String {
        match self {
            StatPath::Top(index) => index.to_string(),
            StatPath::Meta(group) => format!("{group}m"),
            StatPath::Child(group, index) => format!("{group}-{index}"),
        }
    }
}

/// Which `FilterNumeric` of `ItemFilters` a chip edits; `FiltersBlock.vue`'s
/// `filter-btn-numeric` list, in template order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericFilter {
    LinkedSockets,
    MapTier,
    AreaLevel,
    SentinelCharge,
    ItemLevel,
    StackSize,
    WhiteSockets,
    GemLevel,
    Quality,
}

impl NumericFilter {
    /// The template's order.
    pub const ALL: [NumericFilter; 9] = [
        NumericFilter::LinkedSockets,
        NumericFilter::MapTier,
        NumericFilter::AreaLevel,
        NumericFilter::SentinelCharge,
        NumericFilter::ItemLevel,
        NumericFilter::StackSize,
        NumericFilter::WhiteSockets,
        NumericFilter::GemLevel,
        NumericFilter::Quality,
    ];

    /// The key its chip and fields are named with (upstream's `item.*` label key).
    pub const fn key(self) -> &'static str {
        match self {
            NumericFilter::LinkedSockets => "linked_sockets",
            NumericFilter::MapTier => "map_tier",
            NumericFilter::AreaLevel => "area_level",
            NumericFilter::SentinelCharge => "sentinel_charge",
            NumericFilter::ItemLevel => "item_level",
            NumericFilter::StackSize => "stock",
            NumericFilter::WhiteSockets => "white_sockets",
            NumericFilter::GemLevel => "gem_level",
            NumericFilter::Quality => "quality",
        }
    }
}

/// `FiltersBlock.vue`'s `filter-btn-logical` list (minus the stats chip's own collapse state).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalToggle {
    /// The active search's `sub` (`searchSub`).
    SearchSub,
    NotCatalysed,
    /// `filters.influences[i]`.
    Influence(usize),
    /// `filters.rarity`, shown as "Magic" when its value is `magic`.
    RarityMagic,
    Unidentified,
    Veiled,
    Foil,
    Mirrored,
    Split,
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
    Stat(StatPath, NumericPart),
}

impl InputField {
    /// The element key and the [`PriceCheck::inputs`] key.
    pub fn key(self) -> String {
        match self {
            InputField::Numeric(which, part) => format!("btn-{}-{}", which.key(), part.key()),
            InputField::Stat(path, part) => format!("stat-{}-{}", path.key(), part.key()),
        }
    }
}

/// `numeric`: the field the chip edits, when the preset has it.
fn numeric(filters: &ItemFilters, which: NumericFilter) -> Option<&FilterNumeric> {
    match which {
        NumericFilter::LinkedSockets => filters.linked_sockets.as_ref(),
        NumericFilter::MapTier => filters.map_tier.as_ref(),
        NumericFilter::AreaLevel => filters.area_level.as_ref(),
        NumericFilter::SentinelCharge => filters.sentinel_charge.as_ref(),
        NumericFilter::ItemLevel => filters.item_level.as_ref(),
        NumericFilter::StackSize => filters.stack_size.as_ref(),
        NumericFilter::WhiteSockets => filters.white_sockets.as_ref(),
        NumericFilter::GemLevel => filters.gem_level.as_ref(),
        NumericFilter::Quality => filters.quality.as_ref(),
    }
}

/// Every row with a min/max field, groups included, in the order the block draws them.
fn stats_with_paths(stats: &[FilterOrGroup]) -> Vec<(StatPath, &StatFilter)> {
    let mut out = Vec::new();
    for (index, stat) in stats.iter().enumerate() {
        match stat {
            FilterOrGroup::Stat(filter) => out.push((StatPath::Top(index), filter)),
            FilterOrGroup::Group(group) => {
                out.push((StatPath::Meta(index), &group.meta));
                for (child, filter) in group.stats.iter().enumerate() {
                    out.push((StatPath::Child(index, child), filter));
                }
            }
        }
    }
    out
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

/// `CheckedItem.vue`'s first `doSearch`, with the widget's defaults (`smartInitialSearch: true`,
/// `lockedInitialSearch: true`) and the reference's `focusOverlay: false` (`advancedCheck`).
///
/// Anything the trade site can't search by name alone, and anything whose active preset is the
/// bulk exchange, searches straight away; a rare that searches by category waits for the button.
fn initial_do_search(item: &Item, presets: &Presets) -> bool {
    let bulk = presets.active == "filters.preset_bulk";
    let special = matches!(
        item.category.as_ref(),
        Some(
            ItemCategory::HeistContract
                | ItemCategory::HeistBlueprint
                | ItemCategory::SanctumRelic
                | ItemCategory::Charm
                | ItemCategory::Idol
        )
    );
    // `!CATEGORY_TO_TRADE_ID.has(item.category) && refName !== 'Mercenary Warrant'`
    let no_trade_category =
        item.category.as_ref().and_then(category_to_trade_id).is_none() && !item.ref_name_is("Mercenary Warrant");
    item.rarity == Some(Rarity::Unique)
        || bulk
        || item.map_completion_reward.is_some()
        || special
        || item.ref_name_is("Viscera")
        || no_trade_category
        || item.is_unidentified
        || item.is_veiled == Some(true)
}
