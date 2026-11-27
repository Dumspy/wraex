//! `web/settings/chat.vue`: the chat commands, their Enter toggle and their hotkeys.

use rune::{El, col, row, text};
use wraex_core::js::json_value;
use wraex_core::widget::Command;

use crate::settings::hotkey::{self, HotkeyInput, Recording, hotkey_input};
use crate::settings::pages::{TextField, page, text_field, tr};
use crate::settings::{Field, Project, Settings, commands_mut};
use crate::theme::{self, text as type_};
use crate::ui;
use crate::ui::field::Kind;

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let mut page = page("page-chat");
    let mut list = col().key("chat-commands").w_full().gap(theme::rem(1.0)).mb(theme::rem(1.0));

    for index in 0..settings.commands().len() {
        let command = settings.command(index);
        let send = command.send();
        let hotkey = command.hotkey().map(str::to_owned);
        let mut entry = col().key(format!("chat-command-{index}")).gap(theme::rem(0.25));

        // `input v-model.trim="command.text" class="… w-full font-poe"`
        entry = entry.child(text_field(
            settings,
            TextField::new(&format!("chat-command-{index}"), Field::Command(index, "text"), Kind::Text),
            project,
        ));

        let actions = row()
            .key(format!("chat-command-{index}-actions"))
            .w_full()
            .items_center()
            .flex_wrap()
            .gap(theme::rem(0.5))
            .child(ui::toggle(
                &format!("chat-command-{index}-send"),
                &tr(settings, "settings.chat_cmd_send"),
                send,
                move |s: &mut S| {
                    let settings = project(s);
                    settings.set_bool(Field::Command(index, "send"), !settings.bool(Field::Command(index, "send")));
                },
            ))
            .child(row().key(format!("chat-command-{index}-remove-row")).grow(1.0))
            .child(
                row()
                    .key(format!("chat-command-{index}-remove"))
                    .items_center()
                    .focusable(true)
                    .role("button")
                    .label(tr(settings, "Remove"))
                    .on_press(move |s: &mut S| {
                        let settings = project(s);
                        if let Some(commands) = commands_mut(&mut settings.config)
                            && index < commands.len()
                        {
                            commands.remove(index);
                        }
                        settings.seed_edits();
                    })
                    .child(
                        text(tr(settings, "Remove"))
                            .key(format!("chat-command-{index}-remove-label"))
                            .font(type_::SM)
                            .color(theme::gray(500)),
                    ),
            )
            .child(hotkey_input(
                HotkeyInput {
                    key: &format!("chat-command-{index}-hotkey"),
                    value: hotkey.as_deref(),
                    width: hotkey::WIDE,
                    placeholder: &hotkey::no_key(settings.game),
                    recording: super::hotkeys::is_recording(settings, Field::Command(index, "hotkey")),
                },
                move |s: &mut S| {
                    let settings = project(s);
                    settings.recording =
                        Some(Recording { field: Field::Command(index, "hotkey"), required: false, no_mod_keys: false });
                    settings.select_open = None;
                },
            ));
        entry = entry.child(actions);
        list = list.child(entry);
    }
    page = page.child(list);

    // `<button class="bg-gray-900 rounded flex items-baseline px-2 py-1 leading-none"><i class="fas fa-plus">`
    page = page.child(
        row()
            .key("chat-add")
            .self_start()
            .px(theme::rem(0.5))
            .py(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .fill(theme::gray(900))
            .items_center()
            .gap(theme::rem(0.25))
            .focusable(true)
            .role("button")
            .label(tr(settings, "settings.chat_cmd_add"))
            .on_press(move |s: &mut S| {
                let settings = project(s);
                if let Some(commands) = commands_mut(&mut settings.config) {
                    commands.push(json_value(&Command { text: String::new(), hotkey: None, send: true }));
                }
                settings.seed_edits();
            })
            // `fas fa-plus mr-1`: the row's `gap` is upstream's `mr-1`.
            .child(ui::icon::plus(type_::SM.size, theme::gray(400)).key("chat-add-icon"))
            .child(
                text(tr(settings, "settings.chat_cmd_add"))
                    .key("chat-add-label")
                    .font(theme::leading_none(type_::SM))
                    .color(theme::gray(400)),
            ),
    );

    page
}
