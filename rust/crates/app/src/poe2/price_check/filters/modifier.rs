//! `FilterModifier.vue`: one stat row — checkbox, text, tags, tiers, the roll fields and the
//! roll slider.
//!
//! `FilterModifierTiers.vue` and `FilterModifierItemIsElemental.vue` are folded in (EE2 keeps
//! them as separate components, but they only exist for a row).

use rune::{Color, El, col, el, row, text};
use wraex_poe2::filters::{FilterTag, StatFilter};
use wraex_poe2::parser::item::Item;
use wraex_poe2::parser::meta::ItemCategory;

use crate::poe2::i18n;
use crate::poe2::price_check::view::Project;
use crate::poe2::price_check::{InputField, NumericPart, PriceCheck};
use crate::theme::{self, text as type_};
use crate::ui::icon;

use super::{
    FieldStyle, INPUTS_WIDTH, NumberField, ROLL_WIDTH, elemental_options, modifier_text, number_field, shows_quality,
    shows_tag, slider, stat_text, tag_badge, tag_label,
};

/// `FilterModifier.vue`'s `alwaysShowTier` is `false` in the reference's config.
const ALWAYS_SHOW_TIER: bool = false;

/// `FilterModifier.vue`.
pub fn view<S: 'static>(check: &PriceCheck, filter: &StatFilter, item: &Item, index: usize, project: Project<S>) -> El {
    let trade_id = filter.trade_id.as_deref().and_then(|ids| ids.first()).map(String::as_str);
    let show_inputs = filter.roll.is_some() && filter.oils.is_none();

    let mut first = row().key(format!("stat-{index}-line")).w_full().pb(1.0).items_center().justify_between();
    let mut label_area = row().grow(1.0).min_w(0.0).items_center();
    label_area = label_area.child(checkbox(filter, index, project));
    label_area = label_area.child(label(filter, index, project));
    first = first.child(label_area);

    let mut right = row().key(format!("stat-{index}-inputs")).items_center().gap(theme::rem(0.25));
    if shows_quality(trade_id, item) {
        let quality = check.item().and_then(|item| item.quality).unwrap_or(0.0).max(20.0);
        right = right.child(
            row()
                .key(format!("stat-{index}-quality"))
                .px(theme::rem(0.5))
                .radius(theme::ROUNDED)
                .border(1.0, theme::gray(700))
                .border_layout()
                .child(
                    text(i18n::fmt("item.prop_quality", &[&wraex_core::js::number_to_string(quality)]))
                        .key(format!("stat-{index}-quality-text"))
                        .font(type_::SM)
                        .color(theme::gray(500)),
                ),
        );
    }
    if show_inputs {
        let min_field = InputField::Stat(index, NumericPart::Min);
        let max_field = InputField::Stat(index, NumericPart::Max);
        right = right.child(
            row()
                .key(format!("stat-{index}-roll"))
                .items_center()
                .gap(1.0)
                .child(number_field(
                    check,
                    &NumberField {
                        field: min_field,
                        width: ROLL_WIDTH,
                        placeholder: &i18n::t("min"),
                        corners: [theme::ROUNDED, 0.0, 0.0, theme::ROUNDED],
                        style: FieldStyle::Roll,
                    },
                    project,
                ))
                .child(number_field(
                    check,
                    &NumberField {
                        field: max_field,
                        width: ROLL_WIDTH,
                        placeholder: &i18n::t("max"),
                        corners: [0.0, theme::ROUNDED, theme::ROUNDED, 0.0],
                        style: FieldStyle::Roll,
                    },
                    project,
                )),
        );
    }
    first = first.child(right);

    let second = meta_row(filter, item, index, project);

    let mut root = row()
        .key(format!("stat-{index}"))
        .w_full()
        .py(theme::rem(0.5))
        .items_center()
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
        .border_layout();
    // `FilterModifier.vue`'s `.filter:not(:hover) > .mods { display: none }`: the sources
    // panel is drawn by the block (an anchored overlay) only while this row is hovered.
    if shows_source_info(check, filter) {
        root = root.on_hover_change(move |s: &mut S, inside: bool, _tx: &mut rune::Tx| {
            let check = project(s);
            if inside {
                check.source_hover = Some(index);
            } else if check.source_hover == Some(index) {
                check.source_hover = None;
            }
        });
    }
    root = root.child(col().grow(1.0).min_w(0.0).child(first).child(second));
    if let Some(anointment) = super::anointment::view(check, filter, index) {
        root = root.child(anointment);
    }
    root
}

/// The row's second line: the hidden marker's gutter, the tags and tiers (and the elemental
/// buttons), then the roll slider and the space the inputs above take.
fn meta_row<S: 'static>(filter: &StatFilter, item: &Item, index: usize, project: Project<S>) -> El {
    let mut second = row().key(format!("stat-{index}-meta")).w_full().items_start();
    let mut gutter = row().w(theme::rem(1.25)).items_start();
    if filter.hidden.is_some() {
        gutter = gutter.child(icon::eye_slash(theme::rem(0.75), theme::gray(600)).mt(theme::rem(0.125)));
    }
    second = second.child(gutter);
    let mut body = col().key(format!("stat-{index}-tags")).grow(1.0).min_w(0.0).gap(theme::rem(0.25));
    let mut tags = row().w_full().items_start().gap(theme::rem(0.5));
    if shows_tag(filter, item) {
        tags = tags.child(tag_badge(&tag_label(filter), filter.tag));
    }
    if let Some(tiers) = tiers(filter, item, index) {
        tags = tags.child(tiers);
    }
    if let Some(options) = elemental_options(filter, index, project) {
        tags = tags.child(options);
    }
    body = body.child(tags);
    second = second.child(body);
    if let Some(roll) = &filter.roll
        && let Some(bounds) = roll.bounds
    {
        second = second.child(slider::view(index, roll.value, roll.min, roll.max, bounds));
    }
    second.child(el().key(format!("stat-{index}-spacer")).w(INPUTS_WIDTH).shrink())
}

