//! `web/settings/hotkeys.vue` and `HotkeysGeneric.vue`: the hotkey list and the stash-scroll
//! choice.

use rune::{El, col, row, text};

use crate::settings::hotkey::{self, Recording, hotkey_input, modifier_button};
use crate::settings::pages::{page, tr};
use crate::settings::{Field, Project, Settings};
use crate::theme::{self, text as type_};
use crate::ui;

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let mut page = page("page-hotkeys");

    // `<div class="mb-2 bg-gray-700 rounded px-2 py-1 leading-none"><i class="fas fa-info-circle"> …`
    page = page.child(
        row()
            .key("hotkeys-info")
            .w_full()
            .mb(theme::rem(0.5))
            .px(theme::rem(0.5))
            .py(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .fill(theme::gray(700))
            .gap(theme::rem(0.25))
            .items_center()
            // `fas fa-info-circle`, inside the `text-wrap` hint row.
            .child(ui::icon::info_circle(type_::SM.size, theme::gray(100)).key("hotkeys-info-icon"))
            .child(
                text(tr(settings, "settings.clear_hotkey"))
                    .key("hotkeys-info-label")
                    .font(theme::leading_none(type_::SM))
                    .grow(1.0)
                    .min_w(0.0)
                    .wrap(),
            ),
    );

    let mut list = col().key("hotkeys-list").w_full().gap(theme::rem(1.0)).mb(theme::rem(2.0));

    // `price_check.name`'s group: the auto-hide key (Ctrl/Alt + key) and the locked one.
    let mut group = col().key("hotkeys-price-check").gap(theme::rem(0.25));
    group = group.child(text(tr(settings, "price_check.name")).key("hotkeys-price-check-label").font(type_::SM));
    let mut items = col().key("hotkeys-price-check-items").pl(theme::rem(1.0)).gap(theme::rem(0.5));
    items = items.child(split_row(
        settings,
        "hotkeys-price-check",
        "price_check.hotkey",
        Field::Widget("price-check", "hotkeyHold"),
        Field::Widget("price-check", "hotkey"),
        false,
        project,
    ));
    items = items.child(single_row(
        settings,
        "hotkeys-price-check-locked",
        &tr(settings, "price_check.hotkey_locked"),
        Field::Widget("price-check", "hotkeyLocked"),
        false,
        project,
    ));
    group = group.child(items);
    list = list.child(group);

    // Upstream lists the item check's one hotkey twice, as `map_check.name` and `item.info`
    // (APT and EE2 alike): two rows that always change together. The port shows it once, under
    // both names (decided with the user after the first in-game test).
    let item_check = format!("{} / {}", tr(settings, "map_check.name"), tr(settings, "item.info"));
    for (key, label, field, required) in [
        ("hotkeys-overlay", tr(settings, "settings.overlay"), Field::Root("overlayKey"), true),
        ("hotkeys-item-check", item_check, Field::Widget("item-check", "hotkey"), false),
        ("hotkeys-delve-grid", tr(settings, "settings.delve_grid"), Field::Widget("delve-grid", "toggleKey"), false),
    ] {
        list = list.child(single_row(settings, key, &label, field, required, project));
    }
    page = page.child(list);

    // Stash tab scrolling: Ctrl + MouseWheel, or Disabled.
    let stash_scroll = settings.bool(Field::Root("stashScroll"));
    page = page.child(
        row()
            .key("hotkeys-stash-scroll")
            .w_full()
            .mb(theme::rem(2.0))
            .items_center()
            .child(
                text(tr(settings, "settings.stash_scroll"))
                    .key("hotkeys-stash-scroll-label")
                    .font(type_::SM)
                    .grow(1.0)
                    .truncate(),
            )
            .child(
                row()
                    .gap(theme::rem(1.0))
                    .child(ui::radio(
                        "hotkeys-stash-scroll-wheel",
                        "Ctrl + MouseWheel",
                        stash_scroll,
                        false,
                        move |s: &mut S| project(s).set_bool(Field::Root("stashScroll"), true),
                    ))
                    .child(ui::radio(
                        "hotkeys-stash-scroll-disabled",
                        &tr(settings, "Disabled"),
                        !stash_scroll,
                        false,
                        move |s: &mut S| project(s).set_bool(Field::Root("stashScroll"), false),
                    )),
            ),
    );

    page
}

