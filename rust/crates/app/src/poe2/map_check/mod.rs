//! `ref/ee2/renderer/src/web/map-check/MapCheck.vue` and `MapStatButton.vue` in Rune.
//!
//! EE2's map check is APT's with three differences: every stat is listed (in APT only the
//! explicit ones are), each row is keyed by its text and type, and the unknown-modifier lines
//! are not filtered to explicit mods. The waystone's name is the item's own (`info.name`), and
//! its picture comes from `info.map.screenshot`. The decisions themselves are
//! [`wraex_poe2::map_check`]'s; this module is only the widget.

use rune::{Align, El, col, el, row, text};
use wraex_core::Rarity;
use wraex_poe2::map_check::{self, MapCheckConfig, PreparedStat};
use wraex_poe2::parser::item::Item;

use crate::poe2::i18n;
use crate::poe2::item_check::Project;
use crate::theme::{self, text as type_};
use crate::trade::Mode;
use crate::ui::{self, art, icon};

/// The three profile numerals, as `MapCheck.vue` lists them.
const PROFILES: [&str; 3] = ["I", "II", "III"];

/// `MapCheck.vue`: the map check card.
pub fn view<S: 'static>(item: &Item, config: &MapCheckConfig, mode: Mode, project: Project<S>) -> El {
    let data = crate::poe2::price_check::data(mode);
    let outdated = config.selected_stats.iter().any(|stat| map_check::is_outdated(data, config.profile, stat));

    let mut card = col()
        .key("map-check")
        .w_full()
        .fill(theme::gray(800))
        .border(theme::rem(0.25), theme::gray(900))
        .border_layout()
        .child(header(item, config, project));
    if let Some(url) = image(item) {
        card = card.child(screenshot(&url));
    }
    card = card.child(body(item, config, project));
    if outdated {
        card = card.child(
            col()
                .key("map-check-outdated")
                .w_full()
                .py(theme::rem(0.5))
                .px(theme::rem(2.0))
                .fill(theme::gray(700))
                .child(
                    text(i18n::t("map_check.has_outdated"))
                        .key("map-check-outdated-text")
                        .font(type_::SM)
                        .color(theme::gray(200))
                        .wrap(),
                ),
        );
    }
    // The window is the app's, taller than the card: keep the card at its own height so the
    // reference's screenshot and the port's line up.
    col().key("map-check-window").w_full().items_start().child(card)
}

/// `MapCheck.vue`'s header: the waystone's name, then `Profile` and the three numerals.
fn header<S: 'static>(item: &Item, config: &MapCheckConfig, project: Project<S>) -> El {
    let profile = config.profile;
    let mut profiles = row().key("map-check-profiles").gap(2.0).items_center();
    for (index, numeral) in PROFILES.iter().enumerate() {
        let number = index as u32 + 1;
        let mut button = row()
            .key(format!("map-check-profile-{number}"))
            .w(theme::rem(1.5))
            .justify_center()
            .fill(theme::gray(800))
            .focusable(true)
            .role("button")
            .label(*numeral)
            .on_press(move |s: &mut S| project(s).maps.profile = number)
            .child(text(*numeral).key(format!("map-check-profile-{number}-text")).font(type_::SM));
        if profile == number {
            button = button.border(1.0, theme::gray(600)).border_layout();
        }
        profiles = profiles.child(button);
    }
    row()
        .key("map-check-header")
        .w_full()
        .fill(theme::gray(900))
        .py(theme::rem(0.25))
        .px(theme::rem(2.0))
        .gap(theme::rem(0.5))
        .items_center()
        .child(
            text(item.info().name.clone().unwrap_or_else(|| "undefined".to_owned()))
                .key("map-check-name")
                .font(type_::SM)
                .grow(1.0)
                .align(Align::Center)
                .truncate(),
        )
        .child(
            text(i18n::t("map_check.profile"))
                .key("map-check-profile-label")
                .font(type_::SM)
                .color(theme::gray(400))
                .ml(theme::rem(2.0)),
        )
        .child(profiles)
}

