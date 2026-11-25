//! `ref/apt/renderer/src/web/map-check/MapCheck.vue` and `MapStatButton.vue` in Rune.
//!
//! The map check is the item check's other half (`WidgetItemCheck.vue`): for a map-like item
//! (a map, a heist contract or blueprint, an invitation, a chart, a logbook) it shows the map's
//! name, its screenshot, every explicit modifier with the per-profile decision colour, and the
//! "wording has changed" notice for a decided stat the game data no longer has.
//!
//! The decisions themselves are [`wraex_poe1::map_check`]'s (`common.ts`, `prepare-map-stats.ts`);
//! this module is only the widget. Clicking a row cycles its decision for the current profile,
//! the eye column marks a stat seen, and the three roman numerals switch profile.

use rune::{Align, El, col, el, row, text};
use wraex_core::GameId;
use wraex_core::Rarity;
use wraex_poe1::map_check::{self, MapCheckConfig, PreparedStat};
use wraex_poe1::parser::item::Item;
use wraex_poe1::parser::modifiers::ModifierType;

use crate::poe1::i18n;
use crate::poe1::item_check::Project;
use crate::theme::{self, text as type_};
use crate::ui::{self, art, icon};

/// `MapCheck.vue`'s `MEMORY_MAPS`: the maps Maven's Invitation needs a witness from.
const MEMORY_MAPS: [&str; 3] = ["Courtyard of Wasting", "Chambers of Impurity", "Theatre of Lies"];

/// The three profile numerals, as `MapCheck.vue` lists them.
const PROFILES: [&str; 3] = ["I", "II", "III"];

/// `MapCheck.vue`: the map check card.
pub fn view<S: 'static>(item: &Item, config: &MapCheckConfig, project: Project<S>) -> El {
    let data = crate::poe1::price_check::data();
    let outdated = config.selected_stats.iter().any(|stat| map_check::is_outdated(data, config.profile, stat));

    let mut card = col()
        .key("map-check")
        .w_full()
        .fill(theme::gray(800))
        .border(theme::rem(0.25), theme::gray(900))
        .border_layout()
        .child(header(item, config, project));
    let mut body_top_rule = false;
    if let Some(url) = image(item) {
        card = card.child(screenshot(&url));
        body_top_rule = true;
    }
    card = card.child(body(item, config, body_top_rule, project));
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

/// `MapCheck.vue`'s header: the map's name, then `Profile` and the three numerals.
fn header<S: 'static>(item: &Item, config: &MapCheckConfig, project: Project<S>) -> El {
    row()
        .key("map-check-header")
        .w_full()
        .fill(theme::gray(900))
        .py(theme::rem(0.25))
        .px(theme::rem(2.0))
        .gap(theme::rem(0.5))
        .items_center()
        .child(text(map_name(item)).key("map-check-name").font(type_::SM).grow(1.0).align(Align::Center).truncate())
        .child(
            text(i18n::t("map_check.profile"))
                .key("map-check-profile-label")
                .font(type_::SM)
                .color(theme::gray(400))
                .ml(theme::rem(2.0)),
        )
        .child(profile_picker("map-check", config.profile, project))
}

/// `Profile`'s three numerals: the map check and the Ultimatum check share them, because both
/// read the item check's one `maps.profile`.
pub fn profile_picker<S: 'static>(prefix: &'static str, profile: u32, project: Project<S>) -> El {
    let mut picker = row().key(format!("{prefix}-profiles")).gap(2.0).items_center();
    for (index, numeral) in PROFILES.iter().enumerate() {
        let number = index as u32 + 1;
        let mut button = row()
            .key(format!("{prefix}-profile-{number}"))
            .w(theme::rem(1.5))
            .justify_center()
            .fill(theme::gray(800))
            .focusable(true)
            .role("button")
            .label(*numeral)
            .on_press(move |s: &mut S| project(s).maps.profile = number)
            .child(text(*numeral).key(format!("{prefix}-profile-{number}-text")).font(type_::SM));
        // `:class="{ 'border border-gray-600': profile.active }"`
        if profile == number {
            button = button.border(1.0, theme::gray(600)).border_layout();
        }
        picker = picker.child(button);
    }
    picker
}

