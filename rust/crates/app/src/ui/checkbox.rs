//! `web/ui/UiCheckbox.vue`: a check square and its label.

use rune::{Color, El, row, text};

use crate::theme::{self, text as type_};
use crate::ui::icon;

/// `UiCheckbox.vue`: `far fa-square` / `fas fa-check-square`, the slot text beside it. The root
/// is keyed `key` and its label `{key}-label`; `disabled` greys the text, as
/// `settings.general`'s forced international-site box is drawn.
pub fn checkbox<S: 'static>(
    key: &str,
    label: &str,
    checked: bool,
    disabled: bool,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    // The check is cut out of the white box, so a checked box draws a dark check.
    let ink = if checked { theme::gray(800) } else { theme::gray(500) };
    let mut button = row()
        .key(key)
        .w_full()
        .gap(theme::rem(0.25))
        .items_center()
        .focusable(!disabled)
        .role("checkbox")
        .a11y_value(if checked { "checked" } else { "unchecked" })
        .label(label.to_owned());
    if !disabled {
        button = button.on_press(on_press);
    }
    button.child(icon::checkbox(14.0, checked, ink, Color::WHITE)).child(
        text(label.to_owned())
            .key(format!("{key}-label"))
            .font(type_::SM)
            .color(if disabled { theme::gray(500) } else { theme::gray(100) })
            .grow(1.0)
            .min_w(0.0)
            .truncate(),
    )
}
