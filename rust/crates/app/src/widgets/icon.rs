//! The Font Awesome glyphs the overlay widgets draw, as Rune shapes.
//!
//! The reference's icons are a `node_modules` font (`@fortawesome/fontawesome-free`), which CI
//! has no copy of, so these are stand-ins like [`crate::ui::icon`]'s: the right silhouette at the
//! right size, not the font's curves. One function per glyph, `size` points square (icons are
//! square except the ellipsis, which is `w` × `h`).
//!
//! Used: `fa-cog` (the settings button), `fa-search` (item search), `fa-book` (EE2's library),
//! `fa-ellipsis-h` (the menu's add button), `fa-play`/`fa-pause`/`fa-redo` (the stopwatch),
//! `fa-times` and `fa-arrows-alt` (the image strip's editing overlay) and `fa-file-import` (its
//! "Choose File").

use std::f32::consts::TAU;

use rune::{Color, El, LineCap, Shape, canvas, col, el, row, text, tok::TextStyle};

/// The stroke width at the 16 pt icon size the menu draws.
const STROKE: f32 = 1.5;

/// A glyph by its Font Awesome name; `None` for a name the port has no shape for.
pub fn named(name: &str, size: f32, color: Color) -> Option<El> {
    Some(match name {
        "fa-cog" => cog(size, color),
        "fa-search" => search(size, color),
        "fa-book" => book(size, color),
        "fa-ellipsis-h" => ellipsis_horizontal(size * 0.875, size, color),
        "fa-ellipsis-v" => ellipsis_vertical(size, color),
        "fa-play" => play(size, color),
        "fa-pause" => pause(size, color),
        "fa-redo" => redo(size, color),
        "fa-times" => times(size, color),
        "fa-arrows-alt" => arrows_alt(size, color),
        "fa-file-import" => file_import(size, color),
        _ => return None,
    })
}

/// A circle approximated as a closed polyline, for [`Shape::stroke`].
fn ring_points(cx: f32, cy: f32, r: f32, steps: usize) -> Vec<[f32; 2]> {
    (0..steps)
        .map(|step| {
            let angle = step as f32 / steps as f32 * TAU;
            [cx + r * angle.cos(), cy + r * angle.sin()]
        })
        .collect()
}

/// An arc from `from` to `to` radians (counter-clockwise from +x), as a polyline.
fn arc_points(cx: f32, cy: f32, r: f32, from: f32, to: f32, steps: usize) -> Vec<[f32; 2]> {
    (0..=steps)
        .map(|step| {
            let angle = from + (to - from) * step as f32 / steps as f32;
            [cx + r * angle.cos(), cy + r * angle.sin()]
        })
        .collect()
}

