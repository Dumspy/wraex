//! `ref/apt/renderer/src/web/overlay/WidgetMenu.vue` in Rune: the overlay's launcher bar.
//!
//! One button per hidden widget (`wm.show` on press), the settings window's cog among them, the
//! `fa-ellipsis-h` "add widget…" popover for the `multi` widget types, and the price-check box
//! under the bar, which hands the pasted item text to the price check
//! (`MAIN->CLIENT::item-text`). Editing the widget (`Widget.vue`'s `edit`, from the hover panel)
//! swaps the box for the `Show button for active widgets` toggle.
//!
//! The popover is tippy.js upstream (a DOM node appended to `<body>`); the port draws it under
//! the add button instead, right-aligned to it, with tippy's light theme — the same entries and
//! labels, a different box. The widget list the popover offers is the reference's registry
//! (`widget-registry.ts`) filtered to `instances: 'multi'`, written out here because the port
//! has no component registry.

use rune::{Color, El, col, row, text};
use serde::Deserialize;

use super::lenient;
use crate::theme::{self, text as type_};
use crate::ui;
use crate::widgets::{Overlay, Project, Wants, Widget, icon};

/// `WidgetSpec.type`.
pub const WM_TYPE: &str = "menu";

/// `config.alwaysShow` (`WidgetMenu.vue`'s edit mode).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AlwaysShowField {
    #[serde(default, deserialize_with = "lenient")]
    always_show: Option<bool>,
}

/// `config.alwaysShow`: the edit mode's `Show button for active widgets`.
fn always_show(widget: &Widget) -> bool {
    AlwaysShowField::deserialize(&widget.config).unwrap_or_default().always_show.unwrap_or(false)
}

/// `WidgetMenu.vue`: the bar, the add popover and the paste box.
pub fn view<S: 'static>(overlay: &Overlay, widget: &Widget, project: Project<S>) -> El {
    let id = widget.wm_id;
    let editing = overlay.editing == Some(id);
    let always_show = always_show(widget);

    let mut bar = row().key("widget-menu-bar").padding(theme::rem(0.25)).gap(theme::rem(0.25)).items_center();
    for entry in &overlay.widgets {
        if entry.has_flag("menu::skip") || (!always_show && entry.wm_wants != Wants::Hide) {
            continue;
        }
        bar = bar.child(button(entry, project));
    }
    bar = bar.child(add_button(overlay, project));
    if overlay.add_open {
        bar = bar.child(row().key("widget-menu-add-anchor").h_full().child(add_popover(overlay, project)));
    }

    let mut card = col()
        .key("widget-menu")
        .fill(theme::gray(900))
        .radius(theme::ROUNDED)
        // `.widget-default-style`'s `box-shadow: 0 1px 3px rgba(0, 0, 0, 0.75)`.
        .shadow(1.0, 3.0, Color::BLACK.alpha(0.75))
        .child(bar);
    card = card.child(if editing {
        row().key("widget-menu-always-show").px(theme::rem(0.5)).pb(theme::rem(0.25)).child(ui::toggle(
            "widget-menu-always-show-toggle",
            &crate::settings::t(overlay.game, "widget_menu.always_show"),
            always_show,
            move |s: &mut S| {
                let overlay = project(s);
                overlay.set_always_show(id, !always_show);
            },
        ))
    } else {
        paste_box(overlay, project)
    });
    card
}

/// One widget's button: `bg-gray-800 rounded text-gray-100 p-2 leading-none border`, its icon
/// and title, and a `gray-500` border while the widget is shown.
fn button<S: 'static>(entry: &Widget, project: Project<S>) -> El {
    let id = entry.wm_id;
    let (title, icon_name) = entry.title_and_icon();
    let border = if entry.wm_wants == Wants::Show { theme::gray(500) } else { theme::gray(800) };
    let mut button = row()
        .key(format!("widget-menu-entry-{id}"))
        .padding(theme::rem(0.5))
        .radius(theme::ROUNDED)
        .fill(theme::gray(800))
        .border(1.0, border)
        .border_layout()
        .items_center()
        .focusable(true)
        .role("button")
        .label(title.clone().or_else(|| icon_name.clone()).unwrap_or_else(|| format!("#{id}")))
        .on_press(move |s: &mut S| project(s).toggle(id));
    if let Some(name) = &icon_name {
        // `<i v-if="widget.icon" class="fas align-bottom" :class="widget.icon" />`
        let glyph = icon::named(name, type_::BASE.size, theme::gray(100))
            .unwrap_or_else(|| icon::fallback(name, type_::BASE.size, theme::gray(100)));
        button = button.child(glyph.key(format!("widget-menu-entry-{id}-icon")));
    }
    if let Some(title) = title {
        button = button.child(
            text(title)
                .key(format!("widget-menu-entry-{id}-title"))
                .font(theme::leading_none(type_::BASE))
                .color(theme::gray(100)),
        );
    }
    button
}

