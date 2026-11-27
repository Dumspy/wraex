//! System-wide hotkeys and the synthetic input they send.
//!
//! Ported from `main/src/shortcuts/Shortcuts.ts` and `main/src/shortcuts/text-box.ts`
//! (identical apart from EE2's additions, selected here by [`GameId`] and spelled
//! out where they matter), plus `ipc/KeyToCode.ts`'s helpers, which already live in
//! [`wraex_core::keys`].
//!
//! The module decides three things, and does nothing to the OS itself:
//!
//! 1. **Which hotkeys are registered.** [`Shortcuts::update_actions`] takes the
//!    host config's shortcut list and filters it like upstream: the game's own keys
//!    (`Ctrl + C`, `Ctrl + F`, `Enter`, ...) can never be bound, and EE2 also
//!    reserves the `Ctrl + C`/`<game key> + C` copy shortcut when the game's
//!    "show advanced item descriptions" key is not Ctrl. [`Registration`] follows
//!    the game's focus — upstream registers on `active-change` and unregisters when
//!    the game loses it, so Wraex never sees a keypress outside the game.
//! 2. **What a press does.** [`Shortcuts::press`] releases the keys the user was
//!    holding, then runs one action: copy the item (Ctrl+C, or Ctrl+Alt+C when the
//!    game shows advanced descriptions on Alt), type in chat or the stash search
//!    (clipboard + Enter/Ctrl+F), toggle the overlay, or ask the app to act
//!    (`trigger-event`, the heist gem OCR).
//! 3. **What the wheel does.** [`Shortcuts::on_wheel`] turns Ctrl+wheel outside the
//!    stash panel into the stash tabs' ArrowLeft/ArrowRight.
//!
//! Every OS effect is a trait call, so the tests record the exact sequence and its
//! delays: [`KeySender`] (synthetic keys, upstream's `uIOhook.keyTap`/`keyToggle`
//! and `setTimeout`), [`Clipboard`] and [`Clock`] (via [`HostClipboard`]), and
//! [`OverlayHandle`] where the overlay has to react mid-sequence
//! (`stash-search` gives the game focus before typing). One action per keypress, as
//! upstream: no key is repeated, nothing is polled or looped on the game.
//!
//! What stays OS-specific: registering the accelerators (`shortcutToElectron`'s
//! names, see [`accelerator`]; `globalShortcut.register` in upstream), the key
//! injection itself (upstream `uiohook-napi`; PLAN.md's candidates are `rdev`,
//! `global-hotkey` and `enigo` / Win32 `SendInput`), the low-level keyboard and
//! wheel hooks that produce [`InputEvent`]/[`WheelEvent`], and the names for the
//! keys the hook reports. The spike in PLAN.md phase 2 picks among them, so this
//! module names no crate.

use std::cmp::Ordering;

use serde::Deserialize;
use wraex_core::GameId;
use wraex_core::game_files::Os;
use wraex_core::keys::{is_mod_key, key_to_code, merge_two_hotkeys};
use wraex_core::trade::api::Clock;
use wraex_core::trade::rate_limit::Millis;

use crate::clipboard::{Clipboard, HostClipboard};
use crate::overlay::{OverlayController, OverlayHandle};
use crate::window::{GameWindow, Point};

/// The game's own hotkeys, in upstream's order. Upstream never binds them: they
/// are what the game needs for its own copy, chat and stash input.
pub const RESERVED_SHORTCUTS: [&str; 11] = [
    "Ctrl + C",
    "Ctrl + V",
    "Ctrl + A",
    "Ctrl + F",
    "Ctrl + Enter",
    "Home",
    "Delete",
    "Enter",
    "ArrowUp",
    "ArrowRight",
    "ArrowLeft",
];

/// The placeholders a chat message can hold (`text-box.ts`): `@last msg` opens the
/// whisper reply before pasting `msg`, and `msg @last` does it after.
const PLACEHOLDER_LAST: &str = "@last";

/// The first characters whose chat channel accepts typed text as is, so the input
/// does not have to be cleared first.
const AUTO_CLEAR: [char; 6] = ['#', '%', '@', '$', '&', '/'];

/// How long EE2 holds the copy keys down (the game dropped the releases without
/// the pause).
const EE2_KEY_RELEASE_DELAY: Millis = 10;

/// One synthetic key event, as [`KeySender`] performs it.
pub trait KeySender: std::fmt::Debug {
    /// `uIOhook.keyToggle(key, 'down')`.
    fn key_down(&mut self, key: &str);
    /// `uIOhook.keyToggle(key, 'up')`.
    fn key_up(&mut self, key: &str);
    /// `uIOhook.keyTap(key, modifiers)`: modifiers down, key down and up,
    /// modifiers up.
    fn key_tap(&mut self, key: &str, modifiers: &[&str]);
    /// Upstream's `setTimeout` between steps: the app's timer waits, the test's
    /// fake records `Sleep(millis)`.
    fn sleep(&mut self, millis: Millis);
}

/// One entry of the host config (`ipc/types.ts`' `ShortcutAction`): the hotkey and
/// what it runs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutAction {
    /// `Ctrl + Shift + C`: keys joined with ` + `, modifiers first.
    pub shortcut: String,
    /// `keepModKeys`: release only the letter, leave the modifiers the user holds
    /// down to the game.
    #[serde(default)]
    pub keep_mod_keys: bool,
    pub action: Action,
}

/// The `action` half of a [`ShortcutAction`], tagged with `type`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Action {
    /// `copy-item`: copy the item under the cursor and send it to `target`.
    CopyItem {
        target: String,
        /// Whether the widget takes the focus once the item text is there.
        #[serde(default, rename = "focusOverlay")]
        focus_overlay: bool,
    },
    /// `ocr-text`: screenshot and OCR the heist gems (`win32` only).
    OcrText { target: String },
    /// `trigger-event`: `MAIN->CLIENT::widget-action` for `target`.
    TriggerEvent { target: String },
    /// `stash-search`: paste `text` into the stash's search box.
    StashSearch {
        #[serde(default)]
        text: String,
    },
    /// `toggle-overlay`: the overlay key.
    ToggleOverlay,
    /// `paste-in-chat`: `text`, optionally sent with Enter.
    PasteInChat {
        #[serde(default)]
        text: String,
        #[serde(default)]
        send: bool,
    },
    /// `test-only`: registered and unregistered again, to prove a hotkey works.
    TestOnly,
}

impl Action {
    /// `action.type`, for logging and matching.
    pub const fn type_name(&self) -> &'static str {
        match self {
            Self::CopyItem { .. } => "copy-item",
            Self::OcrText { .. } => "ocr-text",
            Self::TriggerEvent { .. } => "trigger-event",
            Self::StashSearch { .. } => "stash-search",
            Self::ToggleOverlay => "toggle-overlay",
            Self::PasteInChat { .. } => "paste-in-chat",
            Self::TestOnly => "test-only",
        }
    }
}

/// The `updateActions` options besides the shortcut list (`restoreClipboard`,
/// `stashScroll`, `logKeys`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutOptions {
    /// Put the clipboard back after a hotkey wrote to it.
    pub restore_clipboard: bool,
    /// Ctrl+wheel changes stash tabs.
    pub stash_scroll: bool,
    /// Log every keypress (for recording a hotkey).
    pub log_keys: bool,
}

/// `HostConfig` (`ipc/types.ts`): what the renderer sends the host. Only the fields
/// these platform modules read are listed; unknown ones (client log paths, the
/// library settings, ...) are ignored by serde.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConfig {
    /// Every configured hotkey, in the settings' order.
    pub shortcuts: Vec<ShortcutAction>,
    /// `restoreClipboard`, `stashScroll` and `logKeys`, as flat keys like upstream's.
    #[serde(flatten)]
    pub options: ShortcutOptions,
    /// The key that toggles the overlay.
    pub overlay_key: String,
    /// The game window's title, for the window hook ([`crate::window`]).
    pub window_title: String,
    /// The client language; the OCR reads it, which this port leaves to the app.
    pub language: String,
    /// EE2 only: the clipboard poll's first delay in milliseconds.
    #[serde(default)]
    pub initial_delay: Option<Millis>,
    /// EE2 only: hide the overlay window while the game is not focused.
    #[serde(default)]
    pub hide_overlay_on_blur: bool,
}

