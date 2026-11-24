//! `web/price-check/filters/FiltersBlock.vue`: the chips, the preset tabs and the stat rows.
//!
//! One module per component where the component is big enough: [`btn`] is
//! `FilterBtnNumeric.vue`/`FilterBtnLogical.vue`, [`modifier`] is `FilterModifier.vue` (with its
//! tiers and options), [`group`] is `FilterGroup.vue`, and [`source_info`], [`unknown`] and
//! [`anointment`] are the small components the rows mount.
//!
//! Font Awesome is not available (see [`crate::ui::icon`]), so its glyphs are drawn shapes.
//! The images are upstream's: the reference's `/images` files and the items' CDN art, through
//! [`crate::ui::art`].
//!
//! Not ported yet: `StatRollSlider.vue` (a unique's second line keeps its min/max fields but no
//! slider), the hidden row's `hiddenReason` tooltip (the eye is drawn, it opens nothing), the
//! connecting line behind `FilterModifierLinks.vue`'s gem dots, and the anointment oil prices.
//!
//! The Mods toggle shows each row's sources as a hover overlay (`FilterModifier.vue`'s
//! `.mods`): Rune has no `:hover` styling, so the row's `on_hover_change` records the hovered
//! [`StatPath`] and the block draws the anchored panel below it.

pub mod anointment;
pub mod btn;
pub mod group;
pub mod modifier;
pub mod source_info;
pub mod unknown;

use rune::tok::TextStyle;
use rune::{Align, Color, El, col, el, row, text, text_input};
use wraex_core::Rarity;
use wraex_poe1::filters::{FilterOrGroup, FilterTag, ItemFilters, SearchFilter, StatFilter};
use wraex_poe1::parser::item::Item;
use wraex_poe1::parser::meta::ItemCategory;
use wraex_poe1::parser::modifiers::ModifierMechanic;

use crate::poe1::i18n;
use crate::poe1::price_check::view::Project;
use crate::poe1::price_check::{InputField, LogicalToggle, NumericFilter, PriceCheck, StatPath};
use crate::theme::{self, text as type_};
use crate::ui::art;
use crate::ui::{self, icon};

/// `FilterModifier.vue`'s `.rollInput`: `w-12` (48 pt), 22 pt tall with its 1 pt border.
const ROLL_WIDTH: f32 = 48.0;

/// The block's tall-first-line height, `line-height: 1.25rem` plus a 1 pt border.
pub(super) const LINE: f32 = 22.0;

