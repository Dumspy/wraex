//! The Font Awesome glyphs the overlay widgets draw.
//!
//! Same faces as [`crate::ui::icon`]: the `@fortawesome/fontawesome-free` glyphs upstream's
//! templates name in `wmTitle` (`{icon=fa-cog}`) and in its widget components, drawn as a
//! one-character `text()` in [`theme::FAMILY_FA`] at the row's text size. One function per glyph,
//! `size` points (the glyph is as wide as its advance, a square only for square icons).
//!
//! Used: `fa-cog` (the settings button), `fa-search` (item search), `fa-book` (EE2's library),
//! `fa-ellipsis-h` (the menu's add button), `fa-play`/`fa-pause`/`fa-redo` (the stopwatch),
//! `fa-times` and `fa-arrows-alt` (the image strip's editing overlay) and `fa-file-import` (its
//! "Choose File").

use rune::tok::TextStyle;
use rune::{Color, El, text};

use crate::theme;

/// `fas`: the solid face, `@font-face` weight 900.
const SOLID: f32 = 900.0;

/// One Font Awesome glyph, `size` points, `line-height: 1` as `.fas` sets it.
fn glyph(character: char, size: f32, color: Color) -> El {
    text(character.to_string())
        .font(TextStyle::new(size, SOLID).family(theme::FAMILY_FA).lh(size))
        .color(color)
        .a11y_hidden()
}

/// A glyph by its Font Awesome name; `None` for a name the port has no glyph for.
pub fn named(name: &str, size: f32, color: Color) -> Option<El> {
    Some(match name {
        "fa-cog" | "fa-gear" => cog(size, color),
        "fa-search" | "fa-magnifying-glass" => search(size, color),
        "fa-book" => book(size, color),
        "fa-ellipsis-h" | "fa-ellipsis" => ellipsis_horizontal(size, size, color),
        "fa-ellipsis-v" | "fa-ellipsis-vertical" => ellipsis_vertical(size, color),
        "fa-play" => play(size, color),
        "fa-pause" => pause(size, color),
        "fa-redo" | "fa-rotate-right" => redo(size, color),
        "fa-times" | "fa-xmark" => times(size, color),
        "fa-arrows-alt" | "fa-up-down-left-right" => arrows_alt(size, color),
        "fa-file-import" => file_import(size, color),
        _ => return None,
    })
}

/// `fas fa-cog`: the settings button.
pub fn cog(size: f32, color: Color) -> El {
    glyph('\u{f013}', size, color)
}

/// `fas fa-search`: a magnifying glass.
pub fn search(size: f32, color: Color) -> El {
    glyph('\u{f002}', size, color)
}

/// `fas fa-book`: EE2's library.
pub fn book(size: f32, color: Color) -> El {
    glyph('\u{f02d}', size, color)
}

/// `fas fa-ellipsis-h`: three dots in a row. `h` is the line's font size, as upstream's `<i>`
/// inherits it; `w` was the stand-in's box and is no longer used.
pub fn ellipsis_horizontal(w: f32, h: f32, color: Color) -> El {
    let _ = w;
    glyph('\u{f141}', h, color)
}

/// `fas fa-ellipsis-v`: three dots stacked.
pub fn ellipsis_vertical(size: f32, color: Color) -> El {
    glyph('\u{f142}', size, color)
}

/// `fas fa-play`: the stopwatch's start.
pub fn play(size: f32, color: Color) -> El {
    glyph('\u{f04b}', size, color)
}

/// `fas fa-pause`: the stopwatch's stop.
pub fn pause(size: f32, color: Color) -> El {
    glyph('\u{f04c}', size, color)
}

/// `fas fa-redo`: the stopwatch's reset.
pub fn redo(size: f32, color: Color) -> El {
    glyph('\u{f01e}', size, color)
}

/// `fas fa-times`: the close cross.
pub fn times(size: f32, color: Color) -> El {
    glyph('\u{f00d}', size, color)
}

/// `fas fa-arrows-alt`: the image strip's move handle.
pub fn arrows_alt(size: f32, color: Color) -> El {
    glyph('\u{f0b2}', size, color)
}

/// `fas fa-file-import`: a tray with an arrow going into it.
pub fn file_import(size: f32, color: Color) -> El {
    glyph('\u{f56f}', size, color)
}

/// A label in the icon's place, for a name with no glyph: the name's text, so a missing icon is
/// visible rather than silent.
pub fn fallback(name: &str, size: f32, color: Color) -> El {
    text(name.trim_start_matches("fa-").to_owned())
        .font(TextStyle::new(size * 0.5, 400.0).lh(size))
        .color(color)
        .a11y_hidden()
}
