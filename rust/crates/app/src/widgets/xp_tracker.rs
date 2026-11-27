//! `ref/ee2/renderer/src/web/leveling/WidgetXpTracker.vue` in Rune: the experience tracker.
//!
//! The first client-log-driven widget. It reads the snapshot the live shell publishes
//! ([`crate::client_log`]) — the current character level, the area's monster level, the zone
//! and the character — and shows the two numbers EE2's widget shows:
//!
//! - **Exp**: the experience gained from a monster of the area's level, as a percentage. The
//!   widget marks it with `*` when the area is over level 70 or the character is level 95+,
//!   where EE2's author could not test the percentages.
//! - **Over**: how far the area's level is above the bottom of the "safe zone" (the level band
//!   with no under-levelled penalty).
//!
//! The maths is a direct port of the Vue component's `calcBaseSafeZone`, `getOverIdeal` and
//! `getExpPenalty`, with the same `Math.max`/`Math.floor` and the same `100 * expMulti`
//! rounding (`toFixed(1)`). The characteristic level input is the character level: typing in
//! it overrides the log's level, as `setPlayerLevel` does upstream.
//!
//! The live shell only runs for PoE2 (`crate::client_log`); a scene feeds the widget a fixed
//! state (`--scene poe2-widget-xp-tracker`).

use rune::{Align, El, col, row, text, text_input};
use serde::Deserialize;

use super::lenient;
use crate::client_log::State;
use crate::theme::{self, text as type_};
use crate::widgets::{Overlay, Project, Widget};

/// `WidgetSpec.type`.
pub const WM_TYPE: &str = "experience-tracker";

/// The message the widget needs, for tests and the scene checklist.
pub const TITLE: &str = "XP";

/// `calcBaseSafeZone(playerLevel)`: `Math.floor(playerLevel / 16) + 3`.
pub fn calc_base_safe_zone(player_level: f64) -> f64 {
    (player_level / 16.0).floor() + 3.0
}

/// `getOverIdeal(playerLevel, monsterLevel)`:
/// `safeZone - (monsterLevel - playerLevel)`.
pub fn get_over_ideal(player_level: f64, monster_level: f64) -> f64 {
    let safe_zone = calc_base_safe_zone(player_level);
    safe_zone - (monster_level - player_level)
}

/// `getExpPenalty(playerLevel, monsterLevel)`: the percentage of a monster's experience the
/// player still receives.
///
/// The safe zone only applies when the monster is above the player
/// (`monsterLevel > playerLevel`), so being under-levelled has no safe band.
pub fn get_exp_penalty(player_level: f64, monster_level: f64) -> f64 {
    let safe_zone = if monster_level > player_level { calc_base_safe_zone(player_level) } else { 0.0 };
    let effective_diff = ((monster_level - player_level).abs() - safe_zone).max(0.0);
    let exp_multi = ((player_level + 5.0) / (player_level + 5.0 + effective_diff.powf(2.5))).powf(1.3).max(0.01);
    100.0 * exp_multi
}

/// `expPenalty.toFixed(1)`: what the widget prints, and the string its colour compares.
pub fn exp_penalty_text(player_level: f64, monster_level: f64) -> String {
    format!("{:.1}", get_exp_penalty(player_level, monster_level))
}

/// `areaLevel > 70 || characterLevel >= 95`: the asterisk beside `Exp`.
pub fn has_asterisk(player_level: f64, area_level: f64) -> bool {
    area_level > 70.0 || player_level >= 95.0
}

/// The fixed state the `poe2-widget-xp-tracker` scene draws: level 72 in a level 80 area, so
/// both numbers are out of their ideal range (a red `-1` and a `98.3%` penalty) and the
/// asterisk is on.
pub fn scene_state() -> State {
    State { area_level: 80.0, player_level: 72.0, zone: "g1_9".to_owned(), character: "Wraex".to_owned() }
}

/// The widget's config fields this module reads.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrackerView {
    #[serde(default, deserialize_with = "lenient")]
    show_exp: Option<bool>,
}

/// `config.showExp`, on unless the config says otherwise.
fn show_exp(widget: &Widget) -> bool {
    TrackerView::deserialize(&widget.config).unwrap_or_default().show_exp.unwrap_or(true)
}

