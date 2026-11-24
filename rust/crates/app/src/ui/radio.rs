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
        .child(marker(checked, color))
        .child(text(label.to_owned()).key(format!("{key}-label")).font(type_::SM).color(color))
}

/// `UiRadio.vue` with an empty slot: the maps page's decision radios, whose colour is on the
/// icon alone. Upstream's `p-1` pads the 0.875 rem glyph to a 1.375 rem square.
pub fn radio_icon<S: 'static>(key: &str, label: &str, checked: bool, on_press: impl Fn(&mut S) + 'static) -> El {
    row()
        .key(key)
        .padding(theme::rem(0.25))
        .focusable(true)
        .role("radio")
        .a11y_value(if checked { "checked" } else { "unchecked" })
        .label(label.to_owned())
        .on_press(on_press)
        .child(marker(checked, theme::gray(400)))
}

/// `far fa-circle` or `fas fa-check-circle`; the check borrows the page's own `bg-gray-800`.
fn marker(checked: bool, color: rune::Color) -> El {
    let size = theme::rem(0.875);
    if checked { icon::check_circle(size, color, theme::gray(800)) } else { icon::radio(size, false, color) }
}