/// `FiltersBlock.vue`.
pub fn view<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let mut root = col().key("filters").w_full();
    let mut chips =
        row().key("filters-chips").w_full().flex_wrap().items_center().pb(theme::rem(0.75)).gap(theme::rem(0.5));
    // `createPresets` throwing leaves the reference with no `CheckedItem` at all; the port
    // says why instead of drawing an empty block.
    if let Some(Err(error)) = &check.presets {
        chips = chips.child(
            text(format!("Filters unavailable: {error}")).key("filters-error").font(type_::SM).color(theme::gray(500)),
        );
        return root.child(chips);
    }
    let Some(filters) = check.item_filters() else { return root.child(chips) };

    if let Some(sub) = active_search(filters).and_then(|search| search.sub.as_deref()) {
        let label = sub.name.clone().or_else(|| sub.base_type.clone()).unwrap_or_default();
        chips = chips.child(btn::logical(
            "search-sub",
            &label,
            sub.disabled != Some(true),
            None,
            None,
            move |s: &mut S| {
                project(s).toggle_logical(LogicalToggle::SearchSub);
            },
        ));
    }
    for which in [NumericFilter::LinkedSockets, NumericFilter::MapTier] {
        chips = numeric_chip(check, chips, which, project);
    }
    if let Some(reward) = &filters.map_completion_reward {
        let label = i18n::fmt("item.map_foil_reward", &[&reward.name]);
        chips = chips.child(btn::logical("map-foil", &label, true, None, None, |_: &mut S| {}));
    }
    for which in [
        NumericFilter::AreaLevel,
        NumericFilter::SentinelCharge,
        NumericFilter::ItemLevel,
        NumericFilter::StackSize,
        NumericFilter::WhiteSockets,
        NumericFilter::GemLevel,
        NumericFilter::Quality,
    ] {
        chips = numeric_chip(check, chips, which, project);
    }
    if filters.not_catalysed.is_some() {
        chips = chips.child(btn::logical(
            "not-catalysed",
            &i18n::t("item.not_catalysed"),
            filters.not_catalysed.as_ref().is_some_and(|filter| !filter.disabled),
            None,
            None,
            move |s: &mut S| project(s).toggle_logical(LogicalToggle::NotCatalysed),
        ));
    }
    if let Some(influences) = &filters.influences {
        for (index, influence) in influences.iter().enumerate() {
            let label = influence.value.as_str().to_owned();
            // `:img="`/images/influence-${influence.value}.png`"`
            let picture = art::Picture::local(format!("influence-{}.png", influence.value.as_str()));
            chips = chips.child(btn::logical(
                &format!("influence-{index}"),
                &label,
                !influence.disabled,
                None,
                Some(picture),
                move |s: &mut S| project(s).toggle_logical(LogicalToggle::Influence(index)),
            ));
        }
    }
    if filters.rarity.as_ref().is_some_and(|rarity| rarity.value == "magic") {
        chips = chips.child(btn::logical(
            "rarity-magic",
            &i18n::t("Magic"),
            filters.rarity.as_ref().is_some_and(|rarity| !rarity.disabled),
            None,
            None,
            move |s: &mut S| project(s).toggle_logical(LogicalToggle::RarityMagic),
        ));
    }
    if let Some(unidentified) = &filters.unidentified {
        chips = chips.child(btn::logical(
            "unidentified",
            &i18n::t("item.unidentified"),
            !unidentified.disabled,
            None,
            None,
            move |s: &mut S| project(s).toggle_logical(LogicalToggle::Unidentified),
        ));
    }
    if let Some(veiled) = &filters.veiled {
        chips = chips.child(btn::logical(
            "veiled",
            &i18n::t("item.veiled"),
            !veiled.disabled,
            None,
            None,
            move |s: &mut S| project(s).toggle_logical(LogicalToggle::Veiled),
        ));
    }
    if let Some(foil) = &filters.foil {
        chips = chips.child(btn::logical(
            "foil",
            &i18n::t("item.foil_unique"),
            !foil.disabled,
            None,
            None,
            move |s: &mut S| project(s).toggle_logical(LogicalToggle::Foil),
        ));
    }
    if let Some(mirrored) = &filters.mirrored
        && !mirrored.hidden
    {
        let label = i18n::t(if mirrored.disabled { "item.not_mirrored" } else { "item.mirrored" });
        chips = chips.child(btn::logical("mirrored", &label, !mirrored.disabled, None, None, move |s: &mut S| {
            project(s).toggle_logical(LogicalToggle::Mirrored);
        }));
    }
    if let Some(split) = &filters.split
        && !split.hidden
    {
        let label = i18n::t(if split.disabled { "item.not_split" } else { "item.split" });
        chips = chips.child(btn::logical("split", &label, !split.disabled, None, None, move |s: &mut S| {
            project(s).toggle_logical(LogicalToggle::Split);
        }));
    }

    let stats = check.item_stats();
    let unknown = unknown_count(check);
    let preset_count = preset_count(check);
    let has_stats = !stats.is_empty()
        || (unknown > 0 && check.item().is_some_and(|item| item.rarity == Some(Rarity::Unique)))
        || preset_count > 1;
    if has_stats {
        let selected = selected_mods(stats);
        let label = if selected > 0 {
            i18n::fmt("filters.selected_some", &[&selected.to_string(), &stats.len().to_string()])
        } else {
            i18n::t("filters.selected_none")
        };
        chips = chips.child(btn::logical(
            "stats",
            &label,
            selected > 0,
            Some(check.stats_collapsed),
            None,
            move |s: &mut S| project(s).toggle_stats_collapsed(),
        ));
    }
    root = root.child(chips);

    if check.stats_collapsed || !has_stats {
        return root;
    }
    let mut block = col().key("filters-stats").w_full().mb(theme::rem(1.0)).mt(if preset_count > 1 {
        theme::rem(0.25)
    } else {
        theme::rem(1.0)
    });
    if preset_count > 1 {
        block = block.child(preset_tabs(check, project));
    }
    let mut list = col().key("filters-list").w_full();
    let Some(item) = check.item() else { return root };
    let filtered: Vec<(StatPath, &FilterOrGroup)> = stats
        .iter()
        .enumerate()
        .filter(|(_, stat)| is_hidden(stat) == check.show_hidden)
        .map(|(index, stat)| match stat {
            FilterOrGroup::Stat(_) => (StatPath::Top(index), stat),
            FilterOrGroup::Group(_) => (StatPath::Meta(index), stat),
        })
        .collect();
    for (path, stat) in &filtered {
        match stat {
            FilterOrGroup::Stat(filter) => {
                list = list.child(modifier::view(check, filter, item, false, None, *path, None, project));
            }
            FilterOrGroup::Group(group) => {
                list = list.child(group::view(check, group, item, path_group(*path), project));
            }
        }
    }
    if filtered.is_empty() && unknown == 0 {
        list = list.child(
            el().key("filters-empty")
                .w_full()
                .py(theme::rem(0.5))
                .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
                .border_layout()
                .child(text(i18n::t("filters.empty")).key("filters-empty-text").font(type_::SM)),
        );
    }
    if unknown > 0 {
        for (index, stat) in item.unknown_modifiers.iter().enumerate() {
            list = list.child(unknown::view(&format!("unknown-{index}"), stat));
        }
    }
    block = block.child(list).child(controls(check, filtered.len(), stats.len(), project));
    root = root.child(block);
    // `FilterModifier.vue`'s `.mods`, which CSS shows only while its row is hovered.
    if check.show_sources
        && let Some(path) = check.source_hover
        && let Some(filter) = check.stat(path)
        && modifier::shows_source_info(check, filter)
    {
        root = root.child(sources_overlay(filter, path));
    }
    root
}

