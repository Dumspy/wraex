//! `FilterModifier.vue`: one stat row — checkbox, text, tags, tiers, roll fields — with
//! `FilterModifierTiers.vue` and `FilterModifierOptions.vue` folded in.

use rune::{Color, El, col, el, row, text};
use wraex_core::Rarity;
use wraex_poe1::filters::{FilterTag, StatFilter};
use wraex_poe1::parser::calc_q20::get_trade_max_quality;
use wraex_poe1::parser::item::Item;
use wraex_poe1::parser::meta::ItemCategory;
use wraex_poe1::parser::modifiers::StatSource;

use crate::poe1::i18n;
use crate::poe1::price_check::view::Project;
use crate::poe1::price_check::{InputField, NumericPart, PriceCheck, StatPath};
use crate::theme::{self, text as type_};
use crate::ui::art;
use crate::ui::icon;

use super::{
    FieldStyle, ROLL_WIDTH, is_mini, modifier_text, number_field, shows_quality, shows_tag, stat_text, tag_badge,
    tag_label,
};

/// `FilterModifier.vue`.
///
/// `expanded` is `Some` for a `FilterGroup`'s own row (with its chevron), `None` otherwise.
/// `input_override` replaces the default min/max slot, as `FilterGroup.vue` does for a
/// mercenary's 6-link row (the gem links).
#[expect(
    clippy::too_many_arguments,
    reason = "one argument per prop FilterModifier.vue takes, plus the view's state projection"
)]
pub fn view<S: 'static>(
    check: &PriceCheck,
    filter: &StatFilter,
    item: &Item,
    grouped: bool,
    expanded: Option<bool>,
    path: StatPath,
    input_override: Option<El>,
    project: Project<S>,
) -> El {
    let key = path.key();
    let mini = is_mini(filter, item);
    let show_inputs = filter.roll.is_some() && filter.oils.is_none();
    let show_bounds = item.rarity == Some(Rarity::Unique);
    let options = roll_options(filter);

    let mut main = col().grow(1.0).min_w(0.0);
    let mut first = row().key(format!("stat-{key}-line")).w_full().min_w(0.0).items_center();
    let mut label_area = row().grow(1.0).shrink().items_center().mr(theme::rem(0.5));
    label_area = label_area.child(checkbox(filter, path, project));
    label_area = label_area.child(label(filter, mini, path, project));
    if let Some(open) = expanded {
        let group = match path {
            StatPath::Meta(group) => group,
            StatPath::Top(index) => index,
            StatPath::Child(group, _) => group,
        };
        label_area = label_area.child(
            row()
                .key(format!("stat-{key}-expand"))
                .w(theme::rem(1.25))
                .pl(theme::rem(0.375))
                .items_center()
                .focusable(true)
                .role("button")
                .label("expand")
                .on_press(move |s: &mut S| project(s).toggle_group_expanded(group))
                .child(if open {
                    icon::chevron(theme::rem(0.875), false, theme::gray(500))
                } else {
                    icon::chevron_right(theme::rem(0.875), theme::gray(500))
                }),
        );
    }
    first = first.child(label_area);
    if mini && !show_inputs {
        first = first.child(tiers(filter, item, path));
    }
    if let Some(inputs) = input_override {
        first = first.child(inputs);
    } else if show_inputs {
        first = first.child(inputs(check, filter, item, path, project));
    } else if mini && let Some(options) = &options {
        first = first.child(options_view(path, filter, options, false, project));
    }
    main = main.child(first);

    if !mini {
        let mut second = row().key(format!("stat-{key}-meta")).w_full().pt(1.0);
        let mut gutter = row().w(theme::rem(1.25)).items_start();
        if filter.hidden.is_some() {
            gutter = gutter.child(icon::eye_slash(theme::rem(0.75), theme::gray(600)).mt(theme::rem(0.125)));
        }
        second = second.child(gutter);
        let mut body = col().grow(1.0).min_w(0.0);
        let mut line = row().w_full().min_w(0.0).items_start();
        let mut tags = row().grow(1.0).min_w(0.0).items_center().gap(theme::rem(0.5));
        if filter.not == Some(true) {
            tags = tags.child(tag_badge(&i18n::t("filters.tag_not"), FilterTag::PSEUDO));
        }
        if shows_tag(filter, item, grouped) {
            tags = tags.child(tag_badge(&tag_label(filter), filter.tag));
        }
        if !show_bounds {
            tags = tags.child(tiers(filter, item, path));
        }
        if let Some(options) = &options {
            tags = tags.child(options_view(path, filter, options, true, project));
        }
        line = line.child(tags).child(el().w(theme::rem(6.0) + 1.0));
        body = body.child(line);
        if let Some(reminder) = &filter.reminder_text {
            body = body.child(
                text(reminder)
                    .key(format!("stat-{key}-reminder"))
                    .font(type_::SM.italic())
                    .color(theme::gray(500))
                    .fill(theme::gray(800))
                    .pr(theme::rem(0.5)),
            );
        }
        second = second.child(body);
        main = main.child(second);
    }

    let mut root = row()
        .key(format!("stat-{key}"))
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
                check.source_hover = Some(path);
            } else if check.source_hover == Some(path) {
                check.source_hover = None;
            }
        });
    }
    if grouped {
        root = root.pl(theme::rem(1.25));
    }
    root = root.child(main);
    if let Some(anointment) = super::anointment::view(filter) {
        root = root.child(row().key(format!("stat-{key}-anointment")).items_center().child(anointment));
    }
    root
}

