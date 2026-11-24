//! Stand-ins for upstream's Font Awesome icons.
//!
//! Font Awesome is a `node_modules` dependency of the reference (`@fortawesome/fontawesome-free`,
//! imported by `web/App.vue`) and CI has no `node_modules`, so the icons the ported widgets draw
//! are plain Rune shapes instead. They are placeholders: replace them with a bundled icon font or
//! upstream's own SVG set when one is available.
//!
//! Reference icons used by the price-check frame: `fa-window-close` (`AppTitlebar.vue`),
//! `fa-arrow-right` (`ItemQuickPrice.vue`, the price row's "→ ?") and `fa-external-link-alt`
//! (`TradeLinks.vue`).

use rune::{Color, El, LineCap, Shape, canvas};

/// The icon font's stroke at the 14 pt body size.
const STROKE: f32 = 1.25;

/// `fas fa-window-close`, `size` points square.
pub fn close(size: f32, color: Color) -> El {
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

/// `fas fa-arrow-right`, `w` × `h` points: a shaft with a chevron head.
pub fn arrow_right(w: f32, h: f32, color: Color) -> El {
    let half = h / 2.0;
    let head = h / 3.0;
    let shape = Shape::new()
        .line([0.0, half], [w, half], STROKE, LineCap::Round, color)
        .line([w - head, half - head], [w, half], STROKE, LineCap::Round, color)
        .line([w - head, half + head], [w, half], STROKE, LineCap::Round, color);
    canvas(shape).size(w, h).a11y_hidden()
}

/// `fas fa-external-link-alt`, `size` points square: a box a link opens out of.
pub fn external_link(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let low = s / 2.0;
    let shape = Shape::new()
        // The box, open towards its top-right corner.
        .stroke(vec![[0.0, low], [0.0, s], [s, s], [s, low]], STROKE, color)
        // The link leaving it.
        .line([low + 1.0, s - low - 1.0], [s, 0.0], STROKE, LineCap::Round, color)
        .line([s - low * 0.6, 0.0], [s, 0.0], STROKE, LineCap::Butt, color)
        .line([s, 0.0], [s, low * 0.6], STROKE, LineCap::Butt, color);
    canvas(shape).size(s, s).a11y_hidden()
}