/// `FilterModifier.vue`'s sources overlay: the row's contributing modifiers on a gradient
/// bar, below the row, 4 pt of `gray-500` along its bottom edge.
fn sources_overlay(filter: &StatFilter, path: StatPath) -> El {
    let mut panel = col()
        .key(format!("stat-{}-sources", path.key()))
        .w(super::WIDTH - theme::rem(1.0))
        .px(theme::rem(1.0))
        .gradient(theme::gray(800), theme::gray(900))
        .border_sides([0.0, 0.0, 4.0, 0.0], theme::gray(500))
        .border_layout()
        .anchored(format!("stat-{}", path.key()), rune::Side::Bottom, rune::Align::Start, 0.0);
    for (index, source) in filter.stat_sources().enumerate() {
        panel = panel.child(
            row()
                .key(format!("stat-{}-source-{index}", path.key()))
                .w_full()
                .pl(theme::rem(1.25))
                .py(theme::rem(0.25))
                .child(source_info::view(filter, source)),
        );
    }
    panel
}

/// One `filter-btn-numeric`, when the preset has that field.
fn numeric_chip<S: 'static>(check: &PriceCheck, chips: El, which: NumericFilter, project: Project<S>) -> El {
    if check.numeric(which).is_some() { chips.child(btn::numeric(check, which, project)) } else { chips }
}

/// `FilterGroup.vue`'s key index: the group's position in `stats`.
fn path_group(path: StatPath) -> usize {
    match path {
        StatPath::Meta(group) => group,
        StatPath::Top(index) => index,
        StatPath::Child(group, _) => group,
    }
}

/// `FiltersBlock.vue`'s `hasStats` without the unknown-modifier clause's item lookup.
fn preset_count(check: &PriceCheck) -> usize {
    match check.presets.as_ref() {
        Some(Ok(presets)) => presets.presets.len(),
        _ => 0,
    }
}

/// `showUnknownMods`: the unknown rows are for craftable items, not maps or Sentinels.
fn unknown_count(check: &PriceCheck) -> usize {
    let Some(item) = check.item() else { return 0 };
    if matches!(item.category, Some(ItemCategory::Sentinel) | Some(ItemCategory::Map)) {
        return 0;
    }
    item.unknown_modifiers.len()
}

/// A `FilterOrGroup`'s `hidden`.
pub(super) fn is_hidden(stat: &FilterOrGroup) -> bool {
    match stat {
        FilterOrGroup::Stat(filter) => filter.hidden.is_some(),
        FilterOrGroup::Group(group) => group.meta.hidden.is_some(),
    }
}

/// `totalSelectedMods`: the rows whose filter is on.
pub(super) fn selected_mods(stats: &[FilterOrGroup]) -> usize {
    stats
        .iter()
        .filter(|stat| match stat {
            FilterOrGroup::Stat(filter) => !filter.disabled,
            FilterOrGroup::Group(group) => !group.meta.disabled,
        })
        .count()
}