/// The row's checkbox: `far fa-square text-gray-500`/`fas fa-check-square`,
/// `filter.disabled = !disabled`. `#price-window` is `text-gray-200`, so a checked box is drawn
/// in it (the check is a hole); a disabled row's outline is `gray-500`.
fn checkbox<S: 'static>(filter: &StatFilter, path: StatPath, project: Project<S>) -> El {
    let checked = !filter.disabled;
    let ink = theme::gray(500);
    let paper = theme::gray(200);
    row()
        .key(format!("stat-{}-toggle", path.key()))
        .w(theme::rem(1.25))
        .items_center()
        .focusable(true)
        .role("button")
        .label(if checked { "checked" } else { "unchecked" })
        .on_press(move |s: &mut S| project(s).toggle_stat(path))
        .child(icon::checkbox(14.0, checked, ink, paper))
}

/// The text button: `smartToggle` on a press.
fn label<S: 'static>(filter: &StatFilter, mini: bool, path: StatPath, project: Project<S>) -> El {
    let key = path.key();
    let mut button = row()
        .key(format!("stat-{key}-label"))
        .shrink()
        .items_center()
        .focusable(true)
        .role("button")
        .on_press(move |s: &mut S| project(s).smart_toggle_stat(path));
    if let Some(icon) = filter.mercenary.as_ref().and_then(|mercenary| mercenary.icon.as_deref()) {
        // `<img v-if="filter.mercenary?.icon" :src="filter.mercenary.icon">`
        button = button.child(super::image(art::Picture::cdn(icon), theme::rem(1.0)).mr(theme::rem(0.25)));
    }
    if filter.not == Some(true) && mini {
        button = button.child(tag_badge(&i18n::t("filters.tag_not"), FilterTag::PSEUDO).mr(theme::rem(0.25)));
    }
    let roll = filter.roll.as_ref().map(|roll| roll.value);
    button.child(modifier_text(&format!("stat-{key}-text"), &stat_text(filter), roll, type_::SM).truncate())
}