/// The `fa-ellipsis-h` target: `rounded text-gray-600 px-2 py-1 leading-none`.
fn add_button<S: 'static>(overlay: &Overlay, project: Project<S>) -> El {
    row()
        .key("widget-menu-add")
        .px(theme::rem(0.5))
        .py(theme::rem(0.25))
        // A 16 pt icon in a `leading-none` line box: the reference's button is 25 pt tall, one
        // more than padding and icon add up to.
        .h(25.0)
        .radius(theme::ROUNDED)
        .focusable(true)
        .role("button")
        .label(crate::settings::t(overlay.game, "widget_menu.add"))
        .on_press(move |s: &mut S| {
            let overlay = project(s);
            overlay.add_open = !overlay.add_open;
        })
        .child(icon::ellipsis_horizontal(14.0, type_::BASE.size, theme::gray(600)).key("widget-menu-add-icon"))
}

/// `ui/popover.vue`'s content: the `t(':add')` header and one button per `multi` widget type.
fn add_popover<S: 'static>(overlay: &Overlay, project: Project<S>) -> El {
    let mut panel = col()
        .key("widget-menu-add-panel")
        .padding(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(Color::WHITE)
        .shadow(1.0, 3.0, Color::BLACK.alpha(0.35))
        .pin(None, Some(26.0), Some(0.0), None)
        .child(
            text(crate::settings::t(overlay.game, "widget_menu.add"))
                .key("widget-menu-add-header")
                .font(type_::SM)
                .color(theme::gray(600))
                .px(theme::rem(0.25)),
        );
    for (wm_type, name_key) in instantiable(overlay.game) {
        let label = crate::settings::t(overlay.game, name_key);
        let wm_type = *wm_type;
        panel = panel.child(
            row()
                .key(format!("widget-menu-add-{wm_type}"))
                .padding(theme::rem(0.25))
                .radius(theme::ROUNDED)
                .focusable(true)
                .role("button")
                .label(label.clone())
                .hover(theme::gray(400))
                .on_press(move |s: &mut S| {
                    let overlay = project(s);
                    overlay.create(wm_type);
                    overlay.add_open = false;
                })
                .child(
                    text(label)
                        .key(format!("widget-menu-add-{wm_type}-label"))
                        .font(type_::BASE)
                        .color(theme::gray(900)),
                ),
        );
    }
    panel
}

/// The `<textarea>`: `px-2 py-1.5 bg-gray-700 rounded`, 172 × 32 (a browser textarea's own
/// default width at 14 pt), placeholder `t(':price_check')`, cleared as it dispatches.
fn paste_box<S: 'static>(overlay: &Overlay, project: Project<S>) -> El {
    let style = type_::SM;
    let placeholder = crate::settings::t(overlay.game, "widget_menu.price_check");
    row().key("widget-menu-paste").px(theme::rem(0.25)).pb(theme::rem(0.25)).child(
        rune::text_input(&overlay.paste, style)
            .key("widget-menu-paste-input")
            .w(172.0)
            .h(32.0)
            .px(theme::rem(0.5))
            .py(theme::rem(0.375))
            .radius(theme::ROUNDED)
            .fill(theme::gray(700))
            .color(theme::gray(100))
            .caret(theme::gray(300), theme::gray(400).alpha(0.35))
            .placeholder(placeholder.clone(), theme::gray(500))
            .placeholder_font(style)
            .label(placeholder)
            .on_edit(move |s: &mut S| &mut project(s).paste)
            .on_input_change(move |s: &mut S, _tx: &mut rune::Tx| {
                let overlay = project(s);
                let text = overlay.paste.text.clone();
                if !text.is_empty() {
                    overlay.actions.push(crate::widgets::Action::PriceCheck(text));
                    overlay.paste.replace_text("");
                }
            }),
    )
}

/// `registry.widgets.filter(({ widget }) => widget.instances === 'multi')`, in registry order:
/// `(wmType, trNameKey ?? type)`. EE2's registry has the notepad too.
const fn instantiable(game: wraex_core::GameId) -> &'static [(&'static str, &'static str)] {
    match game {
        wraex_core::GameId::Poe2 => &[
            ("timer", "stopwatch.name"),
            ("stash-search", "stash_search.name"),
            ("image-strip", "image_strip.name"),
            ("notepad", "notepad.name"),
        ],
        wraex_core::GameId::Poe1 => {
            &[("timer", "stopwatch.name"), ("stash-search", "stash_search.name"), ("image-strip", "image_strip.name")]
        }
    }
}