/// `searchSub`: the active search's `sub` (`searchRelaxed` unless it is disabled).
fn active_search(filters: &ItemFilters) -> Option<&SearchFilter> {
    match &filters.search_relaxed {
        Some(relaxed) if relaxed.disabled != Some(true) => Some(relaxed),
        _ => Some(&filters.search_exact),
    }
}

/// `FiltersBlock.vue`'s preset row: one tab per preset, the active one on a gradient, joined
/// to the line that runs across the block.
fn preset_tabs<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let Some(Ok(presets)) = check.presets.as_ref() else { return el() };
    let mut tabs = row()
        .key("preset-tabs-list")
        .h_full()
        .items_stretch()
        .border_sides([1.0, 1.0, 0.0, 1.0], theme::gray(700))
        .border_layout()
        .corners([theme::ROUNDED, theme::ROUNDED, 0.0, 0.0]);
    for (index, preset) in presets.presets.iter().enumerate() {
        let id = preset.id.clone();
        let active = preset.id == presets.active;
        let mut tab = row()
            .key(format!("preset-{index}"))
            .min_w(theme::rem(3.0))
            .px(theme::rem(0.5))
            .items_center()
            .justify_center()
            .focusable(true)
            .role("button")
            .label(i18n::t(&id))
            .on_press(move |s: &mut S| project(s).select_preset(&id));
        tab = if active { tab.gradient(theme::gray(900), theme::gray(800)) } else { tab.fill(theme::gray(800)) };
        if index > 0 {
            tab = tab.border_sides([0.0, 0.0, 0.0, 1.0], theme::gray(700)).border_layout();
        }
        tabs = tabs.child(tab.child(text(i18n::t(&preset.id)).key(format!("preset-{index}-label")).font(type_::SM)));
    }
    row()
        .key("preset-tabs")
        .w_full()
        // A tab is one point shorter than a chip: its border-top plus a `leading-none` button.
        .h(LINE - 1.0)
        .items_stretch()
        .child(el().w(theme::rem(1.25)).h_full().border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700)).border_layout())
        .child(tabs)
        .child(el().grow(1.0).h_full().border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700)).border_layout())
}

/// `FiltersBlock.vue`'s row under the form: Collapse, then Hidden and Mods.
fn controls<S: 'static>(check: &PriceCheck, filtered: usize, total: usize, project: Project<S>) -> El {
    let mut row = row().key("filters-controls").w_full().items_center().gaps(0.0, theme::rem(1.0)).child(
        row()
            .key("filters-collapse")
            .w(theme::rem(10.0))
            .px(theme::rem(0.5))
            .py(theme::rem(0.25))
            .items_center()
            .justify_center()
            .corners([0.0, 0.0, theme::ROUNDED, theme::ROUNDED])
            .fill(theme::gray(700))
            .focusable(true)
            .role("button")
            .label(i18n::t("filters.collapse"))
            .on_press(move |s: &mut S| project(s).toggle_stats_collapsed())
            // `{{ t('filters.collapse') }} <i class="… pl-1">`: a space, then 4 pt.
            .child(
                text(format!("{} ", i18n::t("filters.collapse")))
                    .key("filters-collapse-label")
                    .font(theme::leading_none(type_::SM))
                    .color(theme::gray(400)),
            )
            .child(icon::chevron(theme::rem(0.75), true, theme::gray(600)).pl(theme::rem(0.25))),
    );
    if filtered != total {
        row = row.child(ui::toggle::toggle(
            "show-hidden",
            &i18n::t("filters.hidden_toggle"),
            check.show_hidden,
            move |s: &mut S| project(s).toggle_show_hidden(),
        ));
    }
    row = row.child(el().key("filters-controls-spacer").grow(1.0));
    row.child(
        ui::toggle::toggle("show-sources", &i18n::t("filters.mods_toggle"), check.show_sources, move |s: &mut S| {
            project(s).toggle_show_sources()
        })
        .pt(theme::rem(0.5)),
    )
}

/// A `#`-text with its roll, via the shared `ItemModifierText`.
pub(super) fn modifier_text(key: &str, text: &str, roll: Option<f64>, style: TextStyle) -> El {
    ui::item_modifier_text(key, text, roll, style)
}

