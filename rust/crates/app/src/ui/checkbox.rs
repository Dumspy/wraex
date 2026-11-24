//! `web/ui/UiCheckbox.vue`: a check square and its label.

use rune::{El, row, text};

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
    checkbox_with(key, label, checked, disabled, None, on_press)
}

/// [`checkbox`] with an element in the same slot as the label, as `settings-price-check`'s
/// prediction box carries the `www.poeprices.info` badge right after its text.
pub fn checkbox_with<S: 'static>(
    key: &str,
    label: &str,
    checked: bool,
    disabled: bool,
    trailing: Option<El>,
    on_press: impl Fn(&mut S) + 'static,
) -> El {
    // `fas fa-check-square` is a filled square with the check as a hole, so a checked box is the
    // page's `text-gray-100` with a dark check; `far fa-square` is a `gray-500` outline.
    let ink = theme::gray(500);
    let paper = theme::gray(100);
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
    let mut text_ = text(label.to_owned())
        .key(format!("{key}-label"))
        .font(type_::SM)
        .color(if disabled { theme::gray(500) } else { theme::gray(100) })
        .min_w(0.0)
        .wrap();
    if trailing.is_none() {
        // An empty slot takes the row's width, so the label's own truncation is at its end.
        text_ = text_.grow(1.0);
    }
    button = button.child(icon::checkbox(14.0, checked, ink, paper)).child(text_);
    match trailing {
        Some(extra) => button.child(extra),
        None => button,
    }
}
