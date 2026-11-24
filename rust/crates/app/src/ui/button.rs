//! The `.btn` class (`web/App.vue`): the plain grey button the widgets use.

use rune::{El, row, text};

use crate::theme::{self, text as type_};

/// `bg-gray-700 px-2 py-1 text-gray-400 leading-none rounded`.
///
/// The root is keyed `key` and its label `{key}-label`. Add `.on_press(..)` (and
/// `.focusable(true)`) at the call site: the label, not the button, names the action.
pub fn button(key: &str, label: &str) -> El {
    row()
        .key(key)
        .px(theme::rem(0.5))
        .py(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(700))
        .center_y()
        .role("button")
        .label(label)
        .child(text(label).key(format!("{key}-label")).font(theme::leading_none(type_::SM)).color(theme::gray(400)))
}