/// Why a configured shortcut was dropped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutWarning {
    /// `Hotkey "{shortcut}" reserved by the game will not be registered.`
    Reserved { shortcut: String },
}

impl std::fmt::Display for ShortcutWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Reserved { shortcut } = self;
        write!(f, "Hotkey \"{shortcut}\" reserved by the game will not be registered.")
    }
}

/// What the host's registration should do after the game's focus changed
/// (`Shortcuts.register`/`unregister`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Registration {
    /// Nothing changed.
    None,
    /// `globalShortcut.register` every [`Shortcuts::registered_shortcuts`] entry.
    Register,
    /// `globalShortcut.unregisterAll`.
    UnregisterAll,
}

/// One accelerator to register, and the action behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredShortcut {
    /// `shortcutToElectron`, what `globalShortcut.register` gets.
    pub accelerator: String,
    /// The hotkey as the config spells it.
    pub shortcut: String,
    pub keep_mod_keys: bool,
    pub action: Action,
    /// `test-only`: register, then unregister straight away. The accelerator was
    /// only there to see whether the game or another application holds the key.
    pub unregister_immediately: bool,
}

/// What a press produced, for the app to forward to the renderer
/// (`MAIN->CLIENT::widget-action`, `MAIN->CLIENT::item-text`).
#[derive(Debug, Clone, PartialEq)]
pub enum ShortcutEvent {
    WidgetAction {
        target: String,
    },
    ItemText(ItemText),
    /// The heist gem finder's turn: screenshot the game and OCR it, then send
    /// `MAIN->CLIENT::ocr-text` with `press_time` (`Date.now()` at the press).
    OcrRequest {
        target: String,
        press_time: Millis,
    },
}

/// `MAIN->CLIENT::item-text`'s payload.
#[derive(Debug, Clone, PartialEq)]
pub struct ItemText {
    pub target: String,
    /// The item's text, which the renderer parses.
    pub clipboard: String,
    /// Where the cursor was when the hotkey fired (`screen.getCursorScreenPoint`),
    /// so the widget opens next to it.
    pub position: Point,
    pub focus_overlay: bool,
}

/// The overlay side of a press: the controller, the game window and the OS handle it
/// drives.
#[derive(Debug)]
pub struct OverlayContext<'a> {
    pub overlay: &'a mut OverlayController,
    pub window: &'a mut GameWindow,
    pub handle: &'a mut dyn OverlayHandle,
}

/// Everything a press acts on: the OS traits, the overlay, and the game's own
/// config. Built per press by the app; the overlay's events are drained from
/// [`OverlayController`] afterwards.
pub struct PressContext<'a> {
    pub keys: &'a mut dyn KeySender,
    pub system_clipboard: &'a mut dyn Clipboard,
    pub clock: &'a dyn Clock,
    pub window: &'a mut GameWindow,
    pub overlay: &'a mut OverlayController,
    pub overlay_handle: &'a mut dyn OverlayHandle,
    /// `screen.getCursorScreenPoint()` when the hotkey fired.
    pub cursor: Point,
    /// `GameConfig.showModsKey`: the game's "show advanced item descriptions" key
    /// (`Alt` until the config is read).
    pub show_mods_key: &'a str,
}

impl std::fmt::Debug for PressContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PressContext")
            .field("cursor", &self.cursor)
            .field("show_mods_key", &self.show_mods_key)
            .finish_non_exhaustive()
    }
}

/// The options `updateActions` receives besides the shortcut list, plus
/// `GameConfig.showModsKey`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateOptions {
    pub stash_scroll: bool,
    pub log_keys: bool,
    pub restore_clipboard: bool,
    /// `GameConfig.showModsKey`.
    pub show_mods_key: String,
    /// EE2's `initialDelay`; `None` keeps [`crate::clipboard::POLL_DELAY`].
    pub initial_delay: Option<Millis>,
}

/// `Shortcuts.ts`: the registered shortcuts, the wheel, and the key log.
#[derive(Debug, Clone)]
pub struct Shortcuts {
    game: GameId,
    os: Os,
    actions: Vec<ShortcutAction>,
    registered: bool,
    stash_scroll: bool,
    log_keys: bool,
    clipboard: HostClipboard,
}

impl Shortcuts {
    pub const fn new(game: GameId, os: Os) -> Self {
        Self {
            game,
            os,
            actions: Vec::new(),
            registered: false,
            stash_scroll: false,
            log_keys: false,
            clipboard: HostClipboard::new(game, os),
        }
    }

    /// The clipboard poller, for the app's timer
    /// ([`HostClipboard::pending_restore_deadline`]).
    pub const fn clipboard_mut(&mut self) -> &mut HostClipboard {
        &mut self.clipboard
    }

    /// The hotkeys in registration order.
    pub fn actions(&self) -> &[ShortcutAction] {
        &self.actions
    }

    /// `updateActions`: filter the config's shortcuts and remember the options.
    /// Returns the problems upstream writes to its log.
    pub fn update_actions(&mut self, actions: Vec<ShortcutAction>, options: &UpdateOptions) -> Vec<ShortcutWarning> {
        self.stash_scroll = options.stash_scroll;
        self.log_keys = options.log_keys;
        self.clipboard.update_options(options.restore_clipboard);
        if let Some(initial_delay) = options.initial_delay {
            self.clipboard.update_delay(initial_delay);
        }

        let mut reserved: Vec<String> = RESERVED_SHORTCUTS.iter().map(|shortcut| (*shortcut).to_owned()).collect();
        if self.game == GameId::Poe2 {
            // EE2 reserves the copy shortcut its `pressKeysToCopyItemText` sends
            // when the game's key is not Ctrl. (`updateActions` also pushes a
            // `test-only` entry for it, but adds the shortcut to the reserved set
            // first, so that entry is filtered out again and nothing registers.)
            let copy_item = merge_two_hotkeys("Ctrl + C", &options.show_mods_key);
            if copy_item != "Ctrl + C" {
                reserved.push(copy_item);
            }
        }

        let mut warnings = Vec::new();
        let mut kept = Vec::with_capacity(actions.len());
        for entry in actions {
            if !reserved.contains(&entry.shortcut) {
                kept.push(entry);
            } else if entry.action != Action::TestOnly {
                warnings.push(ShortcutWarning::Reserved { shortcut: entry.shortcut.clone() });
            }
        }
        // Upstream's second pass looks for duplicate hotkeys *after* the reserved
        // ones were filtered out, so it can never find one; its `toggle-overlay`
        // exception keeps a duplicate that cannot exist. Filtering in one pass is
        // the same behaviour.
        self.actions = kept;
        warnings
    }

    /// The host config's `updateActions` call (`shortcuts.updateActions(...)`),
    /// with the game's current key for advanced item descriptions.
    pub fn apply_host_config(&mut self, config: &HostConfig, show_mods_key: &str) -> Vec<ShortcutWarning> {
        self.update_actions(
            config.shortcuts.clone(),
            &UpdateOptions {
                stash_scroll: config.options.stash_scroll,
                log_keys: config.options.log_keys,
                restore_clipboard: config.options.restore_clipboard,
                show_mods_key: show_mods_key.to_owned(),
                initial_delay: config.initial_delay,
            },
        )
    }

    /// What `globalShortcut` must hold: every action with its accelerator.
    pub fn registered_shortcuts(&self) -> Vec<RegisteredShortcut> {
        self.actions
            .iter()
            .map(|entry| RegisteredShortcut {
                accelerator: accelerator(&entry.shortcut),
                shortcut: entry.shortcut.clone(),
                keep_mod_keys: entry.keep_mod_keys,
                action: entry.action.clone(),
                unregister_immediately: entry.action == Action::TestOnly,
            })
            .collect()
    }

    /// `active-change`: register while the game is focused, unregister otherwise.
    pub const fn on_game_active(&mut self, is_active: bool) -> Registration {
        if self.registered == is_active {
            return Registration::None;
        }
        self.registered = is_active;
        if is_active { Registration::Register } else { Registration::UnregisterAll }
    }

