//! `web/ui/UiRadio.vue`: a circle or a check circle and its label.

use rune::{El, row, text};

use crate::theme::{self, text as type_};
use crate::ui::icon;

/// One [`radio`]'s content: its keys, label and state.
///
/// `disabled` greys the icon and the text (`:disabled`), as upstream's `forcedIntlSite` radio
/// does.
#[derive(Debug, Default)]
pub struct RadioProps {
    key: String,
    label: String,
    checked: bool,
    disabled: bool,
}

impl RadioProps {
    /// A radio keyed `key`, labelled `label`, unchecked and enabled.
    pub fn new(key: &str, label: &str) -> Self {
        Self { key: key.to_owned(), label: label.to_owned(), ..Self::default() }
    }

    /// `checked`: `fas fa-check-circle` instead of the `far fa-circle` outline.
    #[must_use]
    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// `:disabled`: greys the icon and the text, and takes no press handler.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

/// `UiRadio.vue`: `far fa-circle` / `fas fa-check-circle`.
///
/// The root is keyed `props.key` and its label `{key}-label`; otherwise the marker is the page's
/// own `text-gray-100` (`SettingsWindow.vue`), the icon having no colour class of its own.
pub fn radio<S: 'static>(props: RadioProps, on_press: impl Fn(&mut S) + 'static) -> El {
    let RadioProps { key, label, checked, disabled } = props;
    let color = if disabled { theme::gray(600) } else { theme::gray(100) };
    let mut button = row()
        .key(key.clone())
        .gap(theme::rem(0.25))
        .items_center()
        .focusable(!disabled)
        .role("radio")
        .a11y_value(if checked { "checked" } else { "unchecked" })
        .label(label.clone());
    if !disabled {
        button = button.on_press(on_press);
    }
    button.child(marker(checked, color)).child(text(label).key(format!("{key}-label")).font(type_::SM).color(color))
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
        .child(marker(checked, theme::gray(100)))
}

/// `far fa-circle` or `fas fa-check-circle`; the check is a hole in the disc, so the page's own
/// `bg-gray-800` shows through it.
fn marker(checked: bool, color: rune::Color) -> El {
    let size = theme::rem(0.875);
    if checked { icon::check_circle(size, color, theme::gray(800)) } else { icon::radio(size, false, color) }
}