/// `FilterModifierTiers.vue`: the tier badges a modifier's sources carry.
fn tiers(filter: &StatFilter, item: &Item, path: StatPath) -> El {
    let mut tags: Vec<(&'static str, f64)> = Vec::new();
    if let Some(mercenary) = &filter.mercenary
        && let Some(tier) = mercenary.tier
    {
        let kind = match tier {
            4 => "merc-gilded",
            3 => "tier-1",
            _ if mercenary.max_tier == Some(true) => "tier-2",
            _ => "not-tier-1",
        };
        tags.push((kind, f64::from(tier)));
    }
    let jewel_like = matches!(
        item.category,
        Some(ItemCategory::Jewel | ItemCategory::ClusterJewel | ItemCategory::MemoryLine | ItemCategory::Idol)
    );
    let property_like =
        matches!(filter.tag, FilterTag::EXPLICIT | FilterTag::PSEUDO) || filter.tag == FilterTag::Property;
    for source in filter.stat_sources() {
        let Some(tier) = source.modifier.info.tier else { continue };
        if property_like && !jewel_like {
            match tier {
                t if t == 1.0 => tags.push(("tier-1", t)),
                t if t == 2.0 => tags.push(("tier-2", t)),
                _ => {}
            }
        } else if tier >= 2.0 {
            tags.push(("not-tier-1", tier));
        }
    }
    tags.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out = row().key(format!("stat-{}-tiers", path.key())).items_center().gap(theme::rem(0.25));
    for (index, (kind, tier)) in tags.iter().enumerate() {
        let label = if *kind == "merc-gilded" {
            i18n::t("filters.tier_gilded")
        } else {
            i18n::fmt("filters.tier", &[&wraex_core::js::number_to_string(*tier)])
        };
        out = out.child(tier_badge(index, kind, &label));
    }
    out
}

/// One `.tier-1`, `.tier-2`, `.not-tier-1` or `.merc-gilded` badge.
fn tier_badge(index: usize, kind: &'static str, label: &str) -> El {
    let mut badge = row().key(format!("tier-{kind}-{index}")).radius(theme::ROUNDED).px(theme::rem(0.25)).center_y();
    match kind {
        "tier-1" => badge = badge.fill(theme::yellow(500)).child(icon::badge_text(label, Color::BLACK)),
        "tier-2" => {
            badge =
                badge.border(1.0, theme::yellow(500)).border_layout().child(icon::badge_text(label, theme::yellow(500)))
        }
        "merc-gilded" => badge = badge.fill(theme::yellow(700)).child(icon::badge_text(label, theme::yellow(100))),
        _ => badge = badge.fill(theme::gray(700)).child(icon::badge_text(label, Color::BLACK)),
    }
    badge
}

/// `FilterModifierOptions.vue`'s `RollOption`.
struct RollOption {
    text: String,
    value: f64,
}

/// `FilterModifier.vue`'s `rollOptions`.
fn roll_options(filter: &StatFilter) -> Option<Vec<RollOption>> {
    if filter.tag == FilterTag::MercenarySupport && filter.option.is_some() {
        return Some(vec![
            RollOption { text: i18n::t("filters.option_merc_required"), value: 1.0 },
            RollOption { text: i18n::t("filters.option_merc_optional"), value: 0.0 },
        ]);
    }
    if filter.first_trade_id() == Some("item.has_empty_modifier") {
        return Some(vec![
            RollOption { text: i18n::t("filters.option_empty_affix"), value: 0.0 },
            RollOption { text: i18n::t("filters.option_empty_prefix"), value: 1.0 },
            RollOption { text: i18n::t("filters.option_empty_suffix"), value: 2.0 },
        ]);
    }
    None
}

/// The roll-option buttons. `always` draws the checked one with a dashed border (the second
/// line's style), otherwise a solid one while the filter itself is on.
fn options_view<S: 'static>(
    path: StatPath,
    filter: &StatFilter,
    options: &[RollOption],
    always: bool,
    project: Project<S>,
) -> El {
    let checked = filter.option.as_ref().map(|option| option.value);
    let enabled = !filter.disabled;
    let mut out = row().key(format!("stat-{}-options", path.key())).items_center().gap(theme::rem(0.25));
    for (index, option) in options.iter().enumerate() {
        let value = option.value;
        let is_checked = checked == Some(value);
        let mut button = row()
            .key(format!("stat-{}-option-{index}", path.key()))
            .min_w(theme::rem(2.5))
            .px(theme::rem(0.5))
            .items_center()
            .justify_center()
            .fill(theme::gray(700))
            .radius(theme::ROUNDED)
            .border_layout()
            .focusable(true)
            .role("button")
            .label(option.text.clone())
            .on_press(move |s: &mut S| project(s).click_stat_option(path, value));
        button = if is_checked && always {
            button
                .border(1.0, theme::gray(500))
                .border_dash(3.0, 2.0)
                .child(icon::badge_text(&option.text, theme::gray(400)))
        } else if is_checked && enabled {
            button.border(1.0, theme::gray(500)).child(icon::badge_text(&option.text, theme::gray(300)))
        } else {
            button.border(1.0, Color::TRANSPARENT).child(icon::badge_text(&option.text, theme::gray(400)))
        };
        out = out.child(button);
    }
    out
}