    /// Whether hotkeys are currently registered.
    pub const fn is_registered(&self) -> bool {
        self.registered
    }

    /// `logKeys`: every keypress is written to the log while the overlay key was
    /// recorded this way.
    pub const fn logs_keys(&self) -> bool {
        self.log_keys
    }

    /// `debug [Shortcuts] Keydown ...`.
    pub fn log_key_down(&self, event: &InputEvent) -> Option<String> {
        self.log_keys.then(|| format!("debug [Shortcuts] Keydown {}", event_to_string(event)))
    }

    /// `debug [Shortcuts] Keyup ...`.
    pub fn log_key_up(&self, key: Option<&str>) -> Option<String> {
        self.log_keys.then(|| format!("debug [Shortcuts] Keyup {}", key.unwrap_or("not_supported_key")))
    }

    /// `debug [Shortcuts] Action type: ...`.
    pub fn log_action(&self, action: &Action) -> Option<String> {
        self.log_keys.then(|| format!("debug [Shortcuts] Action type: {}", action.type_name()))
    }

    /// `CLIENT->MAIN::user-action`'s `stash-search`: the stash-search widget's own
    /// button, not a hotkey. Same as the hotkey's action without keys to release.
    pub fn user_stash_search(&mut self, text: &str, ctx: PressContext<'_>) {
        let PressContext { keys, system_clipboard, clock, window, overlay, overlay_handle, .. } = ctx;
        let os = self.os;
        self.clipboard.restore_shortly(system_clipboard, clock, |clipboard| {
            overlay.assert_game_active(window, overlay_handle);
            stash_search(text, clipboard, keys, os);
        });
    }

    /// One press of a registered hotkey: release what the user held, then run the
    /// action. Exactly one action per press.
    pub fn press(&mut self, entry: &ShortcutAction, ctx: PressContext<'_>) -> Vec<ShortcutEvent> {
        let PressContext { keys, system_clipboard, clock, window, overlay, overlay_handle, show_mods_key, .. } = ctx;
        let game = self.game;
        let os = self.os;

        if matches!(entry.action, Action::CopyItem { .. }) && self.clipboard.is_polling() {
            return vec![];
        }
        release_pressed_keys(entry, keys);

        let mut events = Vec::new();
        match &entry.action {
            Action::ToggleOverlay => {
                overlay.stop_tracking_area();
                overlay.toggle_active_state(window, overlay_handle);
            }
            Action::PasteInChat { text, send } => {
                let send = *send;
                self.clipboard.restore_shortly(system_clipboard, clock, |clipboard| {
                    type_in_chat(text, send, clipboard, keys, os);
                });
            }
            Action::TriggerEvent { target } => {
                events.push(ShortcutEvent::WidgetAction { target: target.clone() });
            }
            Action::StashSearch { text } => {
                self.clipboard.restore_shortly(system_clipboard, clock, |clipboard| {
                    // The search box only takes the text while the game has focus.
                    overlay.assert_game_active(window, overlay_handle);
                    stash_search(text, clipboard, keys, os);
                });
            }
            Action::CopyItem { .. } => {
                self.clipboard.begin_capture(system_clipboard, clock, || {
                    press_keys_to_copy_item_text(game, os, entry, show_mods_key, keys);
                });
            }
            Action::OcrText { target } => {
                if os == Os::Windows && target == "heist-gems" {
                    events.push(ShortcutEvent::OcrRequest { target: target.clone(), press_time: clock.now() });
                }
            }
            Action::TestOnly => {}
        }
        events
    }

    /// Finish the original copy action with the cursor position recorded at the press.
    /// `None` when `entry` is not a copy action, which cannot happen: only a copy action
    /// starts a capture.
    pub fn finish_copy(
        entry: &ShortcutAction,
        cursor: Point,
        clipboard: String,
        overlay: OverlayContext<'_>,
    ) -> Option<ShortcutEvent> {
        let OverlayContext { overlay, window, handle } = overlay;
        let Action::CopyItem { target, focus_overlay } = &entry.action else {
            return None;
        };
        overlay.stop_tracking_area();
        if *focus_overlay && overlay.was_used_recently() {
            overlay.assert_overlay_active(window, handle);
        }
        Some(ShortcutEvent::ItemText(ItemText {
            target: target.clone(),
            clipboard,
            position: cursor,
            focus_overlay: *focus_overlay,
        }))
    }

    /// Ctrl+wheel outside the stash panel walks the stash tabs. Returns whether a
    /// key was sent.
    pub fn on_wheel(&self, wheel: &WheelEvent, window: &GameWindow, keys: &mut dyn KeySender) -> bool {
        if !wheel.ctrl || !window.is_active() || !self.stash_scroll {
            return false;
        }
        if is_stash_area(wheel.point, window) {
            return false;
        }
        match wheel.rotation.cmp(&0) {
            Ordering::Greater => {
                keys.key_tap("ArrowRight", &[]);
                true
            }
            Ordering::Less => {
                keys.key_tap("ArrowLeft", &[]);
                true
            }
            Ordering::Equal => false,
        }
    }
}

/// The start of every hotkey action: release the keys the user held. With
/// `keepModKeys` only the letter goes, so the game still sees its modifier; an
/// overlay or a lock key presses the release of the whole hotkey, in reverse.
fn release_pressed_keys(entry: &ShortcutAction, keys: &mut dyn KeySender) {
    if entry.keep_mod_keys {
        // The game still needs the modifiers the user is holding: release only the
        // letter.
        if let Some(non_mod) = entry.shortcut.split(" + ").find(|key| !is_mod_key(key)) {
            keys.key_up(non_mod);
        }
    } else {
        let all: Vec<&str> = entry.shortcut.split(" + ").collect();
        for key in all.iter().rev() {
            keys.key_up(key);
        }
    }
}

/// The modifiers `pressKeysToCopyItemText` holds down for the copy.
pub fn copy_item_modifiers(game: GameId, os: Os, entry: &ShortcutAction, show_mods_key: &str) -> Vec<String> {
    let show_mods_key = if game == GameId::Poe1 { "Ctrl" } else { show_mods_key };
    let mut keys: Vec<String> = merge_two_hotkeys("Ctrl + C", show_mods_key).split(" + ").map(str::to_owned).collect();
    keys.retain(|key| key != "C");
    if os != Os::MacOs {
        // On non-Mac platforms, don't toggle keys that are already being pressed.
        // On Mac the game needs them toggled for advanced mod descriptions to be
        // copied (upstream: an "Alt + letter" hotkey fails otherwise).
        let held: Vec<&str> = if entry.keep_mod_keys {
            entry.shortcut.split(" + ").filter(|key| is_mod_key(key)).collect()
        } else {
            Vec::new()
        };
        keys.retain(|key| !held.contains(&key.as_str()));
    }
    keys
}

/// `pressKeysToCopyItemText`: hold the modifiers, tap C, release them.
pub fn press_keys_to_copy_item_text(
    game: GameId,
    os: Os,
    entry: &ShortcutAction,
    show_mods_key: &str,
    keys: &mut dyn KeySender,
) {
    let modifiers = copy_item_modifiers(game, os, entry, show_mods_key);
    for modifier in &modifiers {
        keys.key_down(modifier);
    }
    // Finally press `C` to copy the text.
    keys.key_tap("C", &[]);
    if game == GameId::Poe2 {
        // EE2 waits before releasing: the game dropped the releases otherwise.
        keys.sleep(EE2_KEY_RELEASE_DELAY);
    }
    for modifier in modifiers.iter().rev() {
        keys.key_up(modifier);
    }
}