/// `image`: `item.info.map?.screenshot`, hidden for an unidentified unique (its art gives it away).
fn image(item: &Item) -> Option<String> {
    if item.rarity == Some(Rarity::Unique) && item.is_unidentified {
        return None;
    }
    item.info().map.as_ref().and_then(|map| map.screenshot.clone())
}

/// `FullscreenImage`'s 21:9 box (`style="height: auto"`): the screenshot, filling it.
fn screenshot(url: &str) -> El {
    let width = crate::poe2::price_check::WIDTH - theme::rem(0.25) * 2.0;
    let height = width * 9.0 / 21.0;
    el().key("map-check-image").w_full().aspect_ratio(21.0 / 9.0).fill(theme::gray(700)).child(
        art::map_screenshot(wraex_core::GameId::Poe2, url, (width, height)).key("map-check-image-picture").pin(
            Some(0.0),
            Some(0.0),
            Some(0.0),
            Some(0.0),
        ),
    )
}

/// The rows, or "Item has no modifiers."; `py-2` on the wrapper, as EE2 puts it.
fn body<S: 'static>(item: &Item, config: &MapCheckConfig, project: Project<S>) -> El {
    let stats = map_check::prepare_map_stats(item);
    if stats.is_empty() {
        return col()
            .key("map-check-no-mods")
            .w_full()
            .px(theme::rem(2.0))
            .py(theme::rem(0.5))
            .child(text(i18n::t("map_check.no_mods")).key("map-check-no-mods-text").font(type_::SM));
    }
    let mut rows = col().key("map-check-stats").w_full().py(theme::rem(0.5));
    for (index, stat) in stats.iter().enumerate() {
        rows = rows.child(stat_button(stat, index, config, project));
    }
    for (index, unknown) in item.unknown_modifiers.iter().enumerate() {
        rows = rows.child(unknown_modifier(&unknown.text, index));
    }
    rows
}

/// `MapStatButton.vue`: the decision button and the eye column.
fn stat_button<S: 'static>(stat: &PreparedStat, index: usize, config: &MapCheckConfig, project: Project<S>) -> El {
    let current_decision = decision(config, &stat.matcher);
    let (fill, icon) = match current_decision {
        Some('d') => (Some(theme::red(700)), Some(icon::skull as fn(f32, rune::Color) -> El)),
        Some('w') => (Some(theme::orange(600)), Some(icon::warning as fn(f32, rune::Color) -> El)),
        Some('g') => (Some(theme::green(700)), Some(icon::check as fn(f32, rune::Color) -> El)),
        _ => (None, None),
    };
    let matcher = stat.matcher.clone();
    let mut cycle = row()
        .key(format!("stat-{index}-cycle"))
        .padding(theme::rem(0.5))
        .gap(theme::rem(0.5))
        .items_center()
        .grow(1.0)
        .min_w(0.0)
        .focusable(true)
        .role("button")
        .label(stat.matcher.clone())
        .on_press(move |s: &mut S| {
            let config = &mut project(s).maps;
            let current = decision(config, &matcher).map_or_else(|| "-".to_owned(), String::from);
            let next = map_check::next_decision(&current);
            set_decision(config, &matcher, next);
        });
    cycle = match fill {
        Some(c) => cycle.fill(c),
        None => cycle.hover(theme::gray(700)),
    };
    cycle = match icon {
        Some(draw) => cycle.child(draw(14.0, theme::gray(200)).key(format!("stat-{index}-icon"))),
        None => cycle.child(el().w(theme::rem(1.0))),
    };
    cycle = cycle.child(
        ui::item_modifier_text(&format!("stat-{index}-text"), &stat.matcher, stat.roll, theme::leading_none(type_::SM))
            .truncate(),
    );

    // `newStatIconVisible`: `decision.value === '-'`, the getter's value when there is no entry.
    let seen = config.show_new_stats && current_decision.is_none_or(|value| value == '-');
    let matcher = stat.matcher.clone();
    let mut eye = row()
        .key(format!("stat-{index}-seen"))
        .w(theme::rem(2.0))
        .justify_center()
        .focusable(true)
        .role("button")
        .label("seen")
        .on_press(move |s: &mut S| {
            let config = &mut project(s).maps;
            if !config.show_new_stats {
                return;
            }
            let current = decision(config, &matcher).map_or_else(|| "-".to_owned(), String::from);
            let next = if current == "-" { "s" } else { "-" };
            set_decision(config, &matcher, next);
        });
    if seen {
        eye = eye.child(icon::eye_slash(14.0, theme::gray(600)).key(format!("stat-{index}-seen-icon")));
    }

    row().key(format!("stat-{index}")).w_full().child(cycle).child(eye)
}

