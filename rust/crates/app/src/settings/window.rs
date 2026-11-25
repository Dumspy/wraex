//! `SettingsWindow.vue`: the window's frame — title bar, menu, the selected page and the
//! Save/Cancel footer.

use rune::{Color, Cx, El, KeyEvent, col, el, row, text};

use crate::settings::hotkey;
use crate::settings::{Page, Project, Settings, t};
use crate::theme::{self, text as type_};
use crate::ui;

/// The settings window: `#settings-window`, its menu and the page under it.
pub fn view<S: 'static>(settings: &Settings, cx: &mut Cx<S>, project: Project<S>) -> El {
    if !settings.open {
        return el().key("settings");
    }
    // `.text-gray-100` inside the content column.
    cx.foreground(theme::gray(100));

    let title_bar =
        ui::title_bar(t(settings.game, "settings.title")).on_close(move |s: &mut S| project(s).cancel()).build();

    let window = col()
        .key("settings-window")
        .w_full()
        .max_w(crate::settings::WIDTH)
        .h(theme::rem(38.0))
        .fill(theme::gray(800))
        .min_h(0.0)
        .child(title_bar)
        .child(
            row()
                .key("settings-body")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .child(menu(settings, project))
                .child(content(settings, project)),
        );

    // `SettingsWindow.vue` is the widget: its root only holds the window (upstream has the
    // patrons' blocks above it, which the port does not draw), so it has no background of its
    // own and must not paint over the game — the overlay draws it as a card covering the game.
    col()
        .key("settings")
        .w_full()
        .h_full()
        .items_center()
        .child(window)
        // The recorder: a release anywhere while a field is armed writes the hotkey
        // (`HotkeyInput.vue` listens on its own input; the root hears every release, which also
        // covers Tab).
        .on_key_up(move |s: &mut S, event: &KeyEvent, tx: &mut rune::Tx| {
            hotkey::record(project(s), event, tx.physical_key())
        })
}

/// The menu column: `pl-2 pt-2 bg-gray-900`, one button per page. Upstream's column has
/// `min-width: 10rem` but grows to its widest child (the support block's "Support
/// development on", 11.25 rem); Wraex draws no support block, so the width is fixed there to
/// keep the content column where the reference puts it.
fn menu<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let mut column = col()
        .key("settings-menu")
        .w(theme::rem(11.25))
        .fill(theme::gray(900))
        .pl(theme::rem(0.5))
        .pt(theme::rem(0.5))
        .gap(theme::rem(0.25));

    for (group, pages) in Page::MENU.iter().enumerate() {
        for page in pages.iter().copied() {
            let active = settings.page == page;
            let label = t(settings.game, page.title_key());
            column = column.child(
                row()
                    .key(format!("settings-menu-{}", page.slug()))
                    .w_full()
                    .padding(theme::rem(0.5))
                    .corners([theme::ROUNDED, 0.0, 0.0, theme::ROUNDED])
                    .fill(if active { theme::gray(800) } else { Color::TRANSPARENT })
                    .hover(theme::gray(800))
                    .items_center()
                    .focusable(true)
                    .role("button")
                    .a11y_value(if active { "selected" } else { "not selected" })
                    .label(label.clone())
                    .on_press(move |s: &mut S| project(s).open_page(page))
                    .child(
                        text(label)
                            .key(format!("settings-menu-{}-label", page.slug()))
                            .font(theme::leading_none(type_::SM))
                            .color(if active { theme::gray(400) } else { theme::gray(600) })
                            .truncate(),
                    ),
            );
        }
        if group + 1 < Page::MENU.len() {
            // `<div class="border-b mx-2 border-gray-800" />`
            column = column.child(
                row().key(format!("settings-menu-separator-{group}")).h(1.0).mx(theme::rem(0.5)).fill(theme::gray(800)),
            );
        }
    }

    // `App.quit`: the tray's Quit action.
    column = column.child(
        row()
            .key("settings-quit")
            .mt(theme::rem(0.5))
            .mr(theme::rem(0.5))
            .px(theme::rem(0.25))
            .py(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .border(1.0, theme::gray(800))
            .border_layout()
            .focusable(true)
            .role("button")
            .label(t(settings.game, "app.quit"))
            .on_press_tx(|_s: &mut S, tx: &mut rune::Tx| tx.quit())
            .child(
                text(t(settings.game, "app.quit")).key("settings-quit-label").font(type_::SM).color(theme::gray(600)),
            ),
    );

    // The patrons strip: upstream scrolls the backers' names in a bar under the window.
    // Wraex has no patrons data in the crate yet (`patrons.json` is deferred), so only the
    // thank-you box is drawn, at the bottom of the menu column where the reference puts its
    // (equally static) "Support development on" block.
    column = column.child(el().grow(1.0));
    column.child(
        row().key("settings-patrons").w_full().padding(theme::rem(0.25)).child(
            row()
                .key("settings-thank-you")
                .w_full()
                .fill(theme::gray(800))
                .radius(theme::ROUNDED)
                .padding(theme::rem(0.25))
                .center()
                .child(
                    text(t(settings.game, "settings.thank_you"))
                        .key("settings-thank-you-label")
                        .w_full()
                        .wrap()
                        .min_w(0.0)
                        .align(rune::Align::Center)
                        .font(type_::SM)
                        .color(theme::gray(400)),
                ),
        ),
    )
}

/// The page's column and the Save/Cancel footer under it.
fn content<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    col()
        .key("settings-content")
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .fill(theme::gray(900))
        .child(
            col()
                .key("settings-page-scroll")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .fill(theme::gray(800))
                .corners([theme::ROUNDED, 0.0, 0.0, theme::ROUNDED])
                .scroll(rune::ScrollSpec { stick_bottom: false })
                .child(crate::settings::pages::view(settings, project)),
        )
        .child(footer(settings, project))
}

/// `border-t bg-gray-900 border-gray-600 p-2 flex justify-end gap-x-2`: Save and Cancel.
fn footer<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let mut bar = row()
        .key("settings-footer")
        .w_full()
        .padding(theme::rem(0.5))
        .items_center()
        .justify_end()
        .gap(theme::rem(0.5))
        .border_sides([1.0, 0.0, 0.0, 0.0], theme::gray(600))
        .border_layout();
    if let Some(error) = &settings.save_error {
        bar = bar.child(
            text(error.clone()).key("settings-save-error").font(type_::SM).color(theme::red(500)).grow(1.0).truncate(),
        );
    } else {
        bar = bar.child(el().grow(1.0));
    }
    bar.child(
        row()
            .key("settings-save")
            .px(theme::rem(0.75))
            .h(type_::SM.line_px())
            .fill(theme::gray(800))
            .radius(theme::ROUNDED)
            .center_y()
            .focusable(true)
            .role("button")
            .label(t(settings.game, "Save"))
            .on_press(move |s: &mut S| {
                if let Err(error) = project(s).save() {
                    project(s).save_error = Some(format!("Save failed: {error}"));
                }
            })
            .child(text(t(settings.game, "Save")).key("settings-save-label").font(type_::SM)),
    )
    .child(
        row()
            .key("settings-cancel")
            .px(theme::rem(0.75))
            .h(type_::SM.line_px())
            .center_y()
            .items_center()
            .focusable(true)
            .role("button")
            .label(t(settings.game, "Cancel"))
            .on_press(move |s: &mut S| project(s).cancel())
            .child(text(t(settings.game, "Cancel")).key("settings-cancel-label").font(type_::SM)),
    )
}