/// The row's checkbox: `far fa-square text-gray-500` / `fas fa-check-square`. `#price-window`
/// is `text-gray-200`, so a checked box is drawn in it (the check is a hole).
fn checkbox<S: 'static>(filter: &StatFilter, index: usize, project: Project<S>) -> El {
    let checked = !filter.disabled;
    let ink = theme::gray(500);
    let paper = theme::gray(200);
    row()
        .key(format!("stat-{index}-toggle"))
        .w(theme::rem(1.25))
        .items_center()
        .focusable(true)
        .role("button")
        .label(if checked { "checked" } else { "unchecked" })
        .on_press(move |s: &mut S| project(s).toggle_stat(index))
        .child(icon::checkbox(14.0, checked, ink, paper).mt(theme::rem(0.125)))
}

/// The text button: a press toggles the filter (`toggleFilter`; the keyboard path submits
/// the form, which the port leaves to the min/max fields' Enter key).
fn label<S: 'static>(filter: &StatFilter, index: usize, project: Project<S>) -> El {
    let roll = filter.roll.as_ref().map(|roll| roll.value);
    row()
        .key(format!("stat-{index}-label"))
        .grow(1.0)
        .min_w(0.0)
        .mr(theme::rem(0.25))
        .focusable(true)
        .role("button")
        .on_press(move |s: &mut S| project(s).toggle_stat(index))
        .child(modifier_text(&format!("stat-{index}-text"), &stat_text(filter), roll, type_::SM).truncate())
}

/// `FilterModifierTiers.vue`: the tier badges a modifier's sources carry.
#[expect(
    clippy::float_cmp,
    reason = "`info.tier === 1` / `=== 2`: tiers are integral in the trade data and compared exactly, as upstream does"
)]
fn tiers(filter: &StatFilter, item: &Item, index: usize) -> Option<El> {
    if matches!(item.category, Some(ItemCategory::Map | ItemCategory::Tablet)) {
        return None;
    }
    let mut tags: Vec<(&'static str, f64)> = Vec::new();
    let per_tier = matches!(
        filter.tag,
        FilterTag::EXPLICIT
            | FilterTag::DESECRATED
            | FilterTag::FRACTURED
            | FilterTag::CRAFTED
            | FilterTag::PSEUDO
            | FilterTag::Property
    ) && !matches!(
        item.category,
        Some(ItemCategory::Jewel | ItemCategory::ClusterJewel | ItemCategory::MemoryLine)
    );
    for source in &filter.sources {
        let Some(tier) = source.modifier.info.tier else { continue };
        if per_tier {
            if tier == 1.0 {
                tags.push(("tier-1", tier));
            } else if tier == 2.0 {
                tags.push(("tier-2", tier));
            } else if ALWAYS_SHOW_TIER {
                tags.push(("tier-3-plus", tier));
            }
        } else if tier >= 2.0 {
            tags.push(("not-tier-1", tier));
        }
    }
    if tags.is_empty() {
        return None;
    }
    tags.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out = row().key(format!("stat-{index}-tiers")).items_center().gap(theme::rem(0.25));
    for (position, (kind, tier)) in tags.iter().enumerate() {
        let label = i18n::fmt("filters.tier", &[&wraex_core::js::number_to_string(*tier)]);
        out = out.child(tier_badge(position, kind, &label));
    }
    Some(out)
}

/// One `.tier-1`, `.tier-2`, `.tier-3-plus` or `.not-tier-1` badge.
fn tier_badge(index: usize, kind: &'static str, label: &str) -> El {
    let key = format!("tier-{kind}-{index}");
    let text = icon::badge_text(label, Color::BLACK).key(format!("{key}-text"));
    let mut badge = row().key(key).radius(theme::ROUNDED).px(theme::rem(0.25)).center_y();
    match kind {
        "tier-1" => badge = badge.fill(theme::yellow(500)).child(text),
        "tier-2" => {
            badge = badge
                .border(1.0, theme::yellow(500))
                .border_layout()
                .my(-1.0)
                .child(icon::badge_text(label, theme::yellow(500)).key(format!("tier-{kind}-{index}-text")));
        }
        "not-tier-1" => {
            badge = badge
                .fill(theme::gray(700))
                .border(1.0, Color::BLACK)
                .border_layout()
                .my(-1.0)
                .child(icon::badge_text(label, Color::BLACK).key(format!("tier-{kind}-{index}-text")));
        }
        _ => badge = badge.fill(theme::gray(700)).child(text),
    }
    badge
}

/// `FilterModifier.vue`'s `showSourceInfo`.
pub(super) fn shows_source_info(check: &PriceCheck, filter: &StatFilter) -> bool {
    if !check.show_sources || filter.sources.is_empty() {
        return false;
    }
    // The elemental dps row keeps its sources: its option buttons sit beside them.
    let elemental =
        filter.trade_id.as_deref().and_then(|ids| ids.first()).map(String::as_str) == Some("item.elemental_dps");
    if filter.option.is_some() && !elemental {
        return false;
    }
    filter.tag == FilterTag::PSEUDO
        || filter.sources.len() >= 2
        || filter.sources.first().is_some_and(|source| {
            let info = &source.modifier.info;
            info.name.is_some() || info.tier.is_some() || info.rank.is_some()
        })
}
