//! `web/price-check/filters/FiltersBlock.vue`: the chips, the preset tabs and the stat rows.
//!
//! EE2's block differs from APT's: the chip list follows PoE2's filters (runes, spirit,
//! ascendancy points, waystone hints), stats are a flat list (no `FilterGroup`), an item
//! editor button sits at the right of the chips, and the Hidden toggle is labelled
//! `Explicit` for maps and waystones. The stat rows themselves are [`modifier`].
//!
//! Font Awesome is not available (see [`crate::ui::icon`]), so its glyphs are drawn shapes.
//! The images are upstream's: the reference's `/images` files (the item editor's socket) and
//! the items' CDN art, through [`crate::ui::art`].

pub mod anointment;
pub mod btn;
pub mod modifier;
pub mod slider;
pub mod source_info;
pub mod unknown;

use rune::tok::TextStyle;
use rune::{Align, Color, El, col, el, row, text, text_input};
use wraex_core::Rarity;
use wraex_poe2::filters::{FilterTag, StatFilter};
use wraex_poe2::parser::item::{Item, ItemInfluence};
use wraex_poe2::parser::meta::{self, ARMOUR, ItemCategory};

use crate::poe2::i18n;
use crate::poe2::price_check::view::Project;
use crate::poe2::price_check::{ElementalOption, InputField, ItemFlag, LogicalToggle, NumericFilter, PriceCheck};
use crate::theme::{self, text as type_};
use crate::ui::art;
use crate::ui::{self, icon};

/// `FilterModifier.vue`'s `.rollInput`: `w-12` (48 pt), 22 pt tall with its 1 pt border.
const ROLL_WIDTH: f32 = 48.0;

/// The block's tall-first-line height, `line-height: 1.25rem` plus a 1 pt border.
pub(super) const LINE: f32 = 22.0;

/// The width of the two roll inputs and the 1 pt between them: `calc(2 * 3rem + 1px)`.
const INPUTS_WIDTH: f32 = 2.0 * ROLL_WIDTH + 1.0;

