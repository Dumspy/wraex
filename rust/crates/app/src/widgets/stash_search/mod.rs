//! `ref/apt/renderer/src/web/stash-search/WidgetStashSearch.vue` (EE2's copy is the same
//! widget): the configured stash searches, one button each.
//!
//! An `instances: 'multi'` widget: `defaultConfig()` ships two, "Map rolling" and "Dump
//! sorting", and a user can add more. The card lists the widget's `entries`, each a search
//! string (`text`) with an optional friendly `name` and `hotkey`. Pressing one does not type
//! into the game here: it records [`crate::widgets::Action::StashSearch`], upstream's
//! `CLIENT->MAIN::user-action`, which the app shell runs — the same split as the item check's
//! "Find in Stash". One press is one action.
//!
//! The "Keys" switch is `config.enableHotkeys` (`v-model`, pushed to the shell by
//! `pushHostConfig`): it hides the hotkey labels, and gates the hotkeys themselves, which the
//! shell registers. No state beyond the config: [`crate::widgets::Overlay`] hands the view the
//! widget's own object.

use rune::{El, col, el, row, text};
use serde::Deserialize;
use serde_json::Value;

use super::lenient;
use crate::theme::{self, text as type_};
use crate::ui;
use crate::widgets::{Action, Overlay, Project, Widget};

/// `WidgetStashSearch.vue`'s `widget.type`.
pub const WM_TYPE: &str = "stash-search";

/// One `entries[]` row of the widget's config.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: f64,
    /// `friendly name`: what the button shows instead of the search text.
    pub name: String,
    /// `search text or regex`: the string typed into the stash's search box.
    pub text: String,
    /// A hotkey label like `Ctrl+Shift+1`, drawn while `enableHotkeys` is on.
    pub hotkey: Option<String>,
}

/// One `entries[]` object, read leniently: a missing or odd field is its fallback.
#[derive(Debug, Default, Deserialize)]
struct EntryView {
    #[serde(default, deserialize_with = "lenient")]
    id: Option<f64>,
    #[serde(default, deserialize_with = "lenient")]
    name: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    text: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    hotkey: Option<String>,
}

/// The widget's own fields this module reads.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StashView {
    #[serde(default, deserialize_with = "lenient")]
    entries: Option<Vec<Value>>,
    #[serde(default, deserialize_with = "lenient")]
    enable_hotkeys: Option<bool>,
}

/// `config`, as this module reads it; a widget object is always one.
fn settings(widget: &Widget) -> StashView {
    StashView::deserialize(&widget.config).unwrap_or_default()
}

impl Entry {
    /// The entry's object in the config.
    pub fn from_config(entry: &Value) -> Self {
        let view = EntryView::deserialize(entry).unwrap_or_default();
        Self {
            id: view.id.unwrap_or_default(),
            name: view.name.unwrap_or_default(),
            text: view.text.unwrap_or_default(),
            hotkey: view.hotkey,
        }
    }

    /// `{{ entry.name || entry.text }}`: the button's label.
    pub fn label(&self) -> &str {
        if self.name.is_empty() { &self.text } else { &self.name }
    }
}

/// `config.entries`, in config order.
pub fn entries(widget: &Widget) -> Vec<Entry> {
    settings(widget).entries.unwrap_or_default().iter().map(Entry::from_config).collect()
}

/// `config.enableHotkeys`.
pub fn enable_hotkeys(widget: &Widget) -> bool {
    settings(widget).enable_hotkeys.unwrap_or(false)
}

/// `hasHotkeys`: the switch is drawn only when some entry has one.
pub fn has_hotkeys(entries: &[Entry]) -> bool {
    entries.iter().any(|entry| entry.hotkey.is_some())
}

/// `v-model="config.enableHotkeys"`: flips the widget's config (`pushHostConfig` is the
/// shell's half).
pub fn toggle_hotkeys(overlay: &mut Overlay, wm_id: u32) {
    let Some(widget) = overlay.widget_mut(wm_id) else { return };
    let on = enable_hotkeys(widget);
    if let Value::Object(object) = &mut widget.config {
        object.insert("enableHotkeys".to_owned(), Value::Bool(!on));
    }
}

/// `stashSearch(entry.text)`: the shell types it into the stash's search box.
pub fn press(overlay: &mut Overlay, text: &str) {
    overlay.actions.push(Action::StashSearch(text.to_owned()));
}

