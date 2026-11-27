//! Upstream's Font Awesome icons, drawn from the bundled font.
//!
//! The reference draws every icon as an `<i class="fas fa-…">` in
//! `@fortawesome/fontawesome-free`, a `node_modules` dependency CI does not have. Its two faces
//! are vendored instead (`assets/fontawesome/`, [`theme::FAMILY_FA`]), so an icon here is the
//! real glyph: a one-character `text()` in Font Awesome, weight 900 for `fas` and 400 for `far`,
//! `line-height: 1` (`fontawesome.css`'s `.fas`), at the `font-size` upstream inherits.
//!
//! Like upstream's `<i>`, a glyph is as wide as its advance (0.625–1.25 em, not a square) and its
//! ink is the text colour; the counters (a `fa-check-square`'s check, a `fa-check-circle`'s tick)
//! are holes, so the background shows through them.
//!
//! Glyphs used by the price-check frame: `fa-window-close` (`AppTitlebar.vue`), `fa-arrow-right`
//! (`ItemQuickPrice.vue`, the price row's "→ ?"), `fa-external-link-alt` (`TradeLinks.vue`), the
//! filter block's `fa-check-square`/`fa-square`, tier chevrons, `UiToggle`'s
//! `fa-toggle-on`/`fa-toggle-off`, `fa-eye-slash` and `fa-exclamation-triangle`. The item and map
//! checks add `fa-skull-crossbones`, `fa-check`, `fa-ellipsis-vertical` and `fa-dollar`.

use rune::tok::TextStyle;
use rune::{Color, El, el, text};

use crate::theme::{self, text as type_};

/// `fas`: the solid face, `@font-face` weight 900.
const SOLID: f32 = 900.0;
/// `far`: the regular face, weight 400.
const REGULAR: f32 = 400.0;

/// One Font Awesome glyph, `size` points, `weight` 900 (`fas`) or 400 (`far`).
fn glyph(character: char, size: f32, weight: f32, color: Color) -> El {
    text(character.to_string())
        .font(TextStyle::new(size, weight).family(theme::FAMILY_FA).lh(size))
        .color(color)
        .a11y_hidden()
}

/// `fas fa-window-close` (`AppTitlebar.vue`), `size` points.
pub fn close(size: f32, color: Color) -> El {
    glyph('\u{f410}', size, SOLID, color)
}

/// `fas fa-times`: the plain close cross (`WidgetItemSearch.vue`'s reset, a matcher's remove).
/// Distinct from [`close`], upstream's boxed `fa-window-close`.
pub fn times(size: f32, color: Color) -> El {
    glyph('\u{f00d}', size, SOLID, color)
}

/// `fas fa-plus`: the chat page's "add command" button (`settings/chat.vue`).
pub fn plus(size: f32, color: Color) -> El {
    glyph('+', size, SOLID, color)
}

/// `fas fa-info-circle` (`settings/hotkeys.vue`'s clear-hotkey hint).
pub fn info_circle(size: f32, color: Color) -> El {
    glyph('\u{f05a}', size, SOLID, color)
}

/// `fas fa-trash-alt`: an outdated map-check stat's remove.
pub fn trash(size: f32, color: Color) -> El {
    glyph('\u{f2ed}', size, SOLID, color)
}

/// `fas fa-arrow-right`, the price row's "→ ?". `w` and `h` were the stand-in's box; the glyph
/// is `0.875 em` wide in a `1 em` line, as upstream's `<i>` is.
pub fn arrow_right(w: f32, h: f32, color: Color) -> El {
    glyph('\u{f061}', w.min(h), SOLID, color)
}

/// `fas fa-history` (`OnlineFilter.vue`).
pub fn history(size: f32, color: Color) -> El {
    glyph('\u{f1da}', size, SOLID, color)
}

/// `far fa-circle` (`UiRadio.vue`'s unchecked state), or `fas fa-check-circle` when checked.
pub fn radio(size: f32, checked: bool, color: Color) -> El {
    if checked { check_circle(size, color, color) } else { glyph('\u{f111}', size, REGULAR, color) }
}

/// `fas fa-check-circle`: a solid disc with its check cut out (`UiRadio.vue`'s checked state).
///
/// The cut-out is a hole in the glyph, so `ink` (what the page shows through) is only kept for
/// the callers that pass it; `color` is the disc.
pub fn check_circle(size: f32, color: Color, ink: Color) -> El {
    let _ = ink;
    glyph('\u{f058}', size, SOLID, color)
}

