//! `ref/apt/renderer/src/web/overlay/WidgetDelveGrid.vue` in Rune: the Delve grid overlay.
//!
//! An 11 × 9 grid of `border-gray-600` cells over a translucent white box, the size of the game's
//! map panel, so a route can be planned on the Delve map underneath. The reference is
//! `readonly` and `move-handles: none`: it has no cell state at all — nothing toggles a cell, and
//! the only thing that happens is its hotkey showing and hiding it (`widget-action` target
//! `delve-grid`, or `toggleKey`). The port keeps exactly that: [`view`] draws the grid, and
//! [`crate::widgets::Overlay::widget_action`] toggles the widget.
//!
//! The geometry is the reference's arithmetic on the overlay's height (`wm.size.height`), so a
//! scene renders it at the 900 pt of `tools/golden/screens`'s page: 858 × 673 at the 900 pt
//! overlay, 81 pt cells, and the grid centred with whatever overflow the cells leave, which the
//! card's `overflow: hidden` crops.

use rune::{Color, El, col, el, row};
use wraex_core::num::usize_to_f32;

use crate::theme;
use crate::widgets::{Overlay, Project, Widget};

/// `WidgetSpec.type`.
pub const WM_TYPE: &str = "delve-grid";

/// The rows and columns (`v-for="i in 9"`, `v-for="j in 11"`).
pub const ROWS: usize = 9;
pub const COLUMNS: usize = 11;

/// `WidgetDelveGrid.vue`'s `anchor` computed from the overlay's height, in points.
///
/// `height = round(h * 808 / 1080)`, `width = round(height * 1030 / 808)`,
/// `top = round(h * 67 / 1080)` and `cell = round(h * 97 / 1080)`; `top` is where the card sits
/// down the overlay, which only matters to the whole-overlay layout, not to a scene.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Metrics {
    pub width: f32,
    pub height: f32,
    pub top: f32,
    pub cell: f32,
}

impl Metrics {
    pub fn for_overlay(overlay_height: f32) -> Self {
        let height = (overlay_height * 808.0 / 1080.0).round();
        let width = (height * 1030.0 / 808.0).round();
        let top = (overlay_height * 67.0 / 1080.0).round();
        let cell = (overlay_height * 97.0 / 1080.0).round();
        Self { width, height, top, cell }
    }

    /// `top`/`left` of the inner grid, `(anchor.height - anchor.cell * 9) / 2` and
    /// `(anchor.width - anchor.cell * 11) / 2`: negative when the cells overflow the box, which
    /// is how the reference crops them.
    // `mul_add` would fuse the subtraction's rounding; the reference's CSS arithmetic rounds
    // each operation on its own.
    #[expect(clippy::suboptimal_flops, reason = "the reference's arithmetic is not fused")]
    pub fn offset(self) -> (f32, f32) {
        let columns = usize_to_f32(COLUMNS);
        let rows = usize_to_f32(ROWS);
        ((self.width - self.cell * columns) / 2.0, (self.height - self.cell * rows) / 2.0)
    }
}

/// The translucent box with its 99 cells.
pub fn view<S: 'static>(_overlay: &Overlay, _widget: &Widget, _project: Project<S>) -> El {
    let metrics = Metrics::for_overlay(crate::HEIGHT);
    let (left, top) = metrics.offset();
    let mut grid = col().key("delve-grid-cells").absolute(left, top);
    for row_index in 0..ROWS {
        let mut cells = row().key(format!("delve-grid-row-{row_index}"));
        for column in 0..COLUMNS {
            cells = cells.child(
                el().key(format!("delve-grid-cell-{row_index}-{column}"))
                    .size(metrics.cell, metrics.cell)
                    .border(1.0, theme::gray(600))
                    .border_layout(),
            );
        }
        grid = grid.child(cells);
    }
    // `overflow-hidden` on `background: rgba(255, 255, 255, 0.1)`.
    row()
        .key("delve-grid")
        .size(metrics.width, metrics.height)
        .fill(Color::WHITE.alpha(0.1))
        .clip_corners([0.0; 4])
        .child(grid)
}
