//! What a widget, the tray or a hotkey asked the app to do, and nothing about how.
//!
//! Upstream's main process runs these effects itself: `window.open(url)` in the renderer,
//! `Host.sendEvent` to the main process, `AppTray.ts`'s dialogs, Electron's `shell.openPath`.
//! The port's widgets only record what they want ([`crate::widgets::Action`],
//! [`crate::poe1::item_check::Action`], [`wraex_platform::tray::TrayAction`]); this module
//! turns each into a [`Request`] the live shell runs. Keeping the translation here means the
//! effects — a browser, a folder, the game — stay out of the scenes and the tests, and the
//! mapping is testable without any of them.

use wraex_platform::shortcuts::ShortcutEvent;
use wraex_platform::tray::TrayAction;

/// One thing to do on the UI thread, with the OS effects left to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// `window.open(url)` / Electron's `shell.openExternal`.
    OpenUrl(String),
    /// `wm.show` on the settings widget: the settings window.
    Settings,
    /// The tray's "Open config folder" (`shell.openPath`).
    ConfigFolder,
    /// The tray's "Quit" (`app.quit`).
    Quit,
    /// `CLIENT->MAIN::user-action` with `action: 'stash-search'`: type the text into the
    /// stash's search box (one action, the game keeps the focus afterwards).
    StashSearch(String),
    /// `MAIN->CLIENT::item-text` for `price-check`.
    PriceCheck(String),
    /// `MAIN->CLIENT::item-text` for `item-check`: open the widget, and for the item check's
    /// own hotkeys (`wikiKey`, ...) press the button whose page it opens.
    ItemCheck { text: String, action: Option<ItemAction> },
    /// `MAIN->CLIENT::widget-action`: a widget's hotkey (the delve grid's toggle, a timer's
    /// start/stop).
    WidgetAction(String),
    /// The quick-math bar's hotkey (Wraex's own `quickMathKey`): open it at the cursor, or close
    /// it if it is already up.
    QuickMath,
    /// Draw the widget the config's `wmWants` now shows (`wm.show(wmId)` from the menu).
    ShowWidget(u32),
    /// Nothing the port does yet, and why.
    Unsupported(String),
}

/// The item check's hotkeyable actions (`WidgetItemCheck.vue`'s `hotkeyable-actions`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemAction {
    Wiki,
    Poedb,
    CraftOfExile,
    Similar,
}

/// The tray's menu entries. "Settings/League" is a message box upstream; here it is the
/// settings window, whose price-check page is where the league lives.
pub fn tray_request(action: TrayAction) -> Request {
    match action {
        TrayAction::ShowSettingsHint => Request::Settings,
        TrayAction::OpenConfigFolder => Request::ConfigFolder,
        TrayAction::Quit => Request::Quit,
    }
}

/// The PoE1 item check's recorded action.
pub fn poe1_item_request(action: &crate::poe1::item_check::Action) -> Request {
    use crate::poe1::item_check::Action;
    match action {
        Action::Open(url) => Request::OpenUrl(url.clone()),
        Action::Send(user) => Request::StashSearch(user.payload.text.clone()),
    }
}

/// The PoE2 item check's recorded action.
pub fn poe2_item_request(action: &crate::poe2::item_check::Action) -> Request {
    use crate::poe2::item_check::Action;
    match action {
        Action::Open(url) => Request::OpenUrl(url.clone()),
        // `undefined` for a nameless item: the widget sends nothing, so neither does the shell.
        Action::Send(user) => match user.payload.text.clone() {
            Some(text) => Request::StashSearch(text),
            None => Request::Unsupported("stash search without an item name".to_owned()),
        },
    }
}

/// An overlay widget's recorded action.
pub fn widget_request(action: &crate::widgets::Action) -> Request {
    use crate::widgets::Action;
    match action {
        Action::Show(id) => Request::ShowWidget(*id),
        // The window follows the state (`App::poll` hides it when nothing is open), so a hide
        // needs no effect of its own.
        Action::Hide(_) => Request::Unsupported("hide".to_owned()),
        Action::Create(wm_type) => Request::Unsupported(format!("create {wm_type}")),
        Action::Remove(_) => Request::Unsupported("remove".to_owned()),
        Action::EditInSettings(_) => Request::Settings,
        Action::PriceCheck(text) => Request::PriceCheck(text.clone()),
        Action::ImportImage(_) => Request::Unsupported("import image".to_owned()),
        Action::StashSearch(text) => Request::StashSearch(text.clone()),
        // Applied by `App::poll` (`open_picked_item`), which has the parsed item.
        Action::PriceCheckItem(_) => Request::Unsupported("price-check item".to_owned()),
    }
}