/// One of the reference's small tags (`PSEUDO X 2`, `TIER 1`, `STAT GROUP`), coloured by its
/// [`FilterTag`] as `FilterModifier.vue`'s style module does.
pub(super) fn tag_badge(label: &str, tag: FilterTag) -> El {
    use ModifierMechanic as M;
    use wraex_poe1::parser::modifiers::ModifierType as T;

    let (fill, color, text_only) = match tag {
        FilterTag::Type(T::Pseudo) | FilterTag::Brick => (theme::gray(700), Color::BLACK, false),
        FilterTag::Variant => (theme::yellow(700), theme::yellow(100), false),
        FilterTag::Mechanic(M::Corruption) => (theme::red(700), theme::red(100), false),
        FilterTag::Type(T::Fractured) => (theme::orange(300), Color::BLACK, false),
        FilterTag::Type(T::Crafted) | FilterTag::Mechanic(M::Synthesised) => {
            (theme::blue(600), theme::blue(100), false)
        }
        FilterTag::Type(T::Enchant) => (theme::indigo(200), Color::BLACK, false),
        FilterTag::Mechanic(M::Foulborn) => (theme::red(700), Color::WHITE, false),
        FilterTag::Mechanic(_) => (theme::gray(800), theme::gray(600), true),
        FilterTag::Property => (theme::gray(700), Color::BLACK, false),
        FilterTag::Type(_)
        | FilterTag::MercenaryPrimary
        | FilterTag::MercenarySecondary
        | FilterTag::MercenaryUtility
        | FilterTag::MercenarySupport
        | FilterTag::FilterGroup => (theme::gray(800), theme::gray(600), true),
    };
    let mut badge = row()
        .key(format!("tag-{}", slug(label)))
        .radius(theme::ROUNDED)
        .center_y()
        .child(icon::badge_text(label, color));
    if !text_only {
        badge = badge.px(theme::rem(0.25)).fill(fill);
    }
    badge
}

/// `MILLISECONDS`-free slug for element keys: lower case, non-alphanumerics collapsed to `-`.
pub(super) fn slug(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut dash = false;
    for c in label.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// Where a min/max field sits: `FilterBtnNumeric.vue`'s chip input and
/// `FilterModifier.vue`'s `.rollInput` are styled differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FieldStyle {
    /// `FilterBtnNumeric.vue`'s `.input`: transparent, so the chip's own background and
    /// border show through; `text-gray-300`, placeholder `text-gray-400`; gray-700 while
    /// the caret is in it.
    Chip,
    /// `FilterModifier.vue`'s `.rollInput`: a `bg-gray-900` box, placeholder `text-gray-700`
    /// at 13 pt, gray-500 border while focused.
    Roll,
}

impl FieldStyle {
    fn height(self) -> f32 {
        match self {
            // The chip is 22 pt with its 1 pt border, so its content box is 20.
            FieldStyle::Chip => LINE - 2.0,
            FieldStyle::Roll => LINE,
        }
    }

    fn fill(self, focused: bool) -> Color {
        match self {
            FieldStyle::Chip if focused => theme::gray(700),
            FieldStyle::Chip => Color::TRANSPARENT,
            FieldStyle::Roll => theme::gray(900),
        }
    }

    fn placeholder_color(self) -> Color {
        match self {
            FieldStyle::Chip => theme::gray(400),
            FieldStyle::Roll => theme::gray(700),
        }
    }

    fn placeholder_font(self) -> TextStyle {
        match self {
            // `.input::placeholder` keeps the field's own 20 pt line, so it centres like the
            // value does.
            FieldStyle::Chip => TextStyle::new(14.0, 400.0).lh(20.0),
            // `.rollInput::placeholder` is 13 pt on the field's 20 pt line.
            FieldStyle::Roll => TextStyle::new(13.0, 400.0).lh(20.0),
        }
    }
}