/// `FiltersBlock.vue`.
pub fn view<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let mut root = col().key("filters").w_full();
    // `flex flex-wrap items-center pb-3 gap-2`, beside the editor button (`justify-between`).
    let mut chips = row()
        .key("filters-chips")
        .grow(1.0)
        .min_w(0.0)
        .flex_wrap()
        .items_center()
        .pb(theme::rem(0.75))
        .gap(theme::rem(0.5));
    // `createPresets` throwing leaves the reference with no `CheckedItem` at all; the port
    // says why instead of drawing an empty block.
    if let Some(Err(error)) = &check.presets {
        chips = chips.child(
            text(format!("Filters unavailable: {error}")).key("filters-error").font(type_::SM).color(theme::gray(500)),
        );
        return root.child(chips);
    }
    let Some(filters) = check.active_preset().map(|preset| &preset.filters) else {
        return root.child(chips);
    };

    // The `filter-btn-numeric` list, in template order, then the `filter-btn-logical` one.
    for which in [
        NumericFilter::SocketNumber,
        NumericFilter::LinkedSockets,
        NumericFilter::MapTier,
        NumericFilter::AscendancyPoints,
        NumericFilter::AreaLevel,
        NumericFilter::WingsRevealed,
        NumericFilter::SentinelCharge,
    ] {
        chips = numeric_chip(check, chips, which, project);
    }
    if let Some(blighted) = &filters.map_blighted {
        chips = chips.child(btn::readonly::<S>("map-blighted", blighted.value));
    }
    if filters.rarity.as_ref().is_some_and(|rarity| rarity.value == "magic") {
        chips = chips.child(btn::readonly::<S>("rarity-magic", &i18n::t("Magic")));
    }
    if filters.rarity.as_ref().is_some_and(|rarity| rarity.value == "normal") {
        chips = chips.child(btn::readonly::<S>("rarity-normal", &i18n::t("Normal")));
    }
    if let Some(discriminator) = filters.discriminator.as_ref().and_then(|disc| disc.value.as_ref()) {
        chips = chips.child(btn::readonly::<S>("discriminator", discriminator));
    }
    for which in [NumericFilter::ItemLevel, NumericFilter::RequiresLevel] {
        chips = numeric_chip(check, chips, which, project);
    }
    for which in [
        NumericFilter::StackSize,
        NumericFilter::WhiteSockets,
        NumericFilter::AugmentSockets,
        NumericFilter::GemLevel,
        NumericFilter::Quality,
    ] {
        chips = numeric_chip(check, chips, which, project);
    }
    if let Some(influences) = &filters.influences {
        for (index, influence) in influences.iter().enumerate() {
            let label = influence_name(influence.value);
            // `:img="`/images/influence-${influence.value}.png`"`
            let picture = art::Picture::local(format!("influence-{label}.png"));
            chips = chips.child(btn::logical(
                &format!("influence-{index}"),
                label,
                Some(!influence.disabled),
                None,
                Some(picture),
                |_: &mut S| {},
            ));
        }
    }
    if let Some(tier) = &filters.unidentified_tier {
        chips = chips.child(btn::logical(
            "unidentified-tier",
            &i18n::t("item.unidentified_tier"),
            Some(!tier.disabled),
            None,
            None,
            move |s: &mut S| project(s).toggle_logical(LogicalToggle::UnidentifiedTier),
        ));
    } else if let Some(unidentified) = &filters.unidentified {
        chips = chips.child(btn::logical(
            "unidentified",
            &i18n::t("item.unidentified"),
            Some(!unidentified.disabled),
            None,
            None,
            move |s: &mut S| project(s).toggle_logical(LogicalToggle::Unidentified),
        ));
    }
    if let Some(veiled) = &filters.veiled {
        chips = chips.child(btn::logical(
            "veiled",
            &i18n::t("item.veiled"),
            Some(!veiled.disabled),
            None,
            None,
            move |s: &mut S| project(s).set_item_flag(ItemFlag::Veiled),
        ));
    }
    if let Some(foil) = &filters.foil {
        chips = chips.child(btn::logical(
            "foil",
            &i18n::t("item.foil_unique"),
            Some(!foil.disabled),
            None,
            None,
            move |s: &mut S| project(s).set_item_flag(ItemFlag::Foil),
        ));
    }
    if let Some(mirrored) = &filters.mirrored {
        let label = i18n::t(if mirrored.disabled { "item.not_mirrored" } else { "item.mirrored" });
        chips = chips.child(btn::logical("mirrored", &label, Some(true), None, None, move |s: &mut S| {
            project(s).set_item_flag(ItemFlag::Mirrored);
        }));
    }
    if let Some(sanctified) = &filters.sanctified {
        let label = i18n::t(if sanctified.disabled { "item.not_sanctified" } else { "item.sanctified" });
        chips = chips.child(btn::logical("sanctified", &label, Some(true), None, None, move |s: &mut S| {
            project(s).set_item_flag(ItemFlag::Sanctified);
        }));
    }
    if let Some(hint) = &filters.ultimatum_hint {
        let label = i18n::t(&format!("item.{}", hint.value.to_lowercase()));
        chips = chips.child(btn::logical("ultimatum", &label, Some(!hint.disabled), None, None, move |s: &mut S| {
            project(s).toggle_logical(LogicalToggle::UltimatumHint);
        }));
    }

    let stats = check.item_stats();
    let unknown = unknown_count(check);
    let preset_count = preset_count(check);
    let has_stats = !stats.is_empty()
        || (unknown > 0 && check.item().is_some_and(|item| item.rarity == Some(Rarity::Unique)))
        || preset_count > 1;
    if has_stats {
        let selected = stats.iter().filter(|stat| !stat.disabled).count();
        let label = if selected > 0 {
            i18n::fmt("filters.selected_some", &[&selected.to_string(), &stats.len().to_string()])
        } else {
            i18n::t("filters.selected_none")
        };
        chips = chips.child(btn::logical(
            "stats",
            &label,
            Some(selected > 0),
            Some(check.stats_collapsed),
            None,
            move |s: &mut S| project(s).toggle_stats_collapsed(),
        ));
    }
    let mut chip_row = row().key("filters-chips-row").w_full().justify_between().child(chips);
    if let Some(editor) = item_editor_button(check, project) {
        chip_row = chip_row.child(editor);
    }
    root = root.child(chip_row);

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
    let filtered: Vec<(usize, &StatFilter)> =
        stats.iter().enumerate().filter(|(_, stat)| stat.hidden.is_some() == check.show_hidden).collect();
    for (index, filter) in &filtered {
        list = list.child(modifier::view(check, filter, item, *index, project));
    }
    let show_unknown = unknown > 0;
    if filtered.is_empty() && !show_unknown {
        list = list.child(
            el().key("filters-empty")
                .w_full()
                .py(theme::rem(0.5))
                .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
                .border_layout()
                .child(text(i18n::t("filters.empty")).key("filters-empty-text").font(type_::SM)),
        );
    }
    if show_unknown {
        for (index, stat) in item.unknown_modifiers.iter().enumerate() {
            list = list.child(unknown::view(&format!("unknown-{index}"), stat, &check.text, project));
        }
    }
    block = block.child(list).child(controls(check, filtered.len(), stats.len(), project));
    root = root.child(block);
    // `FilterModifier.vue`'s `.mods`, which CSS shows only while its row is hovered.
    if check.show_sources
        && let Some(index) = check.source_hover
        && let Some(filter) = check.stat(index)
        && modifier::shows_source_info(check, filter)
    {
        root = root.child(sources_overlay(filter, index));
    }
    root
}