/// `mapName`: a blighted area's own name, else the base map's name.
fn map_name(item: &Item) -> String {
    if is_blighted(item) {
        return item.info().name.clone();
    }
    item.map_area.as_ref().map_or_else(|| item.info().name.clone(), |area| area.name.clone())
}

/// `item.info.area?.blighted`
fn is_blighted(item: &Item) -> bool {
    item.info().area.as_ref().is_some_and(|area| area.blighted == Some(true))
}

/// `image`: the map's screenshot, except for an unidentified unique map (its art gives it away)
/// and a blighted area (which has its own, unrecorded, picture).
fn image(item: &Item) -> Option<String> {
    if (item.rarity == Some(Rarity::Unique) && item.is_unidentified) || is_blighted(item) {
        return None;
    }
    item.info()
        .area
        .as_ref()
        .and_then(|area| area.screenshot.clone())
        .or_else(|| item.map_area.as_ref().and_then(|area| area.area.as_ref()?.screenshot.clone()))
}

/// `FullscreenImage`'s 21:9 box: the screenshot filling it, then the reference's white glow
/// (`::after`: an 8 % white wash with a lighter top-left border).
fn screenshot(url: &str) -> El {
    let width = crate::poe1::price_check::WIDTH - theme::rem(0.25) * 2.0;
    let height = width * 9.0 / 21.0;
    let glow = rune::Color::WHITE;
    el().key("map-check-image")
        .w_full()
        .aspect_ratio(21.0 / 9.0)
        .fill(theme::gray(700))
        .child(art::map_screenshot(GameId::Poe1, url, (width, height)).key("map-check-image-picture").pin(
            Some(0.0),
            Some(0.0),
            Some(0.0),
            Some(0.0),
        ))
        .child(
            el().key("map-check-image-glow")
                .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
                .fill(glow.alpha(0.08))
                .border_sides([2.0, 1.0, 1.0, 2.0], glow.alpha(0.18))
                .a11y_hidden(),
        )
}

/// The `.body`: the Maven reminder, then the rows (or "Item has no modifiers."). `rule` adds the
/// `border-top` a `.screenshot + .body` gets.
fn body<S: 'static>(item: &Item, config: &MapCheckConfig, rule: bool, project: Project<S>) -> El {
    let stats = map_check::map_stats(item);
    let mut body = col().key("map-check-body").w_full().py(theme::rem(0.5)).gap(theme::rem(0.25));
    if rule {
        body = body.border_sides([theme::rem(0.25), 0.0, 0.0, 0.0], theme::gray(900));
    }
    if has_maven_reminder(item) {
        let mut reminder = row()
            .key("map-check-maven")
            .w_full()
            .px(theme::rem(2.0))
            .gap(theme::rem(0.5))
            .items_center()
            .child(art::local(GameId::Poe1, "maven-witness.png", theme::rem(2.0)).key("map-check-maven-icon"));
        // `white-space: pre-wrap`: the message keeps its own line break.
        let mut lines = col().key("map-check-maven-lines").grow(1.0).min_w(0.0);
        for (index, line) in i18n::t("map_check.maven_reminder").split('\n').enumerate() {
            lines = lines.child(
                text(line.to_owned())
                    .key(format!("map-check-maven-text-{index}"))
                    .font(type_::SM)
                    .color(theme::gray(500))
                    .wrap(),
            );
        }
        reminder = reminder.child(lines);
        body = body.child(reminder);
    }
    if stats.is_empty() {
        body = body.child(
            row()
                .key("map-check-no-mods")
                .px(theme::rem(2.0))
                .child(text(i18n::t("map_check.no_mods")).key("map-check-no-mods-text").font(type_::SM)),
        );
    } else {
        let mut rows = col().key("map-check-stats").w_full();
        for (index, stat) in stats.iter().enumerate() {
            rows = rows.child(stat_button(stat, index, config, project));
        }
        for (index, unknown) in
            item.unknown_modifiers.iter().filter(|modifier| modifier.kind == ModifierType::Explicit).enumerate()
        {
            rows = rows.child(unknown_modifier(&unknown.text, index));
        }
        body = body.child(rows);
    }
    body
}