/// `MapCheck.vue`'s unknown-modifier line: the orange label, the em dash, then the text.
pub fn unknown_modifier(text_: &str, index: usize) -> El {
    let label = format!("{} \u{2014} ", i18n::t("Not recognized modifier"));
    let content = format!("{label}{text_}");
    let span = rune::Span::new(0, label.len()).color(theme::orange(400));
    col().key(format!("map-check-unknown-{index}")).w_full().py(theme::rem(0.25)).px(theme::rem(2.0)).child(
        rune::rich(content, vec![span])
            .key(format!("map-check-unknown-{index}-text"))
            .font(type_::SM)
            .color(theme::gray(200))
            .wrap(),
    )
}

/// The current profile's decision for a stat text.
pub fn decision(config: &MapCheckConfig, matcher: &str) -> Option<char> {
    let entry = config.selected_stats.iter().find(|entry| entry.matcher == matcher)?;
    entry.decision.chars().nth(config.profile.checked_sub(1)? as usize)
}

/// `decision.value = …`: `decisionCreate` into the entry, pushing a new one when there is none.
pub fn set_decision(config: &mut MapCheckConfig, matcher: &str, value: &str) {
    let profile = config.profile;
    let update =
        config.selected_stats.iter().find(|entry| entry.matcher == matcher).map(|entry| entry.decision.clone());
    let next = map_check::decision_create(value, profile, update.as_deref().unwrap_or("---"));
    match config.selected_stats.iter_mut().find(|entry| entry.matcher == matcher) {
        Some(entry) => entry.decision = next,
        None => config
            .selected_stats
            .push(wraex_poe2::map_check::MapCheckStat { matcher: matcher.to_owned(), decision: next }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_decision_is_per_profile() {
        let mut config = MapCheckConfig {
            profile: 1,
            show_new_stats: false,
            selected_stats: vec![wraex_poe2::map_check::MapCheckStat {
                matcher: "#% increased Monster Damage".to_owned(),
                decision: "w--".to_owned(),
            }],
        };
        assert_eq!(decision(&config, "#% increased Monster Damage"), Some('w'));
        set_decision(&mut config, "#% increased Monster Damage", "g");
        assert_eq!(config.selected_stats[0].decision, "g--");
        config.profile = 3;
        set_decision(&mut config, "Area contains two Unique Bosses", "d");
        assert_eq!(config.selected_stats[1].decision, "--d");
    }

    /// EE2's `image`: `item.info.map?.screenshot`. The waystones in the corpus carry no
    /// picture, so the card has no image block.
    #[test]
    fn a_waystone_has_no_picture_in_the_data() {
        let data = crate::poe2::price_check::data(crate::trade::Mode::Replay);
        let item = wraex_poe2::parser::parse_clipboard(data, &crate::corpus_poe2("ee2-rare-map")).unwrap();
        assert_eq!(item.info().name.as_deref(), Some("Waystone (Tier 14)"));
        assert!(image(&item).is_none());
    }
}
