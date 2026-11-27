//! `web/ui/UiCheckbox.vue`: a check square and its label.

use std::fmt;

use rune::{El, row, text};

use crate::theme::{self, text as type_};
use crate::ui::icon;

/// One [`checkbox`]'s content: its keys, label, state and optional trailing element.
///
/// `disabled` greys the text and takes no press handler, as `settings.general`'s forced
/// international-site box is drawn. `trailing` is an element in the same slot as the label, as
/// `settings-price-check`'s prediction box carries the `www.poeprices.info` badge right after its
/// text.
#[derive(Default)]
pub struct CheckboxProps {
    key: String,
    label: String,
    checked: bool,
    disabled: bool,
    trailing: Option<El>,
}

impl fmt::Debug for CheckboxProps {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CheckboxProps")
            .field("key", &self.key)
            .field("label", &self.label)
            .field("checked", &self.checked)
            .field("disabled", &self.disabled)
            .field("trailing", &self.trailing.is_some())
            .finish()
    }
}

impl CheckboxProps {
    /// A checkbox keyed `key`, labelled `label`, unchecked and enabled.
    pub fn new(key: &str, label: &str) -> Self {
        Self { key: key.to_owned(), label: label.to_owned(), ..Self::default() }
    }

    /// `checked`: a filled `fas fa-check-square` instead of the `far fa-square` outline.
    #[must_use]
    pub const fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }

    /// `:disabled`: greys the label and takes no press handler.
    #[must_use]
    pub const fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// An element in the label's slot, after the text.
    #[must_use]
    pub fn trailing(mut self, el: El) -> Self {
        self.trailing = Some(el);
        self
    }
}

/// `UiCheckbox.vue`: `far fa-square` / `fas fa-check-square`, the slot text beside it.
///
/// The root is keyed `props.key` and its label `{key}-label`.
pub fn checkbox<S: 'static>(props: CheckboxProps, on_press: impl Fn(&mut S) + 'static) -> El {
    let CheckboxProps { key, label, checked, disabled, trailing } = props;
    // `fas fa-check-square` is a filled square with the check as a hole, so a checked box is the
    // page's `text-gray-100` with a dark check; `far fa-square` is a `gray-500` outline.
    let ink = theme::gray(500);
    let paper = theme::gray(100);
    let mut button = row()
        .key(key.clone())
        .w_full()
        .gap(theme::rem(0.25))
        .items_center()
        .focusable(!disabled)
        .role("checkbox")
        .a11y_value(if checked { "checked" } else { "unchecked" })
        .label(label.clone());
    if !disabled {
        button = button.on_press(on_press);
    }
    let mut text_ = text(label)
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
