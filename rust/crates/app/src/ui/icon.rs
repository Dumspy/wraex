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
//! The item and map checks add `fa-skull-crossbones`, `fa-check`, `fa-eye-slash`,
//! `fa-ellipsis-vertical` and `fa-dollar`.

use rune::{Color, El, LineCap, Shape, canvas, col, el, row, text, tok::TextStyle};

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

/// `fas fa-trash-alt`: a lid, a body and its two slots (an outdated map-check stat's remove).
pub fn trash(size: f32, color: Color) -> El {
    let s = size;
    let shape = Shape::new()
        .line([0.1 * s, 0.22 * s], [0.9 * s, 0.22 * s], STROKE, LineCap::Round, color)
        .line([0.38 * s, 0.22 * s], [0.42 * s, 0.1 * s], STROKE, LineCap::Round, color)
        .line([0.62 * s, 0.22 * s], [0.58 * s, 0.1 * s], STROKE, LineCap::Round, color)
        .stroke(vec![[0.2 * s, 0.3 * s], [0.25 * s, 0.9 * s], [0.75 * s, 0.9 * s], [0.8 * s, 0.3 * s]], STROKE, color)
        .line([0.42 * s, 0.42 * s], [0.44 * s, 0.78 * s], STROKE, LineCap::Round, color)
        .line([0.58 * s, 0.42 * s], [0.56 * s, 0.78 * s], STROKE, LineCap::Round, color);
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

/// `fas fa-history` (`OnlineFilter.vue`): a ring with a hand turned back to the left.
pub fn history(size: f32, color: Color) -> El {
    let ring = el().size(size, size).radius(size / 2.0).border(STROKE, color);
    let hand = {
        let s = size;
        let shape = Shape::new()
            .line([s * 0.5, s * 0.5], [s * 0.28, s * 0.5], STROKE, LineCap::Round, color)
            .line([s * 0.28, s * 0.5], [s * 0.4, s * 0.34], STROKE, LineCap::Round, color)
            .line([s * 0.28, s * 0.5], [s * 0.4, s * 0.66], STROKE, LineCap::Round, color);
        canvas(shape).size(size, size).a11y_hidden()
    };
    row().a11y_hidden().child(ring).child(hand)
}

/// `far fa-circle` / `fas fa-check-circle` (`UiRadio.vue`), `size` points square. The
/// checked one is a filled dot in its ring, close enough to the icon font's check.
pub fn radio(size: f32, checked: bool, color: Color) -> El {
    let mut ring = el().size(size, size).radius(size / 2.0).border(1.25, color).center();
    if checked {
        ring = ring.child(el().size(size * 0.45, size * 0.45).radius(size * 0.25).fill(color));
    }
    ring.a11y_hidden()
}

/// `fas fa-check-circle`: the solid disc with its check cut out (`UiRadio.vue`'s checked
/// state), so `ink` shows through the circle and the page shows through the check.
pub fn check_circle(size: f32, color: Color, ink: Color) -> El {
    let s = size;
    let shape = Shape::new().stroke(vec![[s * 0.26, s * 0.52], [s * 0.44, s * 0.72], [s * 0.76, s * 0.3]], 1.6, ink);
    canvas(shape).size(size, size).radius(size / 2.0).fill(color).a11y_hidden()
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

/// `fas fa-exchange-alt` (`PriceCheckWindow.vue`'s `⇄`): two horizontal arrows.
pub fn exchange(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let (top, bottom) = (s * 0.3, s * 0.7);
    let head = s * 0.24;
    let shape = Shape::new()
        .line([0.0, top], [s, top], STROKE, LineCap::Butt, color)
        .line([s - head, top - head], [s, top], STROKE, LineCap::Butt, color)
        .line([s - head, top + head], [s, top], STROKE, LineCap::Butt, color)
        .line([s, bottom], [0.0, bottom], STROKE, LineCap::Butt, color)
        .line([head, bottom - head], [0.0, bottom], STROKE, LineCap::Butt, color)
        .line([head, bottom + head], [0.0, bottom], STROKE, LineCap::Butt, color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fas fa-angle-double-up` / `fa-angle-double-down` (`PriceTrend.vue`'s forecast): two
/// chevrons stacked.
pub fn chevrons(size: f32, up: bool, color: Color) -> El {
    let s = size - STROKE;
    let wing = s * 0.28;
    // `up` draws two `^`, `down` two `v`.
    let (first, second, tip) = if up { (s * 0.46, s * 0.86, -wing) } else { (s * 0.14, s * 0.54, wing) };
    let shape = Shape::new()
        .line([0.0, first], [s / 2.0, first + tip], STROKE, LineCap::Round, color)
        .line([s / 2.0, first + tip], [s, first], STROKE, LineCap::Round, color)
        .line([0.0, second], [s / 2.0, second + tip], STROKE, LineCap::Round, color)
        .line([s / 2.0, second + tip], [s, second], STROKE, LineCap::Round, color);
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

/// `fas fa-skull-crossbones` (`MapStatButton.vue`'s deadly decision): a skull.
pub fn skull(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let eye = s * 0.17;
    let head = el().size(s, s * 0.8).radius(s * 0.4).border(STROKE, color).child(
        row()
            .w_full()
            .justify_center()
            .pt(s * 0.26)
            .gap(s * 0.2)
            .child(el().size(eye, eye).radius(eye / 2.0).fill(color))
            .child(el().size(eye, eye).radius(eye / 2.0).fill(color)),
    );
    let jaw = row()
        .w_full()
        .justify_center()
        .child(el().size(s * 0.42, s * 0.16).radius(1.0).border(STROKE, color).fill(color.alpha(0.25)));
    col().size(size, size).items_center().child(head).child(jaw).a11y_hidden()
}

/// `fas fa-check` (`MapStatButton.vue`'s good decision): a tick.
pub fn check(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let shape = Shape::new().line([s * 0.08, s * 0.52], [s * 0.4, s * 0.86], STROKE, LineCap::Round, color).line(
        [s * 0.4, s * 0.86],
        [s * 0.94, s * 0.16],
        STROKE,
        LineCap::Round,
        color,
    );
    canvas(shape).size(size, size).a11y_hidden()
}

/// `fa-solid fa-ellipsis-vertical` (`ItemInfo.vue`'s separator): three dots.
pub fn ellipsis_vertical(size: f32, color: Color) -> El {
    let dot = 2.0;
    let mut dots = col().size(size, size).items_center().justify_center().gap((size - 3.0 * dot) / 2.0);
    for _ in 0..3 {
        dots = dots.child(el().size(dot, dot).radius(dot / 2.0).fill(color));
    }
    dots.a11y_hidden()
}

/// `fas fa-dollar` (EE2's "find same price" button): the currency mark.
pub fn dollar(size: f32, color: Color) -> El {
    text("$").font(TextStyle::new(size * 0.8, 700.0).lh(size)).color(color).a11y_hidden()
}

/// A dark square for an image that is still loading or never arrived: an item's CDN art, a
/// currency icon. [`art`](crate::ui::art) draws the picture over it once it has one.
pub fn image_placeholder(size: f32) -> El {
    el().size(size, size).radius(theme::ROUNDED).fill(theme::gray(700)).a11y_hidden()
}

/// `fas fa-question` (`TradeItem.vue`): a listing with no note.
pub fn question(size: f32, color: Color) -> El {
    let s = size - STROKE;
    let shape = Shape::new()
        // The hook of the `?`.
        .line([s * 0.25, s * 0.3], [s * 0.5, s * 0.1], STROKE, LineCap::Round, color)
        .line([s * 0.5, s * 0.1], [s * 0.75, s * 0.3], STROKE, LineCap::Round, color)
        .line([s * 0.75, s * 0.3], [s * 0.5, s * 0.6], STROKE, LineCap::Round, color)
        .line([s * 0.5, s * 0.6], [s * 0.5, s * 0.75], STROKE, LineCap::Round, color)
        .line([s * 0.5, s * 0.92], [s * 0.5, s * 0.94], 2.0, LineCap::Round, color);
    canvas(shape).size(size, size).a11y_hidden()
}

/// `text` in the tag style: a 12 pt uppercase-ish label (the app's small-caps face renders it
/// as capitals), used by the tier and tag badges.
pub fn badge_text(label: &str, color: Color) -> El {
    text(label).font(theme::leading_none(type_::XS)).color(color)
}
