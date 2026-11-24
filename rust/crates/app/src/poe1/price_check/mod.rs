//! `web/price-check/PriceCheckWindow.vue`: the PoE1 price-check window.
//!
//! The window frame, the item header (`FilterName.vue`) and the filters block
//! (`FiltersBlock.vue`) are ported. The trend and the listings are still placeholders
//! (PLAN phase 5's next steps).

pub mod filters;
pub mod view;

use std::collections::HashMap;
use std::sync::OnceLock;

use rune::TextEdit;
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
        let mut check = Self {
            text: text.to_owned(),
            item,
            presets,
            do_search,
            league: DEFAULT_LEAGUE.to_owned(),
            open: true,
            stats_collapsed: false,
            show_hidden: false,
            show_sources: false,
            inputs: HashMap::new(),
        };
        check.sync_inputs();
        check
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

    /// The Search button: `doSearch = true`, which fetches the listings instead.
    pub fn start_search(&mut self) {
        self.do_search = true;
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
