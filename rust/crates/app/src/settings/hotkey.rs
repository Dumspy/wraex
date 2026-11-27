//! `web/settings/HotkeyInput.vue`, `SettingsHotkey.vue` and `ipc/KeyToCode.ts`: recording a
//! key combination into upstream's hotkey string.
//!
//! Upstream listens to an `<input>`'s `keyup`, reads `e.code` (the physical key), and formats
//! `ctrlKey`/`shiftKey`/`altKey` through `hotkeyToString` (`wraex_core::keys`). Rune's key
//! releases carry the physical key (`Tx::physical_key`) and the same three modifiers, so the
//! port records on release too: the window's root hears every release, which also covers Tab
//! (whose press moves focus before the window's own key handler runs).

use rune::{Align, El, Key, KeyEvent, Mods, PhysicalKeyCode, row, text};
use serde_json::Value;
use wraex_core::keys::{hotkey_to_string, key_to_code};

use crate::settings::{Field, Settings, t};
use crate::theme::{self, text as type_};

/// The field a release is being recorded into (`HotkeyInput.vue`'s props).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recording {
    pub field: Field,
    /// `required`: Backspace can't clear it.
    pub required: bool,
    /// `noModKeys`: the split key's second half takes no modifiers.
    pub no_mod_keys: bool,
}

/// `e.code`'s name in upstream's `KeyToCode`, for the keys that table holds.
#[expect(
    clippy::too_many_lines,
    reason = "line-for-line port of upstream's `KeyToCode` lookup table (ipc/KeyToCode.ts)"
)]
pub fn key_name(code: PhysicalKeyCode) -> Option<&'static str> {
    use PhysicalKeyCode as C;
    let name = match code {
        C::KeyA => "A",
        C::KeyB => "B",
        C::KeyC => "C",
        C::KeyD => "D",
        C::KeyE => "E",
        C::KeyF => "F",
        C::KeyG => "G",
        C::KeyH => "H",
        C::KeyI => "I",
        C::KeyJ => "J",
        C::KeyK => "K",
        C::KeyL => "L",
        C::KeyM => "M",
        C::KeyN => "N",
        C::KeyO => "O",
        C::KeyP => "P",
        C::KeyQ => "Q",
        C::KeyR => "R",
        C::KeyS => "S",
        C::KeyT => "T",
        C::KeyU => "U",
        C::KeyV => "V",
        C::KeyW => "W",
        C::KeyX => "X",
        C::KeyY => "Y",
        C::KeyZ => "Z",
        C::Digit0 => "0",
        C::Digit1 => "1",
        C::Digit2 => "2",
        C::Digit3 => "3",
        C::Digit4 => "4",
        C::Digit5 => "5",
        C::Digit6 => "6",
        C::Digit7 => "7",
        C::Digit8 => "8",
        C::Digit9 => "9",
        C::Backspace => "Backspace",
        C::Tab => "Tab",
        C::Enter => "Enter",
        C::CapsLock => "CapsLock",
        C::Escape => "Escape",
        C::Space => "Space",
        C::PageUp => "PageUp",
        C::PageDown => "PageDown",
        C::End => "End",
        C::Home => "Home",
        C::ArrowLeft => "ArrowLeft",
        C::ArrowUp => "ArrowUp",
        C::ArrowRight => "ArrowRight",
        C::ArrowDown => "ArrowDown",
        C::Insert => "Insert",
        C::Delete => "Delete",
        C::Numpad0 => "Numpad0",
        C::Numpad1 => "Numpad1",
        C::Numpad2 => "Numpad2",
        C::Numpad3 => "Numpad3",
        C::Numpad4 => "Numpad4",
        C::Numpad5 => "Numpad5",
        C::Numpad6 => "Numpad6",
        C::Numpad7 => "Numpad7",
        C::Numpad8 => "Numpad8",
        C::Numpad9 => "Numpad9",
        C::NumpadMultiply => "NumpadMultiply",
        C::NumpadAdd => "NumpadAdd",
        C::NumpadSubtract => "NumpadSubtract",
        C::NumpadDecimal => "NumpadDecimal",
        C::NumpadDivide => "NumpadDivide",
        C::F1 => "F1",
        C::F2 => "F2",
        C::F3 => "F3",
        C::F4 => "F4",
        C::F5 => "F5",
        C::F6 => "F6",
        C::F7 => "F7",
        C::F8 => "F8",
        C::F9 => "F9",
        C::F10 => "F10",
        C::F11 => "F11",
        C::F12 => "F12",
        C::F13 => "F13",
        C::F14 => "F14",
        C::F15 => "F15",
        C::F16 => "F16",
        C::F17 => "F17",
        C::F18 => "F18",
        C::F19 => "F19",
        C::F20 => "F20",
        C::F21 => "F21",
        C::F22 => "F22",
        C::F23 => "F23",
        C::F24 => "F24",
        C::Semicolon => "Semicolon",
        C::Equal => "Equal",
        C::Comma => "Comma",
        C::Minus => "Minus",
        C::Period => "Period",
        C::Slash => "Slash",
        C::Backquote => "Backquote",
        C::BracketLeft => "BracketLeft",
        C::Backslash => "Backslash",
        C::BracketRight => "BracketRight",
        C::Quote => "Quote",
        C::Pause => "Pause",
        _ => return None,
    };
    key_to_code(name).map(|_| name)
}

