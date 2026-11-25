//! The Inscribed Ultimatum check in the item check, modelled on the map check's card
//! ([`crate::poe1::map_check`]). Upstream has no Ultimatum support, so there is no reference.
//!
//! One row per rolled modifier with its tier's decision, the description under the name, the
//! profile buttons shared with the map check, and a verdict badge at the top: any deadly mark
//! is "Don't run", a warning is "Careful".

use rune::{Align, El, col, el, row, text};
use wraex_poe1::map_check as decisions;
use wraex_poe1::prices::{Prices, display_rounding};
use wraex_poe1::ultimatum::{Ultimatum, UltimatumModifier, value};
use wraex_poe1::ultimatum_check::{self as rules, UltimatumCheckConfig, Verdict};

use crate::poe1::i18n;
use crate::poe1::item_check::Project;
use crate::poe1::map_check::profile_picker;
use crate::theme::{self, text as type_};
use crate::ui::icon;

/// The tier numerals the game writes after a modifier's name (`I` is left off).
const TIERS: [&str; 4] = ["I", "II", "III", "IV"];

/// The item check's Ultimatum card.
pub fn view<S: 'static>(
    ultimatum: &Ultimatum,
    config: &UltimatumCheckConfig,
    profile: u32,
    prices: &Prices,
    project: Project<S>,
) -> El {
    let card = col()
        .key("ultimatum-check")
        .w_full()
        .fill(theme::gray(800))
        .border(theme::rem(0.25), theme::gray(900))
        .border_layout()
        .child(header(ultimatum, config, profile, project))
        .child(info(ultimatum, prices))
        .child(modifiers(ultimatum, config, profile, project));
    let card = match monsters(ultimatum) {
        Some(line) => card.child(line),
        None => card,
    };
    // The window is the app's, wider than the card: keep the card at its own width.
    col().key("ultimatum-check-window").w_full().items_start().child(card)
}

/// The title, the verdict badge, and the map check's profile buttons with the new-mod switch.
fn header<S: 'static>(ultimatum: &Ultimatum, config: &UltimatumCheckConfig, profile: u32, project: Project<S>) -> El {
    let mut title = row().key("ultimatum-check-title").w_full().gap(theme::rem(0.5)).items_center().child(
        text(i18n::t("ultimatum.title")).key("ultimatum-check-name").font(type_::SM).grow(1.0).align(Align::Center),
    );
    if let Some(badge) = verdict_badge(rules::verdict(config, profile, ultimatum)) {
        title = title.child(badge);
    }
    let second = row()
        .key("ultimatum-check-profile-row")
        .w_full()
        .gap(theme::rem(0.5))
        .items_center()
        .child(
            text(i18n::t("map_check.profile"))
                .key("ultimatum-check-profile-label")
                .font(type_::SM)
                .color(theme::gray(400)),
        )
        .child(profile_picker("ultimatum-check", profile, project))
        .child(el().grow(1.0))
        .child(show_new_toggle(config, project));
    col()
        .key("ultimatum-check-header")
        .w_full()
        .fill(theme::gray(900))
        .py(theme::rem(0.25))
        .px(theme::rem(2.0))
        .gap(theme::rem(0.25))
        .child(title)
        .child(second)
}

/// The skull or warning badge; a fine roll gets no badge at all.
fn verdict_badge(verdict: Verdict) -> Option<El> {
    let (label, fill, glyph) = match verdict {
        Verdict::Deadly => (i18n::t("ultimatum.dont_run"), theme::red(700), icon::skull as fn(f32, rune::Color) -> El),
        Verdict::Warning => {
            (i18n::t("ultimatum.careful"), theme::orange(600), icon::warning as fn(f32, rune::Color) -> El)
        }
        Verdict::Fine => return None,
    };
    Some(
        row()
            .key("ultimatum-check-verdict")
            .px(theme::rem(0.5))
            .py(theme::rem(0.125))
            .gap(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .items_center()
            .fill(fill)
            .child(glyph(14.0, theme::gray(100)).key("ultimatum-check-verdict-icon"))
            .child(text(label).key("ultimatum-check-verdict-text").font(type_::SM).color(theme::gray(100))),
    )
}

/// The new-mod icon's switch, as the maps settings page draws it: whether an unmarked
/// modifier shows the unseen eye.
fn show_new_toggle<S: 'static>(config: &UltimatumCheckConfig, project: Project<S>) -> El {
    let label = i18n::t("map_check.new_mods_icon");
    let on = config.show_new_mods;
    row()
        .key("ultimatum-check-show-new")
        .gap(theme::rem(0.25))
        .items_center()
        .focusable(true)
        .role("button")
        .label(label.clone())
        .on_press(move |s: &mut S| {
            let config = &mut project(s).ultimatum;
            config.show_new_mods = !config.show_new_mods;
        })
        .child(text(label).key("ultimatum-check-show-new-label").font(type_::XS).color(theme::gray(500)).truncate())
        .child(
            icon::toggle(on, if on { theme::gray(300) } else { theme::gray(600) }).key("ultimatum-check-show-new-icon"),
        )
}

