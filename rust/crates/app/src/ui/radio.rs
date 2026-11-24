//! `web/ui/UiRadio.vue`: a circle or a check circle and its label.

use rune::{El, row, text};

use crate::theme::{self, text as type_};
use crate::ui::icon;

/// `UiRadio.vue`: `far fa-circle` / `fas fa-check-circle`. The root is keyed `key` and its label
/// `{key}-label`. `disabled` greys the icon and the text (`:disabled`), as upstream's
/// `forcedIntlSite` radio does.
pub fn radio<S: 'static>(
    key: &str,
    label: &str,
    checked: bool,
    disabled: bool,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    let color = if disabled { theme::gray(600) } else { theme::gray(400) };
    let mut button = row()
        .key(key)
        .gap(theme::rem(0.25))
        .items_center()
        .focusable(!disabled)
        .role("radio")
        .a11y_value(if checked { "checked" } else { "unchecked" })
        .label(label.to_owned());
    if !disabled {
        button = button.on_press(on_press);
    }
    button
        .child(icon::radio(theme::rem(0.875), checked, color))
        .child(text(label.to_owned()).key(format!("{key}-label")).font(type_::SM).color(color))
}