/// A release's key name when it has no physical code, named for the character it types.
///
/// A headless `Runtime::key_up`, or a script's `"key"` without a `"code"`, has no physical
/// code: the port names the character the way `KeyToCode` names the key that types it on a US
/// layout.
pub fn logical_name(key: &Key) -> Option<String> {
    let character = |c: char| match c.to_ascii_uppercase() {
        c @ ('A'..='Z' | '0'..='9') => Some(c.to_string()),
        ' ' => Some("Space".to_owned()),
        ';' => Some("Semicolon".to_owned()),
        '=' => Some("Equal".to_owned()),
        ',' => Some("Comma".to_owned()),
        '-' => Some("Minus".to_owned()),
        '.' => Some("Period".to_owned()),
        '/' => Some("Slash".to_owned()),
        '`' => Some("Backquote".to_owned()),
        '[' => Some("BracketLeft".to_owned()),
        '\\' => Some("Backslash".to_owned()),
        ']' => Some("BracketRight".to_owned()),
        '\'' => Some("Quote".to_owned()),
        _ => None,
    };
    match key {
        Key::Text(text) => {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => character(c),
                _ => None,
            }
        }
        Key::Command(c) => character(*c),
        Key::Enter => Some("Enter".to_owned()),
        Key::Backspace => Some("Backspace".to_owned()),
        Key::Delete => Some("Delete".to_owned()),
        Key::Left => Some("ArrowLeft".to_owned()),
        Key::Right => Some("ArrowRight".to_owned()),
        Key::Up => Some("ArrowUp".to_owned()),
        Key::Down => Some("ArrowDown".to_owned()),
        Key::Home => Some("Home".to_owned()),
        Key::End => Some("End".to_owned()),
        Key::PageUp => Some("PageUp".to_owned()),
        Key::PageDown => Some("PageDown".to_owned()),
        Key::Insert => Some("Insert".to_owned()),
        Key::Escape => Some("Escape".to_owned()),
        Key::Tab => Some("Tab".to_owned()),
        Key::Function(n) => Some(format!("F{n}")),
        _ => None,
    }
}

/// `HotkeyInput.vue`'s `handleKeyup`: the hotkey `ev` records, `Some(None)` when it clears the
/// field, and `None` when the release is no hotkey (a modifier alone, F12, an unknown key).
pub fn recorded(recording: Recording, ev: &KeyEvent, physical: Option<PhysicalKeyCode>) -> Option<Option<String>> {
    let name = match physical {
        Some(code) => key_name(code)?.to_owned(),
        None => logical_name(&ev.key)?,
    };
    // `if (e.code === 'Backspace')`: clear, unless the field is required.
    if name == "Backspace" {
        return (!recording.required).then_some(None);
    }
    // `if (code.includes('F12')) return`
    if name.contains("F12") {
        return None;
    }
    let hotkey = hotkey_string(&name, ev.mods);
    // `if (props.noModKeys && code.includes('+')) return`
    if recording.no_mod_keys && hotkey.contains('+') {
        return None;
    }
    Some(Some(hotkey))
}