/// `typeInChat`: type `text` into the game's chat through the clipboard.
pub fn type_in_chat(text: &str, send: bool, clipboard: &mut dyn Clipboard, keys: &mut dyn KeySender, os: Os) {
    let modifier = [primary_modifier(os)];

    if let Some(rest) = text.strip_prefix(PLACEHOLDER_LAST) {
        // `@last msg`: open the whisper reply first, then paste `msg`.
        let rest: String = rest.chars().skip(1).collect();
        clipboard.write_text(&rest);
        keys.key_tap("Enter", &modifier);
    } else if let Some(rest) = text.strip_suffix(PLACEHOLDER_LAST) {
        // `msg @last`: open the reply, jump to the start of the prefilled name and
        // delete it, then the paste below fills in `msg`.
        clipboard.write_text(rest);
        keys.key_tap("Enter", &modifier);
        keys.key_tap("Home", &[]);
        // Twice, to focus the input when using a controller.
        keys.key_tap("Home", &[]);
        keys.key_tap("Delete", &[]);
    } else {
        clipboard.write_text(text);
        keys.key_tap("Enter", &[]);
        // A channel prefix of its own (`/`, `#`, ...) is typed over; otherwise the
        // chat input has to be cleared first.
        if !text.chars().next().is_some_and(|first| AUTO_CLEAR.contains(&first)) {
            keys.key_tap("A", &modifier);
        }
    }

    keys.key_tap("V", &modifier);

    if send {
        keys.key_tap("Enter", &[]);
        // Put the last chat back into the input and close it.
        keys.key_tap("Enter", &[]);
        keys.key_tap("ArrowUp", &[]);
        keys.key_tap("ArrowUp", &[]);
        keys.key_tap("Escape", &[]);
    }
}

/// `stashSearch`: focus the game, then Ctrl+F, paste, Enter.
pub fn stash_search(text: &str, clipboard: &mut dyn Clipboard, keys: &mut dyn KeySender, os: Os) {
    clipboard.write_text(text);
    keys.key_tap("F", &["Ctrl"]);
    keys.key_tap("V", &[primary_modifier(os)]);
    keys.key_tap("Enter", &[]);
}

/// The modifier that opens the chat (`Meta` is macOS' Command).
const fn primary_modifier(os: Os) -> &'static str {
    match os {
        Os::MacOs => "Meta",
        Os::Windows | Os::Linux => "Ctrl",
    }
}

/// `isStashArea`: is the cursor over the stash's item grid? Ctrl+wheel there
/// zooms the game's camera, so it is left alone; outside it, it changes tabs.
pub fn is_stash_area(mouse: Point, window: &GameWindow) -> bool {
    let (Some(bounds), Some(sidebar_width)) = (window.bounds(), window.ui_sidebar_width()) else {
        return false;
    };
    if mouse.x > bounds.x + sidebar_width {
        return false;
    }
    // 154 / 1600 and 1192 / 1600 of the window's height, the stash panel's band.
    mouse.y > bounds.y + bounds.height * 154.0 / 1600.0 && mouse.y < bounds.y + bounds.height * 1192.0 / 1600.0
}

/// One wheel notch, from the mouse hook.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WheelEvent {
    /// Where the cursor was.
    pub point: Point,
    /// Positive is away from the user (`uiohook-napi`'s `rotation`).
    pub rotation: i32,
    pub ctrl: bool,
}

/// One key event, from the keyboard hook. The key's name is `uiohook-napi`'s
/// (`A`, `Enter`, `ArrowUp`, `Ctrl`, ...), which is also what [`KeySender`] takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputEvent {
    /// `None` for a key the hook has no name for (`not_supported_key`).
    pub key: Option<&'static str>,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// `eventToString`: the hotkey string for the log, with the modifiers first.
pub fn event_to_string(event: &InputEvent) -> String {
    let Some(key) = event.key else {
        return "not_supported_key".to_owned();
    };
    // A modifier on its own is logged as itself.
    if is_mod_key(key) {
        return key.to_owned();
    }

    let prefix = match (event.ctrl, event.shift, event.alt) {
        (true, true, true) => "Ctrl + Shift + Alt",
        (_, true, true) => "Shift + Alt",
        (true, true, false) => "Ctrl + Shift",
        (true, false, true) => "Ctrl + Alt",
        (false, false, true) => "Alt",
        (true, false, false) => "Ctrl",
        (false, true, false) => "Shift",
        (false, false, false) => "",
    };
    if prefix.is_empty() { key.to_owned() } else { format!("{prefix} + {key}") }
}

/// `shortcutToElectron`: `Ctrl + Shift + C` becomes `Ctrl+Shift+C`, the
/// accelerator `globalShortcut.register` takes.
///
/// A name `KeyToElectron` does not know joins as nothing, so `Ctrl + Foo` becomes
/// `Ctrl+` — exactly what upstream passes to Electron (whose `join` drops
/// `undefined`), and why the host must reject an accelerator with an empty part
/// rather than register it.
pub fn accelerator(shortcut: &str) -> String {
    shortcut.split(" + ").map(|key| key_to_electron(key).unwrap_or_default()).collect::<Vec<_>>().join("+")
}