/// `FilterModifier.vue`'s input slot: the q20 notice, roll-source images, minus and plus.
fn inputs<S: 'static>(check: &PriceCheck, filter: &StatFilter, item: &Item, path: StatPath, project: Project<S>) -> El {
    let key = path.key();
    let mut block = row().key(format!("stat-{key}-inputs")).items_center().gap(theme::rem(0.25)).shrink();
    if shows_quality(filter.first_trade_id()) {
        block = block.child(
            text(i18n::fmt("item.prop_quality", &[&wraex_core::js::number_to_string(get_trade_max_quality(item))]))
                .key(format!("stat-{key}-quality"))
                .font(rune::tok::TextStyle::new(13.0, 400.0).lh(13.0))
                .color(theme::gray(500))
                .pr(theme::rem(0.25)),
        );
    }
    let (volatile, reflecting, increased) = roll_tags(filter);
    if volatile || reflecting {
        // `FilterModifier.vue`'s `rollTags`: `/images/VolatileVaalOrb.png` or
        // `/images/ReflectingMist.png`.
        let name = if volatile { "VolatileVaalOrb.png" } else { "ReflectingMist.png" };
        block = block.child(super::image(art::Picture::local(name), theme::rem(1.25)));
    }
    if increased {
        block = block.child(super::image(art::Picture::local("increased.png"), theme::rem(1.25)));
    }
    let min_field = InputField::Stat(path, NumericPart::Min);
    let max_field = InputField::Stat(path, NumericPart::Max);
    block.child(
        row()
            .items_center()
            .gap(1.0)
            .child(number_field(
                check,
                min_field,
                ROLL_WIDTH,
                "min",
                [theme::ROUNDED, 0.0, 0.0, theme::ROUNDED],
                FieldStyle::Roll,
                project,
            ))
            .child(number_field(
                check,
                max_field,
                ROLL_WIDTH,
                "max",
                [0.0, theme::ROUNDED, theme::ROUNDED, 0.0],
                FieldStyle::Roll,
                project,
            )),
    )
}

/// `FilterModifier.vue`'s `rollTags`: a volatile or reflecting mist, and whether quality (or
/// another increased roll) scales the stat.
fn roll_tags(filter: &StatFilter) -> (bool, bool, bool) {
    use wraex_poe1::parser::stat_translations::RollGeneration;

    let mut volatile = false;
    let mut reflecting = false;
    for source in filter.stat_sources() {
        match source.stat.roll.as_ref().and_then(|roll| roll.generation) {
            Some(RollGeneration::Volatile) => {
                volatile = true;
                break;
            }
            Some(RollGeneration::Reflecting) => {
                reflecting = true;
                break;
            }
            _ => {}
        }
    }
    let increased = filter.stat_sources().any(|source: &StatSource| {
        source.modifier.info.roll_incr.is_some() && source.stat.roll.as_ref().is_some_and(|roll| !roll.unscalable)
    });
    (volatile, reflecting, increased)
}

/// `FilterModifier.vue`'s `showSourceInfo`.
pub(super) fn shows_source_info(check: &PriceCheck, filter: &StatFilter) -> bool {
    if !check.show_sources || filter.sources.is_empty() || filter.option.is_some() {
        return false;
    }
    filter.tag == FilterTag::PSEUDO
        || filter.sources.len() >= 2
        || filter.stat_sources().next().is_some_and(|source| {
            let info = &source.modifier.info;
            info.name.is_some() || info.tier.is_some() || info.rank.is_some()
        })
}