/// A chip or row's min/max field: a Rune text field bound to the state, `w-12`/`w-auto`,
/// `text-center`, styled by its [`FieldStyle`].
pub(super) fn number_field<S: 'static>(
    check: &PriceCheck,
    field: InputField,
    width: f32,
    placeholder: &str,
    corners: [f32; 4],
    style: FieldStyle,
    project: Project<S>,
) -> El {
    let key = field.key();
    let Some(edit) = check.input(&key) else {
        return el().key(key).size(width, style.height());
    };
    let edit_key = key.clone();
    let change_key = key.clone();
    let focused = edit.focused;
    text_input(edit, type_::SM)
        .key(key)
        .size(width, style.height())
        .align(Align::Center)
        .single_line()
        .label(placeholder)
        .fill(style.fill(focused))
        .corners(corners)
        // `.rollInput:focus` / `.input:hover, .input:focus`: gray-500 only while the field
        // holds the caret.
        .border(1.0, if focused { theme::gray(500) } else { Color::TRANSPARENT })
        .border_layout()
        .caret(theme::gray(300), theme::gray(400).alpha(0.35))
        .color(theme::gray(300))
        .placeholder(placeholder, style.placeholder_color())
        .placeholder_font(style.placeholder_font())
        .on_edit(move |s: &mut S| project(s).input_mut(&edit_key))
        .on_input_change(move |s: &mut S, _| {
            let text = project(s).input(&change_key).map(|edit| edit.text.clone()).unwrap_or_default();
            project(s).apply_input(field, &text);
        })
        .on_submit(move |s: &mut S, _| project(s).start_search())
        .on_input_key(|_s: &mut S, event: &rune::KeyEvent, _tx: &mut rune::Tx| match &event.key {
            // `<input type="number">` never takes a letter.
            rune::Key::Text(text) => !text.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-'),
            _ => false,
        })
}

/// The width `FilterBtnNumeric.vue` gives its input: `${1.2 + Math.max(String(value).length, 2)}ch`.
pub(super) fn number_width(text: &str) -> f32 {
    let chars = text.chars().count().max(2) as f32;
    (1.2 + chars) * 7.8 + 2.0
}

/// A `StatFilter`'s display text: internal trade ids are the app's own labels, with the number
/// placeholders left for [`ui::item_modifier_text`] to fill.
pub(super) fn stat_text(filter: &StatFilter) -> String {
    match filter.first_trade_id() {
        Some(id) if wraex_poe1::filters::INTERNAL_TRADE_IDS.contains(&id) => i18n::fmt(id, &["#", "#"]),
        _ => filter.text.clone(),
    }
}

/// `FilterModifier.vue`'s `showQ20Notice` set.
pub(super) fn shows_quality(trade_id: Option<&str>) -> bool {
    matches!(
        trade_id,
        Some("item.armour")
            | Some("item.evasion_rating")
            | Some("item.energy_shield")
            | Some("item.ward")
            | Some("item.total_dps")
            | Some("item.physical_dps")
    )
}

/// `FilterModifier.vue`'s `showTag`.
pub(super) fn shows_tag(filter: &StatFilter, item: &Item, grouped: bool) -> bool {
    filter.tag != FilterTag::Property
        && filter.tag != FilterTag::MercenarySupport
        && filter.first_trade_id() != Some("item.has_empty_modifier")
        && !item.ref_name_is("Chronicle of Atzoatl")
        && !item.ref_name_is("Mirrored Tablet")
        && !item.ref_name_is("Filled Coffin")
        && !item.ref_name_is("Viscera")
        && item.category != Some(ItemCategory::Gem)
        && !(item.rarity == Some(Rarity::Unique)
            && filter.tag == FilterTag::EXPLICIT
            && (filter.roll.as_ref().is_some_and(|roll| roll.bounds.is_some()) || filter.hidden.is_some())
            && !grouped)
}

/// `FilterModifier.vue`'s `miniFilter`.
pub(super) fn is_mini(filter: &StatFilter, item: &Item) -> bool {
    filter.hidden.is_none()
        && (filter.tag == FilterTag::MercenarySupport
            || (filter.tag == FilterTag::Property && item.ref_name_is("Mercenary Warrant"))
            || item.ref_name_is("Mirrored Tablet")
            || item.ref_name_is("Filled Coffin")
            || item.category == Some(ItemCategory::Gem))
}

/// The `filters.tag_<tag>` label plus `x N` when several sources make it up.
pub(super) fn tag_label(filter: &StatFilter) -> String {
    let tag = filter.tag.as_str().replace('-', "_");
    let mut label = i18n::t(&format!("filters.tag_{tag}"));
    if filter.sources.len() > 1 {
        label.push_str(&format!(" x {}", filter.sources.len()));
    }
    label
}

/// The item's or currency's image (`FilterModifier.vue`'s mercenary icon, the roll markers
/// and the influence badges): the reference's own `/images` art, or an item's CDN icon.
pub(super) fn image(picture: art::Picture, size: f32) -> El {
    art::picture(wraex_core::GameId::Poe1, &picture, (size, size))
}
