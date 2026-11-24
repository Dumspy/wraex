//! `web/price-check/PriceCheckWindow.vue`: the PoE1 price-check window.
//!
//! Filters, results and trends are still to come (PLAN phase 5): the view draws the frame and
//! the item header, and stands in for the rest with placeholders. The state already carries
//! everything those steps read, so they build on it instead of replacing it.

pub mod view;

use std::sync::OnceLock;

use wraex_core::Rarity;
use wraex_game::ParseError;
use wraex_poe1::data::{Data, reference_source};
use wraex_poe1::filters::create_item_filters::{CreateOptions, category_to_trade_id};
use wraex_poe1::filters::{CreatePresetsOptions, FilterError, FilterPreset, ItemFilters, Presets, create_presets};
use wraex_poe1::parser::item::Item;
use wraex_poe1::parser::meta::ItemCategory;
use wraex_poe1::parser::parse_clipboard;

/// `#price-window`'s width (`PriceCheckWindow.vue`): `width: 28.75rem`.
pub const WIDTH: f32 = 28.75 * crate::theme::REM;

/// The league a fresh install prices in. Upstream fills in `leagues.selectedId` from the trade
/// site's league list; the reference screenshots (`tools/golden/screens`) select Standard.
pub const DEFAULT_LEAGUE: &str = "Standard";

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
    CreatePresetsOptions {
        filters: CreateOptions {
            league: DEFAULT_LEAGUE.to_owned(),
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
    /// The widget is shown. Closing it hides the window, as `wm.hide` does upstream.
    pub open: bool,
}

impl PriceCheck {
    /// Parse `text` and build the presets the window opens with.
    pub fn from_text(text: &str) -> Self {
        let data = data();
        let item = parse_clipboard(data, text).map_err(|fail| ParseError::Rejected { key: fail.0.into() });
        let presets = item.as_ref().ok().map(|item| create_presets(data, item, &preset_options()));
        let do_search = match (&item, &presets) {
            (Ok(item), Some(Ok(presets))) => initial_do_search(item, presets),
            _ => false,
        };
        Self { text: text.to_owned(), item, presets, do_search, league: DEFAULT_LEAGUE.to_owned(), open: true }
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

    /// The Search button: `doSearch = true`, which fetches the listings instead.
    pub fn start_search(&mut self) {
        self.do_search = true;
    }

    /// The close button (`closePriceCheck`): upstream hides the widget.
    pub fn close(&mut self) {
        self.open = false;
    }

    fn item_filters_mut(&mut self) -> Option<&mut ItemFilters> {
        let presets = self.presets.as_mut()?.as_mut().ok()?;
        let active = presets.active.clone();
        Some(&mut presets.presets.iter_mut().find(|preset| preset.id == active)?.filters)
    }
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