/// Whether Maven's Invitation needs a witness from the map's area.
fn has_maven_reminder(item: &Item) -> bool {
    let Some(area) = item.map_area.as_ref() else { return false };
    MEMORY_MAPS.contains(&area.ref_name.as_str())
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
        // `.normal:hover { bg-gray-700 }`
        None => cycle.hover(theme::gray(700)),
    };
    // The `<i class="w-4 shrink-0">` keeps the text indented whether or not it draws a glyph.
    cycle = match icon {
        Some(draw) => cycle.child(draw(14.0, theme::gray(200)).key(format!("stat-{index}-icon"))),
        None => cycle.child(el().w(theme::rem(1.0))),
    };
    cycle = cycle.child(
        ui::item_modifier_text(&format!("stat-{index}-text"), &stat.matcher, stat.roll, theme::leading_none(type_::SM))
            .truncate(),
    );

    // `toggleSeenStatus` only acts with the new-mods icon on; the eye is then a "not seen" mark.
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

/// `config.selectedStats.find(({ matcher }) => matcher === stat.matcher)` and the profile's
/// character in its decision set.
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
            .push(wraex_poe1::map_check::MapCheckStat { matcher: matcher.to_owned(), decision: next }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> MapCheckConfig {
        MapCheckConfig {
            profile: 1,
            show_new_stats: false,
            selected_stats: vec![
                wraex_poe1::map_check::MapCheckStat {
                    matcher: "Players have #% to all maximum Resistances".to_owned(),
                    decision: "w--".to_owned(),
                },
                wraex_poe1::map_check::MapCheckStat {
                    matcher: "#% increased Monster Movement Speed".to_owned(),
                    decision: "d--".to_owned(),
                },
            ],
        }
    }

    #[test]
    fn a_profiles_decision_comes_from_its_character() {
        let config = config();
        assert_eq!(decision(&config, "Players have #% to all maximum Resistances"), Some('w'));
        assert_eq!(decision(&config, "#% increased Monster Movement Speed"), Some('d'));
        assert_eq!(decision(&config, "not selected"), None);
    }

    #[test]
    fn clicking_cycles_and_creates_an_entry() {
        let mut config = config();
        set_decision(&mut config, "#% increased Monster Movement Speed", "g");
        assert_eq!(config.selected_stats[1].decision, "g--");
        // A stat with no entry is pushed with the default `---` set patched.
        set_decision(&mut config, "Area contains two Unique Bosses", "g");
        let entry = config.selected_stats.last().unwrap();
        assert_eq!((entry.matcher.as_str(), entry.decision.as_str()), ("Area contains two Unique Bosses", "g--"));
        // The second profile writes its own character.
        config.profile = 2;
        set_decision(&mut config, "#% increased Monster Movement Speed", "w");
        assert_eq!(config.selected_stats[1].decision, "gw-");
    }

    /// `mapName`, `image` and the Maven reminder: a blighted area is named after the item and
    /// has no picture, an area with a screenshot shows it, and the memory maps ask for a witness.
    #[test]
    fn the_name_picture_and_witness_follow_the_item() {
        let parse = |slug: &str| {
            wraex_poe1::parser::parse_clipboard(crate::poe1::price_check::data(), &crate::corpus(slug)).unwrap()
        };
        let valdo = parse("real-map-valdo-foil");
        assert_eq!(map_name(&valdo), "Beach");
        assert!(image(&valdo).is_some_and(|url| url.starts_with("https://i.imgur.com/")), "{:?}", image(&valdo));
        assert!(!has_maven_reminder(&valdo));

        let blighted = parse("real-map-blighted-corrupted");
        assert_eq!(map_name(&blighted), "Blighted Map");
        assert!(image(&blighted).is_none(), "a blighted area has its own, unrecorded, picture");

        let mut witness = parse("real-map-valdo-foil");
        let area = witness.map_area.as_mut().expect("the map's area");
        area.name = "Theatre of Lies".to_owned();
        area.ref_name = "Theatre of Lies".to_owned();
        assert!(has_maven_reminder(&witness));
        assert_eq!(map_name(&witness), "Theatre of Lies");
    }
}
