//! `web/item-check/settings-item-check.vue`: the item check's hotkeys, and EE2's "Find same
//! price" choice.

use rune::{El, col, row, text};
use wraex_core::GameId;

use crate::settings::hotkey::{self, Recording, hotkey_input};
use crate::settings::pages::hotkeys::single_row;
use crate::settings::pages::{is_english, page, tr};
use crate::settings::{Field, Project, Settings};
use crate::theme::{self, text as type_};
use crate::ui;

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let mut rows = col().key("item-check-hotkeys").w_full().gap(theme::rem(1.0));

    for (key, label_key, field) in [
        ("item-check-wiki", "item.open_on_wiki", Field::Widget("item-check", "wikiKey")),
        ("item-check-poedb", "item.open_on_poedb", Field::Widget("item-check", "poedbKey")),
        ("item-check-craft-of-exile", "item.open_on_craftofexile", Field::Widget("item-check", "craftOfExileKey")),
        ("item-check-stash-search", "item.find_in_stash", Field::Widget("item-check", "stashSearchKey")),
    ] {
        // `v-if="isEnglish"` on the Craft of Exile row.
        if key == "item-check-craft-of-exile" && !is_english(settings) {
            continue;
        }
        rows = rows.child(single_row(settings, key, label_key, field, false, project));
    }

    // EE2: `item.find_same_price` — a label, the kind select and the hotkey.
    if settings.game == GameId::Poe2 {
        let kinds = vec![
            ui::select::Choice::new("price", tr(settings, "item.price")),
            ui::select::Choice::new("both", tr(settings, "item.price_and_item")),
        ];
        let kind = settings.string(Field::Widget("item-check", "samePricedType"));
        let field = Field::Widget("item-check", "samePricedKey");
        let value = settings.string(field);
        let value = (!value.is_empty()).then_some(value);
        rows = rows.child(
            row()
                .key("item-check-same-price")
                .w_full()
                .items_center()
                .gap(theme::rem(0.5))
                .child(
                    text(tr(settings, "item.find_same_price"))
                        .key("item-check-same-price-label")
                        .font(type_::SM)
                        .grow(1.0),
                )
                .child(ui::select::select(
                    "item-check-same-price-select",
                    &kinds,
                    &kind,
                    settings.select_open == Some("item-check-same-price-select"),
                    theme::rem(8.0),
                    move |s: &mut S| {
                        let settings = project(s);
                        settings.select_open = (settings.select_open != Some("item-check-same-price-select"))
                            .then_some("item-check-same-price-select");
                        settings.recording = None;
                    },
                    move |s: &mut S, value: String| {
                        project(s).set_string(Field::Widget("item-check", "samePricedType"), &value);
                        project(s).select_open = None;
                    },
                ))
                .child(hotkey_input(
                    "item-check-same-price-hotkey",
                    value.as_deref(),
                    hotkey::WIDE,
                    &hotkey::no_key(settings.game),
                    super::hotkeys::is_recording(settings, field),
                    move |s: &mut S| {
                        let settings = project(s);
                        settings.recording = Some(Recording { field, required: false, no_mod_keys: false });
                        settings.select_open = None;
                    },
                )),
        );
    }
    page("page-item-check").child(rows)
}