/// `KeyToElectron[key]`. Its key names are `KeyToCode`'s, so anything
/// [`key_to_code`] knows has an accelerator; the punctuation and navigation keys
/// have their own names.
pub fn key_to_electron(key: &str) -> Option<String> {
    let mapped = match key {
        "ArrowLeft" => "Left",
        "ArrowUp" => "Up",
        "ArrowRight" => "Right",
        "ArrowDown" => "Down",
        "Numpad0" => "num0",
        "Numpad1" => "num1",
        "Numpad2" => "num2",
        "Numpad3" => "num3",
        "Numpad4" => "num4",
        "Numpad5" => "num5",
        "Numpad6" => "num6",
        "Numpad7" => "num7",
        "Numpad8" => "num8",
        "Numpad9" => "num9",
        "NumpadMultiply" => "nummult",
        "NumpadAdd" => "numadd",
        "NumpadSubtract" => "numsub",
        "NumpadDecimal" => "numdec",
        "NumpadDivide" => "numdiv",
        "Semicolon" => ";",
        "Equal" => "=",
        "Comma" => ",",
        "Minus" => "-",
        "Period" => ".",
        "Slash" => "/",
        "Backquote" => "`",
        "BracketLeft" => "[",
        "Backslash" => "\\",
        "BracketRight" => "]",
        "Quote" => "'",
        other => return key_to_code(other).map(|_| other.to_owned()),
    };
    Some(mapped.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeClipboard, FakeClock, FakeKeySender, FakeOverlay, KeyStep, OverlayCall};
    use crate::window::{Bounds, GameWindowEvents};

    fn entry(shortcut: &str, action: Action, keep_mod_keys: bool) -> ShortcutAction {
        ShortcutAction { shortcut: shortcut.to_owned(), keep_mod_keys, action }
    }

    fn copy_item(shortcut: &str, keep_mod_keys: bool) -> ShortcutAction {
        entry(shortcut, Action::CopyItem { target: "price-check".into(), focus_overlay: false }, keep_mod_keys)
    }

    fn options(show_mods_key: &str) -> UpdateOptions {
        UpdateOptions {
            stash_scroll: true,
            log_keys: false,
            restore_clipboard: false,
            show_mods_key: show_mods_key.to_owned(),
            initial_delay: None,
        }
    }

    fn windows(game: GameId) -> Shortcuts {
        Shortcuts::new(game, Os::Windows)
    }

    /// Everything one press touched, for the assertions.
    struct Pressed {
        keys: FakeKeySender,
        clipboard: FakeClipboard,
        clock: FakeClock,
        handle: FakeOverlay,
        events: Vec<ShortcutEvent>,
    }

    fn press(
        shortcuts: &mut Shortcuts,
        entry: &ShortcutAction,
        show_mods_key: &str,
        clipboard: FakeClipboard,
    ) -> Pressed {
        let mut keys = FakeKeySender::default();
        let mut clipboard = clipboard;
        let clock = FakeClock::default();
        let mut window = GameWindow::new();
        window.set_active(true);
        let mut controller = OverlayController::new(shortcuts.game, shortcuts.os);
        let mut handle = FakeOverlay::default();
        let mut events = shortcuts.press(
            entry,
            PressContext {
                keys: &mut keys,
                system_clipboard: &mut clipboard,
                clock: &clock,
                window: &mut window,
                overlay: &mut controller,
                overlay_handle: &mut handle,
                cursor: Point { x: 400.0, y: 300.0 },
                show_mods_key,
            },
        );
        while let Some(due) = shortcuts.clipboard.capture_deadline() {
            clock.advance(due.saturating_sub(clock.now()));
            if let Some(Ok(text)) = shortcuts.clipboard.poll_capture(&mut clipboard, clock.now())
                && let Some(event) = Shortcuts::finish_copy(
                    entry,
                    Point { x: 400., y: 300. },
                    text,
                    OverlayContext { overlay: &mut controller, window: &mut window, handle: &mut handle },
                )
            {
                events.push(event);
            }
        }
        Pressed { keys, clipboard, clock, handle, events }
    }

    #[test]
    fn accelerators_are_electron_ones() {
        assert_eq!(accelerator("Ctrl + Shift + C"), "Ctrl+Shift+C");
        assert_eq!(accelerator("F8"), "F8");
        assert_eq!(accelerator("Ctrl + F2"), "Ctrl+F2");
        assert_eq!(accelerator("Ctrl + 1"), "Ctrl+1");
        assert_eq!(accelerator("ArrowUp"), "Up");
        assert_eq!(accelerator("Numpad1"), "num1");
        assert_eq!(accelerator("Semicolon"), ";");
        // `KeyToElectron['Foo']` is `undefined`, which `join` drops.
        assert_eq!(accelerator("Ctrl + Foo"), "Ctrl+");
    }

    #[test]
    fn every_key_name_has_an_accelerator() {
        for (name, _) in wraex_core::keys::KEY_TO_CODE {
            assert!(key_to_electron(name).is_some(), "{name} has no accelerator");
        }
    }

    #[test]
    fn every_configured_hotkey_is_registered() {
        let mut shortcuts = windows(GameId::Poe1);
        let warnings = shortcuts.update_actions(
            vec![
                copy_item("Ctrl + D", true),
                entry("Shift + Space", Action::ToggleOverlay, true),
                entry("F5", Action::PasteInChat { text: "/hideout".into(), send: true }, false),
            ],
            &options("Alt"),
        );
        assert_eq!(warnings, vec![]);
        let accelerators: Vec<String> =
            shortcuts.registered_shortcuts().into_iter().map(|registered| registered.accelerator).collect();
        assert_eq!(accelerators, vec!["Ctrl+D", "Shift+Space", "F5"]);
        assert!(!shortcuts.registered_shortcuts()[0].unregister_immediately);
    }

    #[test]
    fn the_games_own_hotkeys_are_never_registered() {
        let mut shortcuts = windows(GameId::Poe1);
        let warnings = shortcuts.update_actions(
            vec![
                copy_item("Ctrl + C", true),
                entry("Ctrl + F", Action::CopyItem { target: "search-similar".into(), focus_overlay: false }, false),
                entry("Enter", Action::PasteInChat { text: "x".into(), send: true }, false),
                copy_item("Ctrl + D", true),
                entry("Ctrl + E", Action::TestOnly, false),
            ],
            &options("Alt"),
        );
        assert_eq!(
            warnings,
            vec![
                ShortcutWarning::Reserved { shortcut: "Ctrl + C".into() },
                ShortcutWarning::Reserved { shortcut: "Ctrl + F".into() },
                ShortcutWarning::Reserved { shortcut: "Enter".into() },
            ],
            "one warning per reserved shortcut; `test-only` only proves the key"
        );
        assert_eq!(shortcuts.actions(), &[copy_item("Ctrl + D", true), entry("Ctrl + E", Action::TestOnly, false)]);

        // A `test-only` entry is registered and immediately unregistered again.
        let mut shortcuts = windows(GameId::Poe1);
        shortcuts.update_actions(vec![entry("Ctrl + E", Action::TestOnly, false)], &options("Alt"));
        let registered = shortcuts.registered_shortcuts();
        assert_eq!(registered.len(), 1);
        assert!(registered[0].unregister_immediately);
    }

    #[test]
    fn poe1_ignores_the_games_show_mods_key_but_poe2_reserves_the_copy_shortcut() {
        let bind_copy = || copy_item("Ctrl + Alt + C", false);

        // APT forces Ctrl, so `Ctrl + Alt + C` is nobody's hotkey.
        let mut poe1 = windows(GameId::Poe1);
        assert_eq!(poe1.update_actions(vec![bind_copy()], &options("Alt")), vec![]);
        assert_eq!(poe1.actions().len(), 1);

        // EE2 merges the game's Alt and takes `Ctrl + Alt + C` for its copy.
        let mut poe2 = windows(GameId::Poe2);
        assert_eq!(
            poe2.update_actions(vec![bind_copy()], &options("Alt")),
            vec![ShortcutWarning::Reserved { shortcut: "Ctrl + Alt + C".into() }]
        );
        assert_eq!(poe2.actions(), []);

        // With the game's key on Ctrl there is nothing extra to reserve.
        let mut poe2 = windows(GameId::Poe2);
        assert_eq!(poe2.update_actions(vec![bind_copy()], &options("Ctrl")), vec![]);
        assert_eq!(poe2.actions().len(), 1);
    }

    #[test]
    fn registration_follows_the_games_focus() {
        let mut shortcuts = windows(GameId::Poe1);
        assert!(!shortcuts.is_registered());
        assert_eq!(shortcuts.on_game_active(true), Registration::Register);
        assert!(shortcuts.is_registered());
        assert_eq!(shortcuts.on_game_active(true), Registration::None);
        assert_eq!(shortcuts.on_game_active(false), Registration::UnregisterAll);
        assert!(!shortcuts.is_registered());
        assert_eq!(shortcuts.on_game_active(false), Registration::None);
    }

    #[test]
    fn copying_an_item_presses_ctrl_c_and_sends_the_text() {
        let mut shortcuts = windows(GameId::Poe1);
        shortcuts.clipboard_mut().update_options(true);
        let pressed = press(
            &mut shortcuts,
            &copy_item("Ctrl + D", true),
            "Alt",
            FakeClipboard::scripted(&["my password", "Item Class: Rings\nRarity: Rare"]),
        );

        // Release the letter the user held but keep Ctrl held (upstream does not
        // toggle a key that is already down on Windows), so C alone copies.
        assert_eq!(
            pressed.keys.steps(),
            vec![KeyStep::Up("D".into()), KeyStep::Tap { key: "C".into(), modifiers: vec![] }]
        );
        assert_eq!(
            pressed.events,
            vec![ShortcutEvent::ItemText(ItemText {
                target: "price-check".into(),
                clipboard: "Item Class: Rings\nRarity: Rare".into(),
                position: Point { x: 400.0, y: 300.0 },
                focus_overlay: false,
            })]
        );
        assert_eq!(pressed.clipboard.written(), vec!["my password"]);
        assert!(pressed.handle.calls().is_empty(), "no focus change without `focusOverlay`");
    }

    #[test]
    fn a_copy_without_keep_mod_keys_releases_everything_and_presses_ctrl_again() {
        let mut shortcuts = windows(GameId::Poe1);
        let pressed = press(
            &mut shortcuts,
            &copy_item("Ctrl + Alt + D", false),
            "Alt",
            FakeClipboard::scripted(&["", "Item Class: Rings\n"]),
        );
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("D".into()),
                KeyStep::Up("Alt".into()),
                KeyStep::Up("Ctrl".into()),
                KeyStep::Down("Ctrl".into()),
                KeyStep::Tap { key: "C".into(), modifiers: vec![] },
                KeyStep::Up("Ctrl".into()),
            ]
        );
    }

    #[test]
    fn a_shortcut_without_keep_mod_keys_releases_every_key_in_reverse() {
        let mut shortcuts = windows(GameId::Poe1);
        let pressed = press(
            &mut shortcuts,
            &entry("Ctrl + Alt + D", Action::TriggerEvent { target: "delve-grid".into() }, false),
            "Alt",
            FakeClipboard::scripted(&[""]),
        );
        assert_eq!(
            pressed.keys.steps(),
            vec![KeyStep::Up("D".into()), KeyStep::Up("Alt".into()), KeyStep::Up("Ctrl".into())]
        );
        assert_eq!(pressed.events, vec![ShortcutEvent::WidgetAction { target: "delve-grid".into() }]);
    }

    #[test]
    fn ee2_copies_with_the_games_advanced_description_key_and_waits_before_releasing() {
        let mut shortcuts = windows(GameId::Poe2);
        let pressed = press(
            &mut shortcuts,
            &copy_item("Alt + A", true),
            "Alt",
            FakeClipboard::scripted(&["", "Item Class: Rings\n"]),
        );
        // The user holds Alt, the game's key is Alt, so only Ctrl is new.
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("A".into()),
                KeyStep::Down("Ctrl".into()),
                KeyStep::Tap { key: "C".into(), modifiers: vec![] },
                KeyStep::Sleep(EE2_KEY_RELEASE_DELAY),
                KeyStep::Up("Ctrl".into()),
            ]
        );
    }

    #[test]
    fn on_mac_the_keys_the_user_holds_are_toggled_too() {
        let mut shortcuts = Shortcuts::new(GameId::Poe2, Os::MacOs);
        let pressed = press(
            &mut shortcuts,
            &copy_item("Alt + A", true),
            "Alt",
            FakeClipboard::scripted(&["", "Item Class: Rings\n"]),
        );
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("A".into()),
                KeyStep::Down("Ctrl".into()),
                KeyStep::Down("Alt".into()),
                KeyStep::Tap { key: "C".into(), modifiers: vec![] },
                KeyStep::Sleep(EE2_KEY_RELEASE_DELAY),
                KeyStep::Up("Alt".into()),
                KeyStep::Up("Ctrl".into()),
            ]
        );
    }

    #[test]
    fn slow_capture_allows_another_action_and_times_out_without_a_second_copy() {
        let mut shortcuts = windows(GameId::Poe1);
        shortcuts.clipboard.update_options(true);
        let clock = FakeClock::default();
        let mut keys = FakeKeySender::default();
        let mut clipboard = FakeClipboard::scripted(&["old", ""]);
        let mut window = GameWindow::new();
        let mut controller = OverlayController::new(GameId::Poe1, Os::Windows);
        let mut handle = FakeOverlay::default();
        let mut press = |entry: &ShortcutAction| {
            shortcuts.press(
                entry,
                PressContext {
                    keys: &mut keys,
                    system_clipboard: &mut clipboard,
                    clock: &clock,
                    window: &mut window,
                    overlay: &mut controller,
                    overlay_handle: &mut handle,
                    cursor: Point { x: 0., y: 0. },
                    show_mods_key: "Alt",
                },
            )
        };
        let copy = copy_item("Ctrl + D", true);
        assert_eq!(press(&copy), []);
        assert_eq!(press(&copy), []);
        let other = entry("F1", Action::TriggerEvent { target: "delve-grid".into() }, false);
        assert_eq!(press(&other), vec![ShortcutEvent::WidgetAction { target: "delve-grid".into() }]);
        assert_eq!(clock.now(), 0);
        assert_eq!(keys.steps().iter().filter(|step| matches!(step, KeyStep::Tap {key, ..} if key == "C")).count(), 1);
        assert_eq!(
            shortcuts.clipboard.poll_capture(&mut clipboard, 1000),
            Some(Err(crate::clipboard::ReadError::Timeout))
        );
        assert!(shortcuts.clipboard.capture_deadline().is_none());
        assert_eq!(clipboard.written(), ["old"]);
    }

    #[test]
    fn a_focused_copy_opens_the_overlay_only_if_it_was_used_recently() {
        let entry = entry("Ctrl + D", Action::CopyItem { target: "item-check".into(), focus_overlay: true }, true);

        let mut shortcuts = windows(GameId::Poe1);
        let pressed = press(&mut shortcuts, &entry, "Alt", FakeClipboard::scripted(&["", "Item Class: Rings\n"]));
        assert_eq!(pressed.events.len(), 1);
        assert_eq!(
            pressed.handle.calls(),
            vec![OverlayCall::Show, OverlayCall::ClickThrough(false)],
            "`wasUsedRecently` starts true: the widget takes the focus"
        );

        // After the user used the game, the widget must not steal the focus.
        let mut shortcuts = windows(GameId::Poe1);
        let mut keys = FakeKeySender::default();
        let mut clipboard = FakeClipboard::scripted(&["", "Item Class: Rings\n"]);
        let clock = FakeClock::default();
        let mut window = GameWindow::new();
        window.set_active(true);
        let mut controller = OverlayController::new(GameId::Poe1, Os::Windows);
        controller.set_was_used_recently(false);
        let mut handle = FakeOverlay::default();
        shortcuts.press(
            &entry,
            PressContext {
                keys: &mut keys,
                system_clipboard: &mut clipboard,
                clock: &clock,
                window: &mut window,
                overlay: &mut controller,
                overlay_handle: &mut handle,
                cursor: Point { x: 1.0, y: 2.0 },
                show_mods_key: "Alt",
            },
        );
        assert!(handle.calls().is_empty(), "no `activateOverlay`");
        assert!(!controller.is_interactable());
    }

    #[test]
    fn typing_in_chat_goes_through_the_clipboard_and_sends() {
        let mut shortcuts = windows(GameId::Poe1);
        let chat = entry("F5", Action::PasteInChat { text: "hello".into(), send: true }, false);
        let pressed = press(&mut shortcuts, &chat, "Alt", FakeClipboard::scripted(&["old clipboard"]));
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("F5".into()),
                KeyStep::Tap { key: "Enter".into(), modifiers: vec![] },
                KeyStep::Tap { key: "A".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "V".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "Enter".into(), modifiers: vec![] },
                KeyStep::Tap { key: "Enter".into(), modifiers: vec![] },
                KeyStep::Tap { key: "ArrowUp".into(), modifiers: vec![] },
                KeyStep::Tap { key: "ArrowUp".into(), modifiers: vec![] },
                KeyStep::Tap { key: "Escape".into(), modifiers: vec![] },
            ]
        );
        assert_eq!(pressed.clipboard.written(), vec!["hello"]);
        // The previous clipboard comes back 120 ms later.
        assert_eq!(pressed.clock.elapsed(), 0);
        assert_eq!(shortcuts.clipboard_mut().pending_restore_deadline(), Some(crate::clipboard::RESTORE_AFTER));
    }

    #[test]
    fn a_channel_prefix_is_not_cleared_first() {
        let mut shortcuts = windows(GameId::Poe2);
        let chat = entry("F5", Action::PasteInChat { text: "#global hi".into(), send: false }, false);
        let pressed = press(&mut shortcuts, &chat, "Alt", FakeClipboard::scripted(&[""]));
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("F5".into()),
                KeyStep::Tap { key: "Enter".into(), modifiers: vec![] },
                KeyStep::Tap { key: "V".into(), modifiers: vec!["Ctrl".into()] },
            ]
        );

        // `/hideout` is a command: also typed over (a fresh hotkey: the first
        // press's restore is still throttling).
        let command = entry("F5", Action::PasteInChat { text: "/hideout".into(), send: false }, false);
        let mut shortcuts = windows(GameId::Poe2);
        let pressed = press(&mut shortcuts, &command, "Alt", FakeClipboard::scripted(&[""]));
        assert_eq!(pressed.keys.steps().len(), 3, "the release, then Enter and paste");
    }

    #[test]
    fn on_mac_the_chat_uses_command() {
        let mut shortcuts = Shortcuts::new(GameId::Poe2, Os::MacOs);
        let chat = entry("F5", Action::PasteInChat { text: "/hideout".into(), send: false }, false);
        let pressed = press(&mut shortcuts, &chat, "Alt", FakeClipboard::scripted(&[""]));
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("F5".into()),
                KeyStep::Tap { key: "Enter".into(), modifiers: vec![] },
                KeyStep::Tap { key: "V".into(), modifiers: vec!["Meta".into()] },
            ]
        );
    }

    #[test]
    fn an_at_last_prefix_opens_the_whisper_before_pasting() {
        let mut shortcuts = windows(GameId::Poe1);
        let reply = entry("F5", Action::PasteInChat { text: "@last ty".into(), send: false }, false);
        let pressed = press(&mut shortcuts, &reply, "Alt", FakeClipboard::scripted(&[""]));
        assert_eq!(pressed.clipboard.written(), vec!["ty"], "`@last ` is dropped");
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("F5".into()),
                KeyStep::Tap { key: "Enter".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "V".into(), modifiers: vec!["Ctrl".into()] },
            ]
        );
    }

    #[test]
    fn an_at_last_suffix_opens_the_whisper_and_clears_the_name_first() {
        let mut shortcuts = windows(GameId::Poe1);
        let tail = entry("F5", Action::PasteInChat { text: "ty @last".into(), send: false }, false);
        let pressed = press(&mut shortcuts, &tail, "Alt", FakeClipboard::scripted(&[""]));
        assert_eq!(pressed.clipboard.written(), vec!["ty "], "`@last` is dropped");
        assert_eq!(
            pressed.keys.steps(),
            vec![
                KeyStep::Up("F5".into()),
                KeyStep::Tap { key: "Enter".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "Home".into(), modifiers: vec![] },
                KeyStep::Tap { key: "Home".into(), modifiers: vec![] },
                KeyStep::Tap { key: "Delete".into(), modifiers: vec![] },
                KeyStep::Tap { key: "V".into(), modifiers: vec!["Ctrl".into()] },
            ]
        );
    }

    #[test]
    fn a_second_chat_hotkey_inside_the_restore_window_sends_nothing() {
        let mut shortcuts = windows(GameId::Poe1);
        let chat = entry("F5", Action::PasteInChat { text: "/hideout".into(), send: true }, false);
        let mut keys = FakeKeySender::default();
        let mut clipboard = FakeClipboard::scripted(&["old"]);
        let clock = FakeClock::default();
        let mut window = GameWindow::new();
        let mut controller = OverlayController::new(GameId::Poe1, Os::Windows);
        let mut handle = FakeOverlay::default();

        let mut press_chat = |keys: &mut FakeKeySender| {
            shortcuts.press(
                &chat,
                PressContext {
                    keys,
                    system_clipboard: &mut clipboard,
                    clock: &clock,
                    window: &mut window,
                    overlay: &mut controller,
                    overlay_handle: &mut handle,
                    cursor: Point { x: 0.0, y: 0.0 },
                    show_mods_key: "Alt",
                },
            );
        };
        press_chat(&mut keys);
        let after_first = keys.steps().len();
        press_chat(&mut keys);
        // The keys the user held are released first (upstream does that before the
        // action too), then the throttle stops the typing.
        let steps = keys.steps();
        assert_eq!(steps.len(), after_first + 1);
        assert_eq!(steps.last(), Some(&KeyStep::Up("F5".into())));
        assert_eq!(clipboard.written(), vec!["/hideout"]);
    }

    #[test]
    fn a_chat_then_stash_press_after_the_restore_deadline_both_paste() {
        for restore in [false, true] {
            let mut shortcuts = windows(GameId::Poe1);
            shortcuts.clipboard_mut().update_options(restore);
            let mut keys = FakeKeySender::default();
            let mut clipboard = FakeClipboard::scripted(&["old"]);
            let clock = FakeClock::default();
            let mut window = GameWindow::new();
            let mut controller = OverlayController::new(GameId::Poe1, Os::Windows);
            let mut handle = FakeOverlay::default();
            for action in [
                Action::PasteInChat { text: "/hideout".into(), send: true },
                Action::StashSearch { text: "maps".into() },
            ] {
                shortcuts.press(
                    &entry("F5", action, false),
                    PressContext {
                        keys: &mut keys,
                        system_clipboard: &mut clipboard,
                        clock: &clock,
                        window: &mut window,
                        overlay: &mut controller,
                        overlay_handle: &mut handle,
                        cursor: Point { x: 0., y: 0. },
                        show_mods_key: "Alt",
                    },
                );
                clock.advance(crate::clipboard::RESTORE_AFTER + 1);
            }
            assert!(shortcuts.clipboard_mut().apply_pending_restore(&mut clipboard, &clock));
            let expected = if restore { vec!["/hideout", "old", "maps", "old"] } else { vec!["/hideout", "maps"] };
            assert_eq!(clipboard.written(), expected);
        }
    }

    #[test]
    fn the_stash_search_focuses_the_game_then_pastes_and_searches() {
        let mut shortcuts = windows(GameId::Poe1);
        let search = entry("Ctrl + 1", Action::StashSearch { text: "\"Pack Size: +3\"".into() }, false);
        let mut keys = FakeKeySender::default();
        let mut clipboard = FakeClipboard::scripted(&["old"]);
        let clock = FakeClock::default();
        let mut window = GameWindow::new();
        // The overlay is open, so the search has to hand the game its focus back.
        let mut controller = OverlayController::new(GameId::Poe1, Os::Windows);
        let mut handle = FakeOverlay::default();
        window.set_active(true);
        controller.assert_overlay_active(&mut window, &mut handle);
        handle.clear();

        let events = shortcuts.press(
            &search,
            PressContext {
                keys: &mut keys,
                system_clipboard: &mut clipboard,
                clock: &clock,
                window: &mut window,
                overlay: &mut controller,
                overlay_handle: &mut handle,
                cursor: Point { x: 0.0, y: 0.0 },
                show_mods_key: "Alt",
            },
        );

        assert!(events.is_empty(), "the stash search only types");
        assert_eq!(
            keys.steps(),
            vec![
                KeyStep::Up("1".into()),
                KeyStep::Up("Ctrl".into()),
                KeyStep::Tap { key: "F".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "V".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "Enter".into(), modifiers: vec![] },
            ]
        );
        assert_eq!(clipboard.written(), vec!["\"Pack Size: +3\""]);
        assert!(window.is_active(), "the game got its focus back before typing");
        assert!(!controller.is_interactable());
        assert_eq!(handle.calls(), vec![OverlayCall::ClickThrough(true), OverlayCall::FocusGame]);
    }

    #[test]
    fn the_stash_search_widget_sends_the_same_sequence_without_releasing_keys() {
        let mut shortcuts = windows(GameId::Poe1);
        let mut keys = FakeKeySender::default();
        let mut clipboard = FakeClipboard::scripted(&["old"]);
        let clock = FakeClock::default();
        let mut window = GameWindow::new();
        window.set_active(true);
        let mut controller = OverlayController::new(GameId::Poe1, Os::Windows);
        let mut handle = FakeOverlay::default();

        // `CLIENT->MAIN::user-action`'s `stash-search`: no hotkey, so no release.
        shortcuts.user_stash_search(
            "\"Pack Size: +3\"",
            PressContext {
                keys: &mut keys,
                system_clipboard: &mut clipboard,
                clock: &clock,
                window: &mut window,
                overlay: &mut controller,
                overlay_handle: &mut handle,
                cursor: Point { x: 0.0, y: 0.0 },
                show_mods_key: "Alt",
            },
        );
        assert_eq!(
            keys.steps(),
            vec![
                KeyStep::Tap { key: "F".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "V".into(), modifiers: vec!["Ctrl".into()] },
                KeyStep::Tap { key: "Enter".into(), modifiers: vec![] },
            ]
        );
        assert_eq!(clipboard.written(), vec!["\"Pack Size: +3\""]);
    }

    #[test]
    fn toggling_the_overlay_flips_it_and_stops_tracking_the_widget_area() {
        let mut shortcuts = windows(GameId::Poe1);
        let toggle = entry("Shift + Space", Action::ToggleOverlay, true);
        let mut keys = FakeKeySender::default();
        let mut clipboard = FakeClipboard::scripted(&[""]);
        let clock = FakeClock::default();
        let mut window = GameWindow::new();
        window.set_active(true);
        let mut controller = OverlayController::new(GameId::Poe1, Os::Windows);
        let mut handle = FakeOverlay::default();

        shortcuts.press(
            &toggle,
            PressContext {
                keys: &mut keys,
                system_clipboard: &mut clipboard,
                clock: &clock,
                window: &mut window,
                overlay: &mut controller,
                overlay_handle: &mut handle,
                cursor: Point { x: 0.0, y: 0.0 },
                show_mods_key: "Alt",
            },
        );
        assert!(controller.is_interactable());
        assert!(!window.is_active(), "the overlay took the focus");
        assert_eq!(keys.steps(), vec![KeyStep::Up("Space".into())], "only the letter is released");
        assert_eq!(handle.calls(), vec![OverlayCall::Show, OverlayCall::ClickThrough(false)]);
    }

    #[test]
    fn the_heist_gem_ocr_runs_only_on_windows_and_only_for_its_target() {
        let mut shortcuts = windows(GameId::Poe1);
        let gems = entry("Ctrl + O", Action::OcrText { target: "heist-gems".into() }, true);
        let pressed = press(&mut shortcuts, &gems, "Alt", FakeClipboard::scripted(&[""]));
        assert_eq!(pressed.events, vec![ShortcutEvent::OcrRequest { target: "heist-gems".into(), press_time: 0 }]);

        let mut shortcuts = Shortcuts::new(GameId::Poe1, Os::Linux);
        let pressed = press(&mut shortcuts, &gems, "Alt", FakeClipboard::scripted(&[""]));
        assert!(pressed.events.is_empty(), "`process.platform !== 'win32'`");

        let mut shortcuts = windows(GameId::Poe1);
        let other = entry("Ctrl + O", Action::OcrText { target: "something".into() }, true);
        let pressed = press(&mut shortcuts, &other, "Alt", FakeClipboard::scripted(&[""]));
        assert_eq!(pressed.events, []);
    }

    #[test]
    fn ctrl_wheel_outside_the_stash_grid_changes_tabs() {
        let mut shortcuts = windows(GameId::Poe1);
        shortcuts.update_actions(vec![], &options("Alt"));
        let mut window = GameWindow::new();
        window.feed(GameWindowEvents::Bounds(Bounds { x: 0.0, y: 0.0, width: 1600.0, height: 1200.0 }));
        window.set_active(true);

        // Above the stash panel's band (154/1600 of the height), left of the UI
        // sidebar.
        let mut keys = FakeKeySender::default();
        let wheel = WheelEvent { point: Point { x: 100.0, y: 10.0 }, rotation: 1, ctrl: true };
        assert!(shortcuts.on_wheel(&wheel, &window, &mut keys));
        assert_eq!(keys.steps(), vec![KeyStep::Tap { key: "ArrowRight".into(), modifiers: vec![] }]);

        let mut keys = FakeKeySender::default();
        let wheel = WheelEvent { point: Point { x: 100.0, y: 10.0 }, rotation: -1, ctrl: true };
        assert!(shortcuts.on_wheel(&wheel, &window, &mut keys));
        assert_eq!(keys.steps(), vec![KeyStep::Tap { key: "ArrowLeft".into(), modifiers: vec![] }]);

        // Over the stash grid the game zooms instead.
        let mut keys = FakeKeySender::default();
        let wheel = WheelEvent { point: Point { x: 700.0, y: 600.0 }, rotation: 1, ctrl: true };
        assert!(!shortcuts.on_wheel(&wheel, &window, &mut keys));
        assert_eq!(keys.steps(), []);
    }

    #[test]
    fn the_wheel_needs_ctrl_an_active_game_and_the_setting() {
        let mut shortcuts = windows(GameId::Poe1);
        let mut window = GameWindow::new();
        let wheel = WheelEvent { point: Point { x: 10.0, y: 10.0 }, rotation: 1, ctrl: true };
        let mut keys = FakeKeySender::default();

        // Stash scrolling off: `updateActions` never ran.
        assert!(!shortcuts.on_wheel(&wheel, &window, &mut keys));
        shortcuts.update_actions(vec![], &options("Alt"));
        // The game is not focused.
        assert!(!shortcuts.on_wheel(&wheel, &window, &mut keys));
        window.set_active(true);
        // Upstream treats an unknown rectangle as "not the stash area", so the
        // wheel still changes tabs.
        assert!(shortcuts.on_wheel(&wheel, &window, &mut keys));
        // Without Ctrl the game keeps the wheel.
        let mut keys = FakeKeySender::default();
        let wheel = WheelEvent { ctrl: false, ..wheel };
        assert!(!shortcuts.on_wheel(&wheel, &window, &mut keys));
        assert_eq!(keys.steps(), []);
    }

    #[test]
    fn key_events_are_logged_like_upstream() {
        let mut shortcuts = windows(GameId::Poe1);
        assert_eq!(shortcuts.log_key_down(&InputEvent { key: Some("A"), ctrl: true, shift: false, alt: false }), None);

        shortcuts.update_actions(vec![], &UpdateOptions { log_keys: true, ..options("Alt") });
        let log = |event: InputEvent| shortcuts.log_key_down(&event);
        assert_eq!(
            log(InputEvent { key: Some("A"), ctrl: true, shift: false, alt: false }),
            Some("debug [Shortcuts] Keydown Ctrl + A".into())
        );
        assert_eq!(
            log(InputEvent { key: Some("K"), ctrl: true, shift: true, alt: true }),
            Some("debug [Shortcuts] Keydown Ctrl + Shift + Alt + K".into())
        );
        assert_eq!(
            log(InputEvent { key: Some("F5"), ctrl: false, shift: false, alt: true }),
            Some("debug [Shortcuts] Keydown Alt + F5".into())
        );
        // A modifier on its own, and a key uiohook has no name for.
        assert_eq!(
            log(InputEvent { key: Some("Alt"), ctrl: false, shift: false, alt: false }),
            Some("debug [Shortcuts] Keydown Alt".into())
        );
        assert_eq!(
            log(InputEvent { key: None, ctrl: false, shift: false, alt: false }),
            Some("debug [Shortcuts] Keydown not_supported_key".into())
        );
        assert_eq!(shortcuts.log_key_up(Some("A")), Some("debug [Shortcuts] Keyup A".into()));
        assert_eq!(shortcuts.log_key_up(None), Some("debug [Shortcuts] Keyup not_supported_key".into()));
        assert_eq!(
            shortcuts.log_action(&Action::CopyItem { target: "price-check".into(), focus_overlay: false }),
            Some("debug [Shortcuts] Action type: copy-item".into())
        );
    }

    #[test]
    fn the_host_configs_the_references_produce_are_accepted() {
        // `fixtures/config/<game>/<case>/host.json`: what `getConfigForHost`
        // records. Every one must deserialize, and every hotkey the game does not
        // hold must survive the filter, in its config order.
        let mut compared = 0;
        for game in GameId::ALL {
            for case in wraex_golden::config_cases(game) {
                compared += 1;
                let config: HostConfig = serde_json::from_value(case.host.clone())
                    .unwrap_or_else(|error| panic!("{}/{}/host.json: {error}", game, case.slug));
                let mut shortcuts = Shortcuts::new(game, Os::Windows);
                let warnings = shortcuts.apply_host_config(&config, "Alt");

                let mut reserved: Vec<String> =
                    RESERVED_SHORTCUTS.iter().map(|shortcut| (*shortcut).to_owned()).collect();
                if game == GameId::Poe2 {
                    reserved.push(merge_two_hotkeys("Ctrl + C", "Alt"));
                }
                let expected: Vec<&ShortcutAction> =
                    config.shortcuts.iter().filter(|action| !reserved.contains(&action.shortcut)).collect();
                assert_eq!(
                    shortcuts.actions().len(),
                    expected.len(),
                    "{}/{}/{} shortcuts dropped",
                    game,
                    case.slug,
                    config.shortcuts.len()
                );
                for (kept, expected) in shortcuts.actions().iter().zip(expected) {
                    assert_eq!(kept, expected);
                }
                let expected_warnings: Vec<ShortcutWarning> = config
                    .shortcuts
                    .iter()
                    .filter(|action| reserved.contains(&action.shortcut))
                    .map(|action| ShortcutWarning::Reserved { shortcut: action.shortcut.clone() })
                    .collect();
                assert_eq!(warnings, expected_warnings, "{}/{}", game, case.slug);

                // The config's own options reached the poller.
                assert_eq!(
                    shortcuts.clipboard_mut().should_restore(),
                    config.options.restore_clipboard,
                    "{}/{}",
                    game,
                    case.slug
                );
            }
        }
        assert!(compared >= 8, "the settings fixtures are missing ({compared} cases)");
    }
}