/// `WidgetStashSearch.vue`: the header (title and "Keys") and one button per entry.
pub fn view<S: 'static>(overlay: &Overlay, widget: &Widget, project: Project<S>) -> El {
    let entries = entries(widget);
    let id = widget.wm_id;
    let mut list = col().key("stash-search-entries").gap(theme::rem(0.25));
    for (index, entry) in entries.iter().enumerate() {
        list = list.child(entry_button(widget, index, entry, project));
    }

    let hotkeys = has_hotkeys(&entries);
    let mut header = row().key("stash-search-header").w_full().padding(theme::rem(0.25)).items_center();
    // `<span class="truncate">{{ config.wmTitle || 'Untitled' }}</span>`
    header = header.child(
        text(if widget.wm_title.is_empty() { "Untitled".to_owned() } else { widget.wm_title.clone() })
            .key("stash-search-title")
            .font(type_::BASE)
            .truncate(),
    );
    if hotkeys {
        header = header.child(el().grow(1.0)).child(ui::toggle(
            "stash-search-keys",
            &crate::settings::t(overlay.game, "stash_search.enable_keys"),
            enable_hotkeys(widget),
            move |s: &mut S| toggle_hotkeys(project(s), id),
        ));
    }

    // `widget-default-style p-1 flex flex-col overflow-y-auto min-h-0` with
    // `style="min-width: 5rem"`, inside the `.widget` frame the overlay adds. The card is as
    // wide as its entries (`target/screens/<game>/stash-search.png` measures 166x176 for
    // "Map rolling"), which is what sizes the scene's window.
    col()
        .key("stash-search")
        .min_w(theme::rem(5.0))
        .padding(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        .child(col().key("stash-search-body").gap(theme::rem(0.25)).child(header).child(list))
}

/// `.searchBtn`: `rounded max-w-sm p-2 leading-4 text-gray-100 bg-gray-800`, the label in a
/// span and the hotkey badge after it.
fn entry_button<S: 'static>(widget: &Widget, index: usize, entry: &Entry, project: Project<S>) -> El {
    let label = entry.label().to_owned();
    let mut button = row()
        .key(format!("stash-search-entry-{index}"))
        .max_w(theme::rem(24.0))
        .px(theme::rem(0.5))
        .py(theme::rem(0.5))
        .radius(theme::ROUNDED)
        .fill(theme::gray(800))
        .hover(theme::gray(700))
        .focusable(true)
        .role("button")
        .label(label.clone())
        .a11y_value(entry.text.clone())
        .child(
            // `p-2 leading-4` in the reference; Rune's 14 pt line keeps "Map rolling" at the
            // reference screenshot's 166x176.
            text(label).key(format!("stash-search-entry-{index}-text")).font(theme::leading_none(type_::SM)).truncate(),
        );
    if let Some(hotkey) = entry.hotkey.clone()
        && enable_hotkeys(widget)
    {
        // `<span class="text-black bg-gray-400 rounded px-1 ml-1">{{ entry.hotkey }}</span>`
        button = button.child(
            row()
                .key(format!("stash-search-entry-{index}-hotkey"))
                .ml(theme::rem(0.25))
                .px(theme::rem(0.25))
                .radius(theme::ROUNDED)
                .fill(theme::gray(400))
                .center()
                .child(
                    text(hotkey)
                        .key(format!("stash-search-entry-{index}-hotkey-text"))
                        .font(theme::leading_none(type_::SM))
                        .color(theme::gray(900)),
                ),
        );
    }
    // `@click="stashSearch(entry.text)"`
    let text = entry.text.clone();
    button.on_press(move |s: &mut S| press(project(s), &text))
}

#[cfg(test)]
#[expect(clippy::disallowed_macros, reason = "expected JSON written as literals")]
mod tests {
    use super::*;
    use crate::widgets::Kind;

    fn widget(game: wraex_core::GameId) -> Widget {
        let overlay = Overlay::scene(game, Kind::StashSearch);
        overlay.shown_widget().cloned().expect("the stash-search widget")
    }