/// The widget's card: the level input, `Exp: n%` and `Over: n`.
// The green band is `over === 0 || over === 1`: an exact comparison of the whole number
// `getOverIdeal` produces, as the Vue component's `===` is.
#[expect(clippy::float_cmp, reason = "`getOverIdeal` compares the exact whole-number result, as the Vue `===` does")]
pub fn view<S: 'static>(overlay: &Overlay, widget: &Widget, project: Project<S>) -> El {
    let state = &overlay.xp;
    let show_exp = show_exp(widget);
    let over = get_over_ideal(state.player_level, state.area_level);
    let penalty = exp_penalty_text(state.player_level, state.area_level);

    let font = type_::SM;
    let input = text_input(&overlay.xp_input, font)
        .key("xp-tracker-level")
        .h(font.line_px())
        .single_line()
        .align(Align::Center)
        .w(theme::rem(2.0))
        .px(theme::rem(0.25))
        .border(1.0, rune::Color::TRANSPARENT)
        .border_layout()
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        .caret(theme::gray(300), theme::gray(400).alpha(0.35))
        .color(theme::gray(300))
        .label("Character level")
        .on_edit(move |s: &mut S| &mut project(s).xp_input)
        .on_input_key(|_s: &mut S, event: &rune::KeyEvent, _tx: &mut rune::Tx| match &event.key {
            // `<input type="number" min="1" max="100" step="1">`: digits only, as the settings'
            // number fields. The min/max attributes do not stop typing in a browser either.
            rune::Key::Text(text) => !text.chars().all(|c| c.is_ascii_digit()),
            _ => false,
        })
        .on_input_change(move |s: &mut S, _tx: &mut rune::Tx| {
            let overlay = project(s);
            if let Ok(level) = overlay.xp_input.text.trim().parse::<f64>() {
                overlay.xp.player_level = level;
            }
        });

    // `.xpContainer`: `min-h-10 flex flex-row justify-between items-center px-2 rounded
    // bg-gray-800 border-4 border-gray-900`, `min-w-48`/`min-w-32` by `showExp`.
    let mut card = row()
        .key("xp-tracker")
        .min_h(theme::rem(2.5))
        .min_w(if show_exp { theme::rem(12.0) } else { theme::rem(8.0) })
        .px(theme::rem(0.5))
        .items_center()
        .justify_between()
        .radius(theme::ROUNDED)
        .fill(theme::gray(800))
        .border(4.0, theme::gray(900))
        .border_layout()
        .shadow(1.0, 3.0, rune::Color::BLACK.alpha(0.75))
        .child(input);

    if show_exp {
        let color = if penalty == "100.0" { theme::green(600) } else { theme::red(600) };
        let asterisk = if has_asterisk(state.player_level, state.area_level) { "*" } else { "" };
        card = card.child(
            row()
                .key("xp-tracker-exp")
                .items_center()
                .gap(theme::rem(0.25))
                .child(text(format!("Exp {asterisk}:")).key("xp-tracker-exp-label").font(font).color(theme::gray(100)))
                .child(text(format!("{penalty}%")).key("xp-tracker-exp-value").font(font).color(color)),
        );
    }

    let over_color = if over < 0.0 {
        theme::red(600)
    } else if over == 0.0 || over == 1.0 {
        theme::green(600)
    } else {
        theme::gray(100)
    };
    card.child(
        row()
            .key("xp-tracker-over")
            .items_center()
            .gap(theme::rem(0.25))
            .child(text("Over:").key("xp-tracker-over-label").font(font).color(theme::gray(100)))
            .child(
                text(wraex_core::js::number_to_string(over)).key("xp-tracker-over-value").font(font).color(over_color),
            ),
    )
}

/// The card's column root, for the scene's `widget::card` match.
pub fn card<S: 'static>(overlay: &Overlay, widget: &Widget, project: Project<S>) -> El {
    col().key("xp-tracker-card").child(view(overlay, widget, project))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hand-computed from the Vue component's three functions.
    #[test]
    fn the_safe_zone_is_the_reference_formula() {
        assert_eq!(calc_base_safe_zone(1.0), 3.0);
        assert_eq!(calc_base_safe_zone(15.0), 3.0);
        assert_eq!(calc_base_safe_zone(16.0), 4.0);
        assert_eq!(calc_base_safe_zone(72.0), 7.0);
        assert_eq!(calc_base_safe_zone(95.0), 8.0);
        assert_eq!(calc_base_safe_zone(100.0), 9.0);
    }

    #[test]
    fn over_is_the_safe_zone_minus_the_level_gap() {
        // Level 83 in a 90 area: safe zone floor(83/16)+3 = 8, gap 7, over 1.
        assert_eq!(get_over_ideal(83.0, 90.0), 1.0);
        // Level 72 in an 80 area: safe zone 7, gap 8, over -1.
        assert_eq!(get_over_ideal(72.0, 80.0), -1.0);
        // Under-levelled: the same formula, no clamping.
        assert_eq!(get_over_ideal(95.0, 90.0), 13.0);
        assert_eq!(get_over_ideal(1.0, 1.0), 3.0);
    }

    /// `100 * expMulti`, rounded to one decimal: the exact numbers the Vue code prints.
    #[test]
    fn the_experience_penalty_matches_the_reference() {
        // Monster above the player, inside the safe zone: no penalty.
        assert_eq!(get_exp_penalty(83.0, 90.0), 100.0);
        assert_eq!(exp_penalty_text(83.0, 90.0), "100.0");
        // One level past the safe zone: (72+5)/(77+1)^1.3 -> 98.3%.
        assert_eq!(exp_penalty_text(72.0, 80.0), "98.3");
        // Five levels past the safe zone, monster below the player: (100/155.9…)^1.3 -> 56.1%.
        assert_eq!(exp_penalty_text(95.0, 90.0), "56.1");
        // A huge gap hits the reference's `Math.max(0.01, …)` floor.
        assert_eq!(get_exp_penalty(1.0, 100.0), 1.0);
        assert_eq!(exp_penalty_text(1.0, 100.0), "1.0");
        // Equal levels: the safe zone does not apply, the gap is zero.
        assert_eq!(get_exp_penalty(100.0, 100.0), 100.0);
    }

    #[test]
    fn the_asterisk_follows_area_and_level() {
        assert!(!has_asterisk(94.0, 70.0));
        assert!(has_asterisk(95.0, 70.0));
        assert!(has_asterisk(72.0, 71.0));
        assert!(!has_asterisk(72.0, 70.0));
    }

    /// The scene's fixed state renders the values its golden screenshot shows.
    #[test]
    fn the_scene_state_prints_the_expected_values() {
        let state = scene_state();
        assert_eq!(get_over_ideal(state.player_level, state.area_level), -1.0);
        assert_eq!(exp_penalty_text(state.player_level, state.area_level), "98.3");
    }
}