/// `FilterModifier.vue`'s sources overlay: the row's contributing modifiers on a gradient
/// bar, below the row, 4 pt of `gray-500` along its bottom edge.
fn sources_overlay(filter: &StatFilter, index: usize) -> El {
    let mut panel = col()
        .key(format!("stat-{index}-sources"))
        .w(super::WIDTH - theme::rem(1.0))
        .px(theme::rem(1.0))
        .gradient(theme::gray(800), theme::gray(900))
        .border_sides([0.0, 0.0, 4.0, 0.0], theme::gray(500))
        .border_layout()
        .anchored(format!("stat-{index}"), rune::Side::Bottom, rune::Align::Start, 0.0);
    for (source_index, source) in filter.sources.iter().enumerate() {
        panel = panel.child(
            row()
                .key(format!("stat-{index}-source-{source_index}"))
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

/// `FiltersBlock.vue`'s item-editor button (`getItemEditorType`): a ring, an amulet or a
/// jewel gets the catalyst editor, a weapon or armour the rune one, and a unique nothing.
fn item_editor_button<S: 'static>(check: &PriceCheck, _project: Project<S>) -> Option<El> {
    let item = check.item()?;
    if item.rarity == Some(Rarity::Unique) {
        return None;
    }
    let category = item.category.as_ref()?;
    let catalyst = matches!(category, ItemCategory::Ring | ItemCategory::Amulet | ItemCategory::Jewel);
    let augment = meta::is_weapon(category) || ARMOUR.contains(category);
    if !catalyst && !augment {
        return None;
    }
    // `FiltersBlock.vue`'s item-editor button. A catalyst's own icon is not ported (the item
    // editor itself is not), so the rune/empty-socket art stands in for its two other states.
    let rune = item.augment_sockets.as_ref().is_some_and(|sockets| sockets.augments.iter().any(Option::is_some));
    let name = if rune { "augments/rune.png" } else { "augments/empty-socket.png" };
    Some(
        row()
            .key("item-editor")
            .w(theme::rem(2.0))
            .h(theme::rem(2.0))
            .shrink()
            .radius(theme::ROUNDED)
            .border(1.0, theme::gray(500))
            .border_layout()
            .fill(theme::gray(900))
            .center()
            .focusable(true)
            .role("button")
            .label("item editor")
            .child(art::local(wraex_core::GameId::Poe2, name, theme::rem(1.75)).key("item-editor-icon")),
    )
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

/// `FiltersBlock.vue`'s row under the form: Collapse, then Hidden (or Explicit) and Mods.
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
            .child(icon::chevron(theme::rem(0.5), true, theme::gray(600)).pl(theme::rem(0.25))),
    );
    if filtered != total {
        row = row.child(ui::toggle::toggle(
            "show-hidden",
            &i18n::t(hidden_label(check)),
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

/// `filters.hidden_toggle`, or `filters.hidden_explicit_toggle` for maps and waystones.
fn hidden_label(check: &PriceCheck) -> &'static str {
    match check.item().and_then(|item| item.category.as_ref()) {
        Some(ItemCategory::Map | ItemCategory::Waystone) => "filters.hidden_explicit_toggle",
        _ => "filters.hidden_toggle",
    }
}

/// `hasStats`' preset clause.
fn preset_count(check: &PriceCheck) -> usize {
    match check.presets.as_ref() {
        Some(Ok(presets)) => presets.presets.len(),
        _ => 0,
    }
}

/// `showUnknownMods`: not for Sentinels, and not for a unique map.
fn unknown_count(check: &PriceCheck) -> usize {
    let Some(item) = check.item() else { return 0 };
    if item.category == Some(ItemCategory::Sentinel)
        || (item.category == Some(ItemCategory::Map) && item.rarity == Some(Rarity::Unique))
    {
        return 0;
    }
    item.unknown_modifiers.len()
}

/// `influence.value` as the chip's text (`ItemInfluence` serialises as `Shaper`, ...).
pub(super) fn influence_name(influence: ItemInfluence) -> &'static str {
    match influence {
        ItemInfluence::Crusader => "Crusader",
        ItemInfluence::Elder => "Elder",
        ItemInfluence::Hunter => "Hunter",
        ItemInfluence::Redeemer => "Redeemer",
        ItemInfluence::Shaper => "Shaper",
        ItemInfluence::Warlord => "Warlord",
    }
}

/// A `#`-text with its roll, via the shared `ItemModifierText`.
pub(super) fn modifier_text(key: &str, text: &str, roll: Option<f64>, style: TextStyle) -> El {
    ui::item_modifier_text(key, text, roll, style)
}

/// One of the reference's small tags (`pseudo x 2`, `Tier 1`), coloured by its [`FilterTag`]
/// as `FilterModifier.vue`'s style module does.
pub(super) fn tag_badge(label: &str, tag: FilterTag) -> El {
    use wraex_poe2::parser::modifiers::ModifierType as T;

    // `.tag` has no colour of its own; each tag class adds one (`FilterModifier.vue`'s
    // style module, which EE2 changed from APT's: `explicit` is plain gray-600).
    let (fill, color, margins) = match tag {
        FilterTag::Type(T::Explicit) | FilterTag::Type(T::Implicit) | FilterTag::Type(T::Skill) => {
            (None, theme::gray(600), true)
        }
        FilterTag::Corrupted => (Some(theme::red(700)), theme::red(100), false),
        FilterTag::Type(T::Fractured) => (Some(theme::yellow(400)), Color::BLACK, false),
        FilterTag::Type(T::Crafted) | FilterTag::Synthesised | FilterTag::Type(T::Augment) => {
            (Some(theme::blue(600)), theme::blue(100), false)
        }
        FilterTag::Variant => (Some(theme::yellow(700)), theme::yellow(100), false),
        FilterTag::Type(T::Pseudo) => (Some(theme::gray(700)), Color::BLACK, false),
        FilterTag::Type(T::Enchant) => (Some(theme::purple(600)), theme::purple(100), false),
        FilterTag::Type(T::AddedAugment) => (Some(theme::blue(600)), theme::blue(100), false),
        FilterTag::Type(T::Desecrated) => (Some(theme::green(900)), theme::green(100), false),
        FilterTag::Mutated => (Some(theme::rose(900)), theme::yellow(100), false),
        // `.tag-sanctum { @apply border }`: a bare outline, no fill.
        FilterTag::Type(T::Sanctum) => (None, theme::gray(600), false),
        FilterTag::Type(T::Scourge) => (Some(theme::orange(600)), Color::WHITE, false),
        _ => (None, theme::gray(600), false),
    };
    let key = format!("tag-{}", slug(label));
    let mut badge = row()
        .key(key.clone())
        .radius(theme::ROUNDED)
        .px(theme::rem(0.25))
        .center_y()
        .child(icon::badge_text(label, color).key(format!("{key}-text")));
    if let Some(fill) = fill {
        badge = badge.fill(fill);
    }
    if margins {
        // `-mx-1`: the tag's padding overlaps the row's gutter.
        badge = badge.mx(-(theme::rem(0.25)));
    }
    badge
}

/// Lower case, non-alphanumerics collapsed to `-`, for element keys.
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
    /// `.input`: transparent, so the chip's own background and border show through;
    /// `text-gray-300`, placeholder `text-gray-400`; gray-700 while the caret is in it.
    Chip,
    /// `.rollInput`: a `bg-gray-900` box, placeholder `text-gray-700` at 13 pt, gray-500
    /// border while focused.
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
    // The chip's fields have no placeholder of their own; the key names them for a reader.
    let label = if placeholder.is_empty() { key.clone() } else { placeholder.to_owned() };
    text_input(edit, type_::SM)
        .key(key)
        .size(width, style.height())
        .align(Align::Center)
        .single_line()
        .label(label)
        .fill(style.fill(focused))
        .corners(corners)
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

/// The element buttons of an `item.elemental_dps` row (`FilterModifierItemIsElemental.vue`).
pub(super) fn elemental_options<S: 'static>(filter: &StatFilter, index: usize, project: Project<S>) -> Option<El> {
    if filter.trade_id.as_deref().and_then(|ids| ids.first()).map(String::as_str) != Some("item.elemental_dps") {
        return None;
    }
    let info = filter.additional_info.as_ref()?.elemental_info.as_ref()?;
    let selected = filter.option.and_then(|option| option.value);
    let mut out = row().key(format!("stat-{index}-elements")).gap(theme::rem(0.25));
    for option in ElementalOption::ALL {
        if !info.contains_key(option.stat_ref()) {
            continue;
        }
        let is_selected = selected == Some(option as u8 as f64);
        let color = match option {
            ElementalOption::Any => theme::gray(200),
            ElementalOption::Fire => theme::fire(),
            ElementalOption::Cold => theme::cold(),
            ElementalOption::Lightning => theme::lightning(),
        };
        out = out.child(
            row()
                .key(format!("stat-{index}-element-{}", option.label()))
                .h(theme::rem(1.15))
                .px(theme::rem(0.5))
                .items_center()
                .justify_center()
                .fill(theme::gray(700))
                .radius(theme::ROUNDED)
                .border(1.0, if is_selected { color } else { Color::TRANSPARENT })
                .border_layout()
                .focusable(true)
                .role("button")
                .label(i18n::t(option.label()))
                .on_press(move |s: &mut S| project(s).select_element(index, option))
                .child(icon::badge_text(&i18n::t(option.label()), color)),
        );
    }
    Some(out)
}

/// The item's or currency's image (`FiltersBlock.vue`'s influence badges, `FilterModifier.vue`):
/// the reference's own `/images` art, or an item's CDN icon.
pub(super) fn image(picture: art::Picture, size: f32) -> El {
    art::picture(wraex_core::GameId::Poe2, &picture, (size, size))
}

/// `Quality: {0}` and friends: the stat filter's own text, with internal trade ids
/// replaced by the app's label.
pub(super) fn stat_text(filter: &StatFilter) -> String {
    match filter.trade_id.as_deref().and_then(|ids| ids.first()) {
        Some(id) if wraex_poe2::filters::INTERNAL_TRADE_IDS.contains(&id.as_str()) => i18n::fmt(id, &["#", "#"]),
        _ => filter.text.clone(),
    }
}

/// `FilterModifier.vue`'s `showQ20Notice`.
pub(super) fn shows_quality(trade_id: Option<&str>, item: &Item) -> bool {
    matches!(
        trade_id,
        Some("item.armour")
            | Some("item.evasion_rating")
            | Some("item.energy_shield")
            | Some("item.total_dps")
            | Some("item.physical_dps")
    ) && item.is_modifiable()
}

/// `FilterModifier.vue`'s `showTag`.
pub(super) fn shows_tag(filter: &StatFilter, item: &Item) -> bool {
    let first = filter.trade_id.as_deref().and_then(|ids| ids.first()).map(String::as_str);
    filter.tag != FilterTag::Property
        && first != Some("item.has_empty_modifier")
        && !item.ref_name_is("Chronicle of Atzoatl")
        && !item.ref_name_is("Mirrored Tablet")
        && !item.ref_name_is("Filled Coffin")
        && !(item.rarity == Some(Rarity::Unique) && matches!(filter.tag, FilterTag::EXPLICIT | FilterTag::PSEUDO))
}

/// The `filters.tag_<tag>` label plus `x N` when several sources make it up.
pub(super) fn tag_label(filter: &StatFilter) -> String {
    let tag = tag_key(filter.tag).replace('-', "_");
    let mut label = i18n::t(&format!("filters.tag_{tag}"));
    if filter.sources.len() > 1 {
        label.push_str(&format!(" x {}", filter.sources.len()));
    }
    label
}

/// `FilterTag`'s `filters.tag_<tag>` key: the modifier type's name, or the tag's own.
fn tag_key(tag: FilterTag) -> &'static str {
    match tag {
        FilterTag::Type(kind) => kind.as_str(),
        FilterTag::Corrupted => "corrupted",
        FilterTag::Synthesised => "synthesised",
        FilterTag::Eldritch => "eldritch",
        FilterTag::Variant => "variant",
        FilterTag::Property => "property",
        FilterTag::Delve => "explicit-delve",
        FilterTag::Unveiled => "explicit-veiled",
        FilterTag::Incursion => "explicit-incursion",
        FilterTag::Mutated => "mutated",
    }
}
