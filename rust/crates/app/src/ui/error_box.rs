//! `UiErrorBox.vue`: what a widget shows when something failed.

use rune::{El, col, row, text};

use crate::theme::{self, text as type_};

/// `border border-red-400 rounded px-4 py-2 flex flex-col gap-y-1`, with the red name heading.
///
/// The box sizes to its content; add actions with [`actions`] as a child.
pub fn error_box(name: &str, message: &str) -> El {
    // No `.w_full()`: the box stretches to the column it's in, so a caller's margins shrink it
    // as `margin` does in CSS rather than pushing a 100%-wide box past the edge.
    col()
        .key("error-box")
        .border(1.0, theme::red(400))
        // A CSS border takes room in the box (Tailwind's `box-sizing: border-box`).
        .border_layout()
        .radius(theme::ROUNDED)
        .px(theme::rem(1.0))
        .py(theme::rem(0.5))
        .gap(theme::rem(0.25))
        .child(text(name).key("error-name").font(type_::SM).color(theme::red(400)))
        .child(text(message).key("error-message").font(type_::SM).color(theme::gray(200)))
}

/// `.actions`: `flex gap-x-1 justify-end`, for buttons inside an [`error_box`].
pub fn actions(buttons: impl IntoIterator<Item = El>) -> El {
    let mut row = row().key("error-actions").w_full().justify_end().gap(theme::rem(0.25));
    for button in buttons {
        row = row.child(button);
    }
    row
}