    /// `defaultConfig()`'s first instance, as `WidgetStashSearch.vue`'s `defaultInstances`
    /// writes it.
    #[test]
    fn the_default_configs_entries() {
        let rolling = widget(wraex_core::GameId::Poe1);
        assert_eq!(rolling.wm_type, WM_TYPE);
        assert_eq!(rolling.wm_title, "Map rolling");
        assert!(enable_hotkeys(&rolling));
        let found = entries(&rolling);
        let texts: Vec<&str> = found.iter().map(|entry| entry.text.as_str()).collect();
        assert_eq!(texts, ["\"Pack Size: +3\"", "Reflect", "\"Cannot Leech Life\"", "\"Cannot Leech Mana\""]);
        assert!(entries(&rolling).iter().all(|entry| entry.name.is_empty()), "no friendly names");
        assert!(!has_hotkeys(&entries(&rolling)), "no hotkey in the default config");

        // EE2 ships its own lists for PoE2.
        let poe2 = widget(wraex_core::GameId::Poe2);
        assert_eq!(entries(&poe2)[0].text, "\"Increased Pack Size\"");
    }

    /// The config's second instance is the same widget with another list ("Dump sorting").
    #[test]
    fn the_second_instance_sorts_a_dump() {
        let config = crate::settings::default_config(wraex_core::GameId::Poe1);
        let widgets: Vec<Widget> =
            crate::widgets::overlay_widgets(&config).into_iter().filter(|widget| widget.wm_type == WM_TYPE).collect();
        assert_eq!(widgets.len(), 2, "`defaultInstances` ships two");
        let sorting = &widgets[1];
        assert_eq!(sorting.wm_title, "Dump sorting");
        let texts: Vec<String> = entries(sorting).iter().map(|entry| entry.text.clone()).collect();
        assert_eq!(
            texts,
            [
                "Currency",
                "\"Divination Card\"",
                "Fossil",
                "\"Map Tier\"",
                "\"Map Device\" \"Rarity: Normal\"",
                "Tane Laboratory"
            ]
        );
    }

    /// A friendly name replaces the search text on the button (`entry.name || entry.text`).
    #[test]
    fn a_friendly_name_labels_the_button() {
        let entry = Entry::from_config(&serde_json::json!({
            "id": 7,
            "name": "Burning",
            "text": "\"Burning Ground\"",
            "hotkey": "Ctrl+Shift+1",
        }));
        assert_eq!(entry.label(), "Burning");
        assert_eq!(entry.hotkey.as_deref(), Some("Ctrl+Shift+1"));
        assert!(has_hotkeys(std::slice::from_ref(&entry)));

        let unnamed =
            Entry::from_config(&serde_json::json!({ "id": 1, "name": "", "text": "Currency", "hotkey": null }));
        assert_eq!(unnamed.label(), "Currency");
        assert_eq!(unnamed.hotkey, None);
    }

    /// One press records upstream's event once, with the entry's exact text.
    #[test]
    fn a_press_records_the_stash_search() {
        let mut overlay = Overlay::scene(wraex_core::GameId::Poe1, Kind::StashSearch);
        let entries = entries(overlay.shown_widget().expect("the widget"));
        press(&mut overlay, &entries[0].text);
        let actions = overlay.take_actions();
        let expected = Action::StashSearch("\"Pack Size: +3\"".to_owned());
        assert_eq!(actions, [expected]);
        assert!(overlay.take_actions().is_empty(), "one press, one action");
        // Upstream's `MainProcess.sendEvent` payload, byte for byte.
        let Action::StashSearch(text) = &actions[0] else { panic!("a stash search") };
        assert_eq!(
            serde_json::json!({
                "name": "CLIENT->MAIN::user-action",
                "payload": { "action": "stash-search", "text": text },
            }),
            serde_json::json!({
                "name": "CLIENT->MAIN::user-action",
                "payload": { "action": "stash-search", "text": "\"Pack Size: +3\"" },
            })
        );
    }

    /// `v-model="config.enableHotkeys"` writes the widget's own config object.
    #[test]
    fn the_keys_switch_flips_the_config() {
        let mut overlay = Overlay::scene(wraex_core::GameId::Poe1, Kind::StashSearch);
        let id = overlay.shown;
        assert!(enable_hotkeys(overlay.shown_widget().unwrap()));
        toggle_hotkeys(&mut overlay, id);
        assert!(!enable_hotkeys(overlay.shown_widget().unwrap()));
        assert_eq!(overlay.shown_widget().unwrap().config["enableHotkeys"], serde_json::Value::Bool(false));
        toggle_hotkeys(&mut overlay, id);
        assert!(enable_hotkeys(overlay.shown_widget().unwrap()));
    }
}
