//! The `<select>` controls of the settings pages: the closed box, and the list it opens.
//!
//! Upstream uses a native `<select>`, whose popup the system draws. Rune's native menus
//! (`Tx::popup_menu`) are unavailable headless, so this draws its own list anchored under the
//! box: a white tippy-style frame around a `bg-gray-800` column, as the price check's popovers
//! are drawn.

use std::rc::Rc;

use rune::{Align, Color, El, Side, row, text};

use crate::theme::{self, text as type_};
use crate::ui::icon;

/// One option: its value in the config, and the text shown for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

impl Choice {
    pub fn new(value: impl Into<String>, label: impl Into<String>) -> Self {
        Self { value: value.into(), label: label.into() }
    }
}

/// One [`select`]'s content: its key, the options, the picked value and whether it is open.
#[derive(Debug)]
pub struct SelectProps<'a> {
    /// The select's element key; the box is `key`, the list `{key}-list`.
    pub key: &'a str,
    /// The options, in order.
    pub choices: &'a [Choice],
    /// The picked option's value (or a value with no option, shown as is).
    pub value: &'a str,
    /// Whether the list is drawn under the box.
    pub open: bool,
    /// The closed box's width (`w-24`).
    pub width: f32,
}

/// The closed select: `p-1 rounded bg-gray-700 w-24`, with the selected option's label. `toggle`
/// opens and closes it, `pick` takes an option's value.
pub fn select<S: 'static>(
    props: &SelectProps<'_>,
    toggle: impl Fn(&mut S) + 'static,
    pick: impl Fn(&mut S, String) + 'static,
) -> El {
    let SelectProps { key, choices, value, open, width } = *props;
    let pick = Rc::new(pick);
    let current = choices
        .iter()
        .find(|choice| choice.value == value)
        .map_or_else(|| value.to_owned(), |choice| choice.label.clone());
    let mut root = row()
        .key(key)
        .w(width)
        .padding(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(700))
        .center_y()
        .gap(theme::rem(0.25))
        .focusable(true)
        .role("combobox")
        .label(current.clone())
        .on_press(toggle)
        .child(text(current).key(format!("{key}-label")).font(type_::SM).color(theme::gray(100)).grow(1.0).truncate())
        .child(icon::chevron(theme::rem(0.75), false, theme::gray(400)));
    if !open {
        return root;
    }
    // The list, anchored under the box, as `select`'s native popup would open.
    let mut list = rune::col().key(format!("{key}-list")).py(theme::rem(0.25)).min_w(width);
    for choice in choices {
        let selected = choice.value == value;
        let chosen = choice.value.clone();
        let label = choice.label.clone();
        let pick = Rc::clone(&pick);
        list = list.child(
            row()
                .key(format!("{key}-option-{}", choice.value))
                .px(theme::rem(0.5))
                .py(theme::rem(0.125))
                .gap(theme::rem(0.25))
                .items_center()
                .radius(theme::ROUNDED)
                .hover(theme::gray(700))
                .focusable(true)
                .role("option")
                .a11y_value(if selected { "selected" } else { "not selected" })
                .label(label.clone())
                .on_press(move |s: &mut S| pick(s, chosen.clone()))
                .child(icon::radio(theme::rem(0.75), selected, theme::gray(400)))
                .child(text(label).key(format!("{key}-option-{}-label", choice.value)).font(type_::SM)),
        );
    }
    root = root.child(
        row()
            .key(format!("{key}-popup"))
            .padding(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .fill(Color::WHITE)
            .child(list.fill(theme::gray(800)).color(theme::gray(300)))
            .anchored(key, Side::Bottom, Align::Start, 2.0),
    );
    root
}