/// A hotkey press's events ([`wraex_platform::shortcuts::press`]).
pub fn shortcut_requests(events: Vec<ShortcutEvent>) -> Vec<Request> {
    events
        .into_iter()
        .map(|event| match event {
            ShortcutEvent::ItemText(item) => {
                let target = item.target.as_str();
                match target {
                    "price-check" => Request::PriceCheck(item.clipboard),
                    "item-check" => Request::ItemCheck { text: item.clipboard, action: None },
                    "open-wiki" => Request::ItemCheck { text: item.clipboard, action: Some(ItemAction::Wiki) },
                    "open-poedb" => Request::ItemCheck { text: item.clipboard, action: Some(ItemAction::Poedb) },
                    "open-craft-of-exile" => {
                        Request::ItemCheck { text: item.clipboard, action: Some(ItemAction::CraftOfExile) }
                    }
                    "search-similar" => Request::ItemCheck { text: item.clipboard, action: Some(ItemAction::Similar) },
                    other => Request::Unsupported(format!("item text for {other}")),
                }
            }
            ShortcutEvent::WidgetAction { target } => {
                // Wraex's own bar, not an upstream widget: its hotkey is a `trigger-event` like
                // the delve grid's, but the shell opens a window rather than a widget action.
                if target == crate::quick_math::WM_TYPE { Request::QuickMath } else { Request::WidgetAction(target) }
            }
            ShortcutEvent::OcrRequest { target, .. } => Request::Unsupported(format!("OCR for {target}")),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wraex_platform::shortcuts::ItemText;
    use wraex_platform::window::Point;

    #[test]
    fn the_trays_entries_map_to_the_shell() {
        assert_eq!(tray_request(TrayAction::ShowSettingsHint), Request::Settings);
        assert_eq!(tray_request(TrayAction::OpenConfigFolder), Request::ConfigFolder);
        assert_eq!(tray_request(TrayAction::Quit), Request::Quit);
    }

    #[test]
    fn a_price_check_press_opens_the_window_with_the_copied_item() {
        let events = vec![ShortcutEvent::ItemText(ItemText {
            target: "price-check".to_owned(),
            clipboard: "Rarity: Currency\nDivine Orb".to_owned(),
            position: Point { x: 1.0, y: 2.0 },
            focus_overlay: false,
        })];
        assert_eq!(shortcut_requests(events), vec![Request::PriceCheck("Rarity: Currency\nDivine Orb".to_owned())]);
    }

    #[test]
    fn the_item_checks_hotkeys_open_its_pages() {
        let text = "Rarity: Unique\nLethal Pride".to_owned();
        for (target, action) in [
            ("open-wiki", ItemAction::Wiki),
            ("open-poedb", ItemAction::Poedb),
            ("open-craft-of-exile", ItemAction::CraftOfExile),
            ("search-similar", ItemAction::Similar),
            ("item-check", ItemAction::Wiki),
        ] {
            let events = vec![ShortcutEvent::ItemText(ItemText {
                target: target.to_owned(),
                clipboard: text.clone(),
                position: Point { x: 0.0, y: 0.0 },
                focus_overlay: false,
            })];
            let expected = if target == "item-check" {
                Request::ItemCheck { text: text.clone(), action: None }
            } else {
                Request::ItemCheck { text: text.clone(), action: Some(action) }
            };
            assert_eq!(shortcut_requests(events), vec![expected], "{target}");
        }
    }

    #[test]
    fn a_timer_or_delve_hotkey_is_a_widget_action() {
        assert_eq!(
            shortcut_requests(vec![ShortcutEvent::WidgetAction { target: "stopwatch-start-stop:3".to_owned() }]),
            vec![Request::WidgetAction("stopwatch-start-stop:3".to_owned())]
        );
    }

    #[test]
    fn the_quick_math_hotkey_opens_the_bar_rather_than_a_widget_action() {
        assert_eq!(
            shortcut_requests(vec![ShortcutEvent::WidgetAction { target: crate::quick_math::WM_TYPE.to_owned() }]),
            vec![Request::QuickMath]
        );
    }

    #[test]
    fn an_item_checks_buttons_open_pages_or_search_the_stash() {
        use wraex_poe1::item_check::{StashSearch, UserAction};
        let page = crate::poe1::item_check::Action::Open("https://www.poewiki.net/wiki/Orb".to_owned());
        assert_eq!(poe1_item_request(&page), Request::OpenUrl("https://www.poewiki.net/wiki/Orb".to_owned()));
        let search = crate::poe1::item_check::Action::Send(UserAction {
            name: "CLIENT->MAIN::user-action",
            payload: StashSearch { action: "stash-search", text: "\"~price 5 chaos\"".to_owned() },
        });
        assert_eq!(poe1_item_request(&search), Request::StashSearch("\"~price 5 chaos\"".to_owned()));

        let page = crate::poe2::item_check::Action::Open("https://poe2db.tw/us/Bows".to_owned());
        assert_eq!(poe2_item_request(&page), Request::OpenUrl("https://poe2db.tw/us/Bows".to_owned()));
        let search = crate::poe2::item_check::Action::Send(wraex_poe2::item_check::UserAction {
            name: "CLIENT->MAIN::user-action",
            payload: wraex_poe2::item_check::StashSearch { action: "stash-search", text: None },
        });
        assert!(matches!(poe2_item_request(&search), Request::Unsupported(_)));
    }

    #[test]
    fn the_widgets_actions_map_to_the_shell() {
        use crate::widgets::Action;
        assert_eq!(widget_request(&Action::PriceCheck("text".to_owned())), Request::PriceCheck("text".to_owned()));
        assert_eq!(
            widget_request(&Action::StashSearch("\"Currency\"".to_owned())),
            Request::StashSearch("\"Currency\"".to_owned())
        );
        // `wm.show(wmId)` names the widget by its id; the settings widget opens the settings
        // window in the shell.
        assert_eq!(widget_request(&Action::Show(2)), Request::ShowWidget(2));
        assert_eq!(widget_request(&Action::EditInSettings(2)), Request::Settings);
        assert!(matches!(widget_request(&Action::Hide(2)), Request::Unsupported(_)));
        assert!(matches!(widget_request(&Action::ImportImage(2)), Request::Unsupported(_)));
    }
}