/// `SettingsHotkey.vue`'s `'value' in schema.config` branch: a label and one `HotkeyInput`.
pub(super) fn single_row<S: 'static>(
    settings: &Settings,
    key: &str,
    label: &str,
    field: Field,
    required: bool,
    project: Project<S>,
) -> El {
    let value = settings.string(field);
    let value = (!value.is_empty()).then_some(value);
    let mut label_line = row()
        .grow(1.0)
        .min_w(0.0)
        .items_center()
        .gap(theme::rem(0.25))
        .child(text(label.to_owned()).key(format!("{key}-label")).font(type_::SM).truncate());
    if required {
        // `<span v-if="schema.required" :class="$style.required">*</span>`
        label_line = label_line.child(text("*").key(format!("{key}-required")).font(type_::LG).color(theme::red(500)));
    }
    row().key(key.to_owned()).w_full().items_center().child(label_line).child(hotkey_input(
        &format!("{key}-input"),
        value.as_deref(),
        hotkey::WIDE,
        &hotkey::no_key(settings.game),
        is_recording(settings, field),
        move |s: &mut S| start(project(s), Recording { field, required, no_mod_keys: false }),
    ))
}

/// `SettingsHotkey.vue`'s split branch: `Ctrl`/`Alt` buttons, `+`, then a `noModKeys` input.
pub(super) fn split_row<S: 'static>(
    settings: &Settings,
    key: &str,
    label_key: &str,
    mod_field: Field,
    key_field: Field,
    required: bool,
    project: Project<S>,
) -> El {
    let mod_key = settings.string(mod_field);
    let mod_key = (!mod_key.is_empty()).then_some(mod_key);
    let non_mod = settings.string(key_field);
    let non_mod = (!non_mod.is_empty()).then_some(non_mod);
    let color = if mod_key.is_some() { theme::gray(100) } else { theme::red(400) };

    let mut label_line = row()
        .grow(1.0)
        .min_w(0.0)
        .items_center()
        .gap(theme::rem(0.25))
        .child(text(tr(settings, label_key)).key(format!("{key}-label")).font(type_::SM).truncate());
    if required {
        label_line = label_line.child(text("*").key(format!("{key}-required")).font(type_::LG).color(theme::red(500)));
    }

    let split = row()
        .key(format!("{key}-split"))
        .w(hotkey::WIDE)
        .gap(theme::rem(0.25))
        .items_center()
        .color(color)
        .child(modifier_button(&format!("{key}-ctrl"), "Ctrl", mod_key.as_deref() == Some("Ctrl"), move |s: &mut S| {
            project(s).set_string(mod_field, "Ctrl");
        }))
        .child(modifier_button(&format!("{key}-alt"), "Alt", mod_key.as_deref() == Some("Alt"), move |s: &mut S| {
            project(s).set_string(mod_field, "Alt");
        }))
        .child(text("+").key(format!("{key}-plus")).font(type_::SM).grow(1.0).align(rune::Align::Center))
        .child(hotkey_input(
            &format!("{key}-input"),
            non_mod.as_deref(),
            hotkey::NARROW,
            &hotkey::no_key(settings.game),
            is_recording(settings, key_field),
            move |s: &mut S| start(project(s), Recording { field: key_field, required, no_mod_keys: true }),
        ));

    row().key(key.to_owned()).w_full().items_center().child(label_line).child(split)
}

/// Whether `field` is the hotkey the window is capturing.
pub(super) fn is_recording(settings: &Settings, field: Field) -> bool {
    settings.recording.is_some_and(|recording| recording.field == field)
}

/// A press on a hotkey field: `HotkeyInput.vue`'s focused input, which the window's root hears
/// releases for (`hotkey::record`).
fn start(settings: &mut Settings, recording: Recording) {
    settings.recording = Some(recording);
    settings.select_open = None;
}