/// `fas fa-cog`: a ring with eight teeth.
pub fn cog(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let c = s / 2.0;
    let mut shape = Shape::new().stroke_with(ring_points(c, c, s * 0.24, 20), true, STROKE, LineCap::Round, color);
    for tooth in 0..8 {
        let angle = tooth as f32 / 8.0 * TAU;
        let (sin, cos) = angle.sin_cos();
        shape = shape.line(
            [c + s * 0.31 * cos, c + s * 0.31 * sin],
            [c + s * 0.46 * cos, c + s * 0.46 * sin],
            STROKE * 1.5,
            LineCap::Butt,
            color,
        );
    }
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-search`: a magnifying glass.
pub fn search(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let shape = Shape::new()
        .stroke_with(ring_points(s * 0.42, s * 0.42, s * 0.29, 20), true, STROKE, LineCap::Round, color)
        .line([s * 0.64, s * 0.64], [s * 0.95, s * 0.95], STROKE * 1.4, LineCap::Round, color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-book`: an open book, two pages either side of the spine.
pub fn book(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let spine = s / 2.0;
    let shape = Shape::new()
        .stroke_with(
            vec![[0.0, s * 0.2], [spine, s * 0.1], [spine, s * 0.9], [0.0, s * 0.8]],
            true,
            STROKE,
            LineCap::Round,
            color,
        )
        .stroke_with(
            vec![[s, s * 0.2], [spine, s * 0.1], [spine, s * 0.9], [s, s * 0.8]],
            true,
            STROKE,
            LineCap::Round,
            color,
        );
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-ellipsis-h`: three dots in a row, `w` × `h` points.
pub fn ellipsis_horizontal(w: f32, h: f32, color: Color) -> El {
    let dot = (h * 0.2).max(2.0);
    let mut dots = row().size(w, h).center().gap((w - 3.0 * dot) / 2.0);
    for _ in 0..3 {
        dots = dots.child(el().size(dot, dot).radius(dot / 2.0).fill(color));
    }
    dots.a11y_hidden()
}

/// `fas fa-ellipsis-v`: three dots stacked.
pub fn ellipsis_vertical(size: f32, color: Color) -> El {
    let dot = (size * 0.2).max(2.0);
    let mut dots = col().size(size, size).center().gap((size - 3.0 * dot) / 2.0);
    for _ in 0..3 {
        dots = dots.child(el().size(dot, dot).radius(dot / 2.0).fill(color));
    }
    dots.a11y_hidden()
}

/// `fas fa-play`: a filled triangle.
pub fn play(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let shape = Shape::new().fill(vec![[s * 0.22, s * 0.12], [s * 0.92, s * 0.5], [s * 0.22, s * 0.88]], color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-pause`: two filled bars.
pub fn pause(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let shape = Shape::new()
        .fill(vec![[s * 0.24, s * 0.12], [s * 0.42, s * 0.12], [s * 0.42, s * 0.88], [s * 0.24, s * 0.88]], color)
        .fill(vec![[s * 0.58, s * 0.12], [s * 0.76, s * 0.12], [s * 0.76, s * 0.88], [s * 0.58, s * 0.88]], color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-redo`: an almost-closed ring with an arrowhead at its top right.
pub fn redo(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let c = s / 2.0;
    let r = s * 0.38;
    // From `-250°` round to `60°`, leaving the gap the arrow head sits in.
    let arc = arc_points(c, c, r, -250f32.to_radians(), 60f32.to_radians(), 22);
    let head = [c + r * 60f32.to_radians().cos(), c + r * 60f32.to_radians().sin()];
    let shape = Shape::new().stroke_with(arc, false, STROKE, LineCap::Round, color).fill(
        vec![
            [head[0] - s * 0.1, head[1] - s * 0.16],
            [head[0] + s * 0.16, head[1] - s * 0.1],
            [head[0], head[1] + s * 0.14],
        ],
        color,
    );
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-times`: the close cross.
pub fn times(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let shape = Shape::new().line([0.0, 0.0], [s, s], STROKE, LineCap::Round, color).line(
        [s, 0.0],
        [0.0, s],
        STROKE,
        LineCap::Round,
        color,
    );
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-arrows-alt`: a four-way arrow.
pub fn arrows_alt(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let c = s / 2.0;
    let head = s * 0.16;
    let shape = Shape::new()
        .line([0.0, c], [s, c], STROKE, LineCap::Butt, color)
        .line([c, 0.0], [c, s], STROKE, LineCap::Butt, color)
        .fill(vec![[0.0, c], [head * 1.6, c - head], [head * 1.6, c + head]], color)
        .fill(vec![[s, c], [s - head * 1.6, c - head], [s - head * 1.6, c + head]], color)
        .fill(vec![[c, 0.0], [c - head, head * 1.6], [c + head, head * 1.6]], color)
        .fill(vec![[c, s], [c - head, s - head * 1.6], [c + head, s - head * 1.6]], color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-file-import`: a tray with an arrow going into it.
pub fn file_import(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let shape = Shape::new()
        .stroke_with(
            vec![[0.05 * s, s * 0.6], [0.05 * s, s * 0.92], [s * 0.95, s * 0.92], [s * 0.95, s * 0.6]],
            false,
            STROKE,
            LineCap::Round,
            color,
        )
        .line([s * 0.5, 0.0], [s * 0.5, s * 0.62], STROKE, LineCap::Butt, color)
        .line([s * 0.28, s * 0.4], [s * 0.5, s * 0.64], STROKE, LineCap::Round, color)
        .line([s * 0.72, s * 0.4], [s * 0.5, s * 0.64], STROKE, LineCap::Round, color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// A label in the icon's place, for a name with no shape: the glyph's text, so a missing icon is
/// visible rather than silent.
pub fn fallback(name: &str, size: f32, color: Color) -> El {
    text(name.trim_start_matches("fa-").to_owned())
        .font(TextStyle::new(size * 0.5, 400.0).lh(size))
        .color(color)
        .a11y_hidden()
}