/// The challenge, the area, both sides of the deal, and what poe.ninja says it is worth.
fn info(ultimatum: &Ultimatum, prices: &Prices) -> El {
    let mut block = col()
        .key("ultimatum-check-info")
        .w_full()
        .py(theme::rem(0.5))
        .px(theme::rem(2.0))
        .gap(theme::rem(0.125))
        .child(info_line("challenge", &i18n::fmt("ultimatum.challenge", &[ultimatum.challenge.label()])));
    if let Some(level) = ultimatum.area_level {
        block = block.child(info_line("area-level", &i18n::fmt("ultimatum.area_level", &[&level.to_string()])));
    }
    block = block
        .child(info_line("sacrifice", &i18n::fmt("ultimatum.sacrifice", &[&sacrifice_text(ultimatum)])))
        .child(info_line("reward", &i18n::fmt("ultimatum.reward", &[ultimatum.reward.description()])));
    if let Some(line) = value_line(ultimatum, prices) {
        block = block.child(line);
    }
    block
}

fn info_line(key: &str, content: &str) -> El {
    text(content.to_owned()).key(format!("ultimatum-check-{key}")).font(type_::SM).color(theme::gray(200))
}

/// `Vaal Orb ×20`; a unique has no stack count.
fn sacrifice_text(ultimatum: &Ultimatum) -> String {
    match ultimatum.sacrifice.count {
        Some(count) => format!("{} \u{d7}{count}", ultimatum.sacrifice.name),
        None => ultimatum.sacrifice.name.clone(),
    }
}

/// "Sacrifice worth 87c · Reward worth 174c", once the price list has the units.
fn value_line(ultimatum: &Ultimatum, prices: &Prices) -> Option<El> {
    if prices.is_empty() {
        return None;
    }
    let value = value(ultimatum, crate::poe1::price_check::data(), prices);
    let part = |key: &str, chaos: Option<f64>| chaos.map(|chaos| i18n::fmt(key, &[&compact_price(prices, chaos)]));
    let parts: Vec<String> =
        [part("ultimatum.value_sacrifice", value.sacrifice), part("ultimatum.value_reward", value.reward)]
            .into_iter()
            .flatten()
            .collect();
    (!parts.is_empty()).then(|| {
        text(parts.join(" \u{b7} "))
            .key("ultimatum-check-value")
            .font(type_::SM)
            .color(theme::gray(400))
            .py(theme::rem(0.125))
    })
}

/// `compact_price`: the value in the orb poe.ninja prices it in, as the price check writes it.
fn compact_price(prices: &Prices, chaos: f64) -> String {
    let price = prices.auto_currency(chaos);
    let amount = display_rounding(price.min, false);
    match price.currency {
        "div" => format!("{amount}div"),
        _ => format!("{amount}c"),
    }
}

/// One row per rolled modifier: the decision button, the name and tier, the description, and
/// the eye column when the new-mod icon is on.
fn modifiers<S: 'static>(
    ultimatum: &Ultimatum,
    config: &UltimatumCheckConfig,
    profile: u32,
    project: Project<S>,
) -> El {
    let mut rows = col().key("ultimatum-check-mods").w_full();
    for (index, modifier) in ultimatum.modifiers.iter().enumerate() {
        rows = rows.child(modifier_row(modifier, index, config, profile, project));
    }
    rows
}