/// `hotkeyToString([code], ctrlKey, shiftKey, altKey)`
pub fn hotkey_string(name: &str, mods: Mods) -> String {
    hotkey_to_string(&[name], mods.ctrl, mods.shift, mods.alt)
}

/// The window root's `on_key_up`: write the recorded combination into the working copy. True
/// when a field was capturing, so nothing else treats the release as a navigation key.
pub fn record(settings: &mut Settings, ev: &KeyEvent, physical: Option<PhysicalKeyCode>) -> bool {
    let Some(recording) = settings.recording else { return false };
    if matches!(ev.key, Key::Modifier { .. }) {
        return false;
    }
    let Some(hotkey) = recorded(recording, ev, physical) else { return true };
    match hotkey {
        Some(hotkey) => settings.set(recording.field, Value::String(hotkey)),
        None => settings.set(recording.field, Value::Null),
    }
    settings.recording = None;
    true
}

/// The width `HotkeyInput.vue` takes: `w-48` beside a label, `w-20` in the split key.
pub const WIDE: f32 = 12.0 * theme::REM;
pub const NARROW: f32 = 5.0 * theme::REM;

/// A [`hotkey_input`]: its element key, the hotkey (or the placeholder shown), its width and
/// whether it is capturing keys.
#[derive(Debug, Clone, Copy)]
pub struct HotkeyInput<'a> {
    /// The element key; the value text's key is `{key}-value`.
    pub key: &'a str,
    /// The hotkey, or `None` for the red `Not Set` placeholder.
    pub value: Option<&'a str>,
    /// `w-48` beside a label, `w-20` in the split key.
    pub width: f32,
    /// `t('settings.no_key')`, shown while no key is set.
    pub placeholder: &'a str,
    /// Whether this is the field the window is capturing keys for.
    pub recording: bool,
}

/// `HotkeyInput.vue`: `rounded bg-gray-900 px-1 text-center font-poe`, the hotkey or the red
/// `Not Set` placeholder.
pub fn hotkey_input<S: 'static>(props: HotkeyInput<'_>, on_start: impl Fn(&mut S) + 'static) -> El {
    let HotkeyInput { key, value, width, placeholder, recording } = props;
    let (label, color) =
        value.map_or_else(|| (placeholder.to_owned(), theme::red(400)), |value| (value.to_owned(), theme::gray(100)));
    row()
        .key(key)
        .w(width)
        .h(type_::SM.line_px())
        .px(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        // A mock `<input>` has no caret: the border marks the field that is capturing keys.
        .border(1.0, if recording { theme::gray(500) } else { rune::Color::TRANSPARENT })
        .border_layout()
        .justify_center()
        .center_y()
        .focusable(true)
        .role("textbox")
        .label(label.clone())
        .on_press(on_start)
        .child(
            text(label)
                .key(format!("{key}-value"))
                .font(type_::SM.family(theme::FAMILY_POE))
                .color(color)
                .align(Align::Center),
        )
}

/// The split key's modifier button (`Ctrl`, `Alt`): `bg-gray-900`, a 1 pt border that turns
/// currentColor while it is the held modifier, `line-height: 1`.
pub fn modifier_button<S: 'static>(key: &str, label: &str, active: bool, on_press: impl Fn(&mut S) + 'static) -> El {
    row()
        .key(key)
        .px(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        .border(1.0, if active { theme::gray(100) } else { rune::Color::TRANSPARENT })
        .border_layout()
        .center_y()
        .focusable(true)
        .role("button")
        .label(label.to_owned())
        .on_press(on_press)
        .child(text(label.to_owned()).key(format!("{key}-label")).font(theme::leading_none(type_::SM)))
}

/// `t('settings.no_key')` for the window's game.
pub fn no_key(game: wraex_core::GameId) -> String {
    t(game, "settings.no_key")
}