/// `fas fa-external-link-alt`, `size` points: a box a link opens out of.
pub fn external_link(size: f32, color: Color) -> El {
    glyph('\u{f35d}', size, SOLID, color)
}

/// `fas fa-check-square` (`checked`) or `far fa-square`: the filter row's checkbox, `size`
/// points.
///
/// `ink` is the unchecked box's colour and `paper` the checked box's, which upstream
/// takes from the row's text colour (`#price-window` is `text-gray-200`, the disabled filter
/// `text-gray-500`); the check itself is a hole in the glyph.
pub fn checkbox(size: f32, checked: bool, ink: Color, paper: Color) -> El {
    if checked { glyph('\u{f14a}', size, SOLID, paper) } else { glyph('\u{f0c8}', size, REGULAR, ink) }
}

/// `fas fa-chevron-up` or `fa-chevron-down`, `size` points.
pub fn chevron(size: f32, up: bool, color: Color) -> El {
    glyph(if up { '\u{f077}' } else { '\u{f078}' }, size, SOLID, color)
}

/// `fas fa-exchange-alt` (`PriceCheckWindow.vue`'s `⇄`): two arrows passing each other.
pub fn exchange(size: f32, color: Color) -> El {
    glyph('\u{f362}', size, SOLID, color)
}

/// `fas fa-angle-double-up` / `fa-angle-double-down` (`PriceTrend.vue`'s forecast).
pub fn chevrons(size: f32, up: bool, color: Color) -> El {
    glyph(if up { '\u{f102}' } else { '\u{f103}' }, size, SOLID, color)
}

/// `fas fa-chevron-right` (`size` points square), the collapsed group's marker.
pub fn chevron_right(size: f32, color: Color) -> El {
    glyph('\u{f054}', size, SOLID, color)
}

/// `fas fa-toggle-on` / `fa-toggle-off` (`UiToggle.vue`): a switch at the row's text size.
pub fn toggle(on: bool, color: Color) -> El {
    glyph(if on { '\u{f205}' } else { '\u{f204}' }, type_::SM.size, SOLID, color)
}

/// `fas fa-eye-slash`: the hidden-filter marker (the row's `hiddenReason` popover trigger).
pub fn eye_slash(size: f32, color: Color) -> El {
    glyph('\u{f070}', size, SOLID, color)
}

/// `fas fa-exclamation-triangle`: an unknown modifier's marker.
pub fn warning(size: f32, color: Color) -> El {
    glyph('\u{f071}', size, SOLID, color)
}

/// `fas fa-skull-crossbones` (`MapStatButton.vue`'s deadly decision).
pub fn skull(size: f32, color: Color) -> El {
    glyph('\u{f714}', size, SOLID, color)
}

/// `fas fa-check` (`MapStatButton.vue`'s good decision).
pub fn check(size: f32, color: Color) -> El {
    glyph('\u{f00c}', size, SOLID, color)
}

/// `fa-solid fa-ellipsis-vertical` (`ItemInfo.vue`'s separator): three dots.
pub fn ellipsis_vertical(size: f32, color: Color) -> El {
    glyph('\u{f142}', size, SOLID, color)
}

/// `fas fa-dollar` (EE2's "find same price" button).
pub fn dollar(size: f32, color: Color) -> El {
    glyph('$', size, SOLID, color)
}

/// A dark square for an image that is still loading or never arrived: an item's CDN art, a
/// currency icon. [`art`](crate::ui::art) draws the picture over it once it has one.
pub fn image_placeholder(size: f32) -> El {
    el().size(size, size).radius(theme::ROUNDED).fill(theme::gray(700)).a11y_hidden()
}

/// `fas fa-question` (`TradeItem.vue`): a listing with no note.
pub fn question(size: f32, color: Color) -> El {
    glyph('?', size, SOLID, color)
}

/// `text` in the tag style: a 12 pt uppercase-ish label (the app's small-caps face renders it
/// as capitals), used by the tier and tag badges.
pub fn badge_text(label: &str, color: Color) -> El {
    text(label).font(theme::leading_none(type_::XS)).color(color)
}
