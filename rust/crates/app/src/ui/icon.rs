//! Stand-ins for upstream's Font Awesome icons.
//!
//! Font Awesome is a `node_modules` dependency of the reference (`@fortawesome/fontawesome-free`,
//! imported by `web/App.vue`) and CI has no `node_modules`, so the icons the ported widgets draw
//! are plain Rune shapes instead. They are placeholders: replace them with a bundled icon font or
//! upstream's own SVG set when one is available.
//!
//! Reference icons used by the price-check frame: `fa-window-close` (`AppTitlebar.vue`),
//! `fa-arrow-right` (`ItemQuickPrice.vue`, the price row's "→ ?") and `fa-external-link-alt`
//! (`TradeLinks.vue`). The filter block adds `fa-check-square`/`fa-square`, the tier chevrons,
//! `UiToggle`'s `fa-toggle-on`/`fa-toggle-off`, `fa-eye-slash` and `fa-exclamation-triangle`.

use rune::{Color, El, LineCap, Shape, canvas, el, row, text};

use crate::theme::{self, text as type_};

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

/// `fas fa-check-square` (`checked`) or `far fa-square`: the filter row's checkbox, `size`
/// points square. The cut-out check shows `ink` through the white box, as the icon font's
/// glyph does.
pub fn checkbox(size: f32, checked: bool, ink: Color, paper: Color) -> El {
    if !checked {
        return el().size(size, size).radius(2.0).border(1.5, ink).a11y_hidden();
    }
    let s = size;
    let shape = Shape::new().stroke(vec![[s * 0.2, s * 0.52], [s * 0.42, s * 0.74], [s * 0.8, s * 0.26]], 2.0, ink);
    canvas(shape).size(size, size).radius(2.5).fill(paper).a11y_hidden()
}

/// `fas fa-chevron-up` or `fa-chevron-down`, `size` points square.
pub fn chevron(size: f32, up: bool, color: Color) -> El {
    let s = size - STROKE;
    let mut shape = Shape::new();
    if up {
        shape = shape.line([0.0, s * 0.65], [s / 2.0, s * 0.25], STROKE, LineCap::Round, color).line(
            [s / 2.0, s * 0.25],
            [s, s * 0.65],
            STROKE,
            LineCap::Round,
            color,
        );
    } else {
        shape = shape.line([0.0, s * 0.35], [s / 2.0, s * 0.75], STROKE, LineCap::Round, color).line(
            [s / 2.0, s * 0.75],
            [s, s * 0.35],
            STROKE,
            LineCap::Round,
            color,
        );
    }
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-chevron-right` (`size` points square), the collapsed group's marker.
pub fn chevron_right(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let shape = Shape::new().line([s * 0.35, 0.0], [s * 0.75, s / 2.0], STROKE, LineCap::Round, color).line(
        [s * 0.75, s / 2.0],
        [s * 0.35, s],
        STROKE,
        LineCap::Round,
        color,
    );
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-toggle-on` / `fa-toggle-off` (`UiToggle.vue`): a 14 × 8 pt switch.
pub fn toggle(on: bool, color: Color) -> El {
    let (w, h) = (14.0, 8.0);
    let paper = theme::gray(800);
    row()
        .size(w, h)
        .radius(h / 2.0)
        .fill(if on { color } else { paper })
        .border(1.0, color)
        .border_layout()
        .items_center()
        .pl(if on { 5.5 } else { 1.5 })
        .child(el().size(5.0, 5.0).radius(2.5).fill(if on { paper } else { color }))
        .a11y_hidden()
}

/// `fas fa-eye-slash`: the hidden-filter marker (the row's `hiddenReason` popover trigger).
pub fn eye_slash(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let shape = Shape::new()
        // The eye's two lids, flattened.
        .line([0.0, s / 2.0], [s / 2.0, s * 0.2], STROKE, LineCap::Round, color)
        .line([s / 2.0, s * 0.2], [s, s / 2.0], STROKE, LineCap::Round, color)
        .line([0.0, s / 2.0], [s / 2.0, s * 0.8], STROKE, LineCap::Round, color)
        .line([s / 2.0, s * 0.8], [s, s / 2.0], STROKE, LineCap::Round, color)
        // The slash.
        .line([0.0, s], [s, 0.0], STROKE, LineCap::Round, color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-exclamation-triangle`: an unknown modifier's marker.
pub fn warning(size: f32, color: Color) -> El {
    let s = size - 1.0;
    let triangle = Shape::new()
        .stroke(vec![[s / 2.0, 0.0], [s, s], [0.0, s]], STROKE, color)
        .line([s / 2.0, s * 0.32], [s / 2.0, s * 0.68], 1.5, LineCap::Round, color)
        .line([s / 2.0, s * 0.84], [s / 2.0, s * 0.86], 2.0, LineCap::Round, color);
    canvas(triangle).size(size, size).a11y_hidden()
}

/// A 1 rem placeholder square for an item, currency or oil image the data layer does not
/// resolve yet (`<img>` tags in the reference).
pub fn image_placeholder(size: f32) -> El {
    el().size(size, size).radius(theme::ROUNDED).fill(theme::gray(700)).a11y_hidden()
}

/// `text` in the tag style: a 12 pt uppercase-ish label (the app's small-caps face renders it
/// as capitals), used by the tier and tag badges.
pub fn badge_text(label: &str, color: Color) -> El {
    text(label).font(theme::leading_none(type_::XS)).color(color)
}