fn modifier_row<S: 'static>(
    modifier: &UltimatumModifier,
    index: usize,
    config: &UltimatumCheckConfig,
    profile: u32,
    project: Project<S>,
) -> El {
    let current = rules::decision(config, profile, &modifier.name, modifier.tier);
    let (fill, glyph) = match current {
        Some('d') => (Some(theme::red(700)), Some(icon::skull as fn(f32, rune::Color) -> El)),
        Some('w') => (Some(theme::orange(600)), Some(icon::warning as fn(f32, rune::Color) -> El)),
        Some('g') => (Some(theme::green(700)), Some(icon::check as fn(f32, rune::Color) -> El)),
        _ => (None, None),
    };
    let name = modifier.name.clone();
    let tier = modifier.tier;
    let mut cycle = row()
        .key(format!("ultimatum-mod-{index}-cycle"))
        .padding(theme::rem(0.5))
        .gap(theme::rem(0.5))
        .items_center()
        .grow(1.0)
        .min_w(0.0)
        .focusable(true)
        .role("button")
        .label(format!("{} {}", modifier.name, TIERS[usize::from(tier.saturating_sub(1)).min(3)]))
        .on_press(move |s: &mut S| {
            let check = project(s);
            let profile = check.maps.profile;
            let config = &mut check.ultimatum;
            let current = rules::decision(config, profile, &name, tier).map_or_else(|| "-".to_owned(), String::from);
            let next = decisions::next_decision(&current);
            rules::set_decision(config, profile, &name, tier, next);
        });
    cycle = match fill {
        Some(color) => cycle.fill(color),
        None => cycle.hover(theme::gray(700)),
    };
    // The first column keeps the text indented whether or not it draws a glyph.
    cycle = match glyph {
        Some(draw) => cycle.child(draw(14.0, theme::gray(200)).key(format!("ultimatum-mod-{index}-icon"))),
        None => cycle.child(el().w(theme::rem(1.0))),
    };
    cycle = cycle.child(modifier_text(modifier, index));

    // The eye only acts while the new-mod icon is on; it marks this tier seen, not the rest.
    let seen = config.show_new_mods && current.is_none_or(|value| value == '-');
    let name = modifier.name.clone();
    let mut eye = row()
        .key(format!("ultimatum-mod-{index}-seen"))
        .w(theme::rem(2.0))
        .justify_center()
        .items_center()
        .focusable(true)
        .role("button")
        .label("seen")
        .on_press(move |s: &mut S| {
            let check = project(s);
            if !check.ultimatum.show_new_mods {
                return;
            }
            let profile = check.maps.profile;
            let config = &mut check.ultimatum;
            let current = rules::decision(config, profile, &name, tier).map_or_else(|| "-".to_owned(), String::from);
            let next = if current == "-" { "s" } else { "-" };
            rules::set_tier_decision(config, profile, &name, tier, next);
        });
    if seen {
        eye = eye.child(icon::eye_slash(14.0, theme::gray(600)).key(format!("ultimatum-mod-{index}-seen-icon")));
    }

    row().key(format!("ultimatum-mod-{index}")).w_full().child(cycle).child(eye)
}

/// The modifier's name with its numeral (from tier II up) and its muted description.
fn modifier_text(modifier: &UltimatumModifier, index: usize) -> El {
    let mut lines = col().key(format!("ultimatum-mod-{index}-text")).grow(1.0).min_w(0.0);
    let mut name =
        row().key(format!("ultimatum-mod-{index}-name")).gap(theme::rem(0.25)).items_center().child(
            text(modifier.name.clone()).key(format!("ultimatum-mod-{index}-name-text")).font(type_::SM).truncate(),
        );
    if modifier.tier > 1 {
        name = name.child(
            text(TIERS[usize::from(modifier.tier.saturating_sub(1)).min(3)])
                .key(format!("ultimatum-mod-{index}-tier"))
                .font(type_::XS)
                .color(theme::gray(400)),
        );
    }
    lines = lines.child(name);
    for (line, content) in modifier.description.split('\n').enumerate() {
        lines = lines.child(
            text(content.to_owned())
                .key(format!("ultimatum-mod-{index}-description-{line}"))
                .font(type_::XS)
                .color(theme::gray(500))
                .wrap(),
        );
    }
    lines
}

/// "Monsters: 10% increased damage, 30% more life": the encounter's own multiplier, display only.
fn monsters(ultimatum: &Ultimatum) -> Option<El> {
    let parts: Vec<String> = [
        ultimatum.monster_damage.map(|value| i18n::fmt("ultimatum.monster_damage", &[&value.to_string()])),
        ultimatum.monster_life.map(|value| i18n::fmt("ultimatum.monster_life", &[&value.to_string()])),
    ]
    .into_iter()
    .flatten()
    .collect();
    if parts.is_empty() {
        return None;
    }
    Some(
        row()
            .key("ultimatum-check-monsters")
            .w_full()
            .py(theme::rem(0.5))
            .px(theme::rem(2.0))
            .border_sides([theme::rem(0.25), 0.0, 0.0, 0.0], theme::gray(900))
            .child(
                text(i18n::fmt("ultimatum.monsters", &[&parts.join(", ")]))
                    .key("ultimatum-check-monsters-text")
                    .font(type_::XS)
                    .color(theme::gray(500)),
            ),
    )
}
