//! `web/ui/UiToggle.vue`: the on/off switch `FiltersBlock.vue` uses for Hidden and Mods.

use rune::{El, row, text};

use crate::theme::{self, text as type_};
use crate::ui::icon;

/// `.toggle` with its slotted label: a 22 pt row, a switch and the text, `text-gray-400` at
/// the call site as upstream puts it.
pub fn toggle<S: 'static>(key: &str, label: &str, on: bool, on_press: impl Fn(&mut S) + 'static) -> El {
    row()
        .key(key)
        .h(theme::rem(1.375))
        .items_center()
        .focusable(true)
        .role("button")
        .label(label)
        .on_press(on_press)
        .child(row().pr(theme::rem(0.25)).child(icon::toggle(on, if on { theme::gray(300) } else { theme::gray(600) })))
        .child(text(label).key(format!("{key}-label")).font(type_::SM).color(theme::gray(400)))
}
