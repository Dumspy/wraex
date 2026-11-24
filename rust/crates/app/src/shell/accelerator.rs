//! The app's hotkeys in the spelling Rune's global-shortcut API takes.
//!
//! The config (`HostConfig.shortcuts`) and [`wraex_platform::shortcuts`] use upstream's
//! names — `Ctrl + Alt + D`, `Shift + Space`, `Meta + C` — while
//! [`rune::Shortcut`] parses `ctrl+alt+d`. Only the spelling differs: both name physical
//! keys, so a hotkey the settings page records is the one the OS grabs.
//!
//! Key names are [`wraex_core::keys::KEY_TO_CODE`]'s, the same table the config's hotkey
//! recorder and the settings page use. A name Rune's parser does not know yields `None`
//! and the shell skips that one hotkey with a warning, as upstream's host skips an
//! accelerator Electron cannot register.

/// Rune's name for one key of an upstream hotkey.
fn rune_key(key: &str) -> Option<&'static str> {
    // Modifiers first: `KEY_TO_CODE` has the left-hand names, and upstream writes `Meta`
    // only for macOS' Command key in the chat and stash-search sequences.
    let named = match key {
        "Ctrl" => "ctrl",
        "Alt" => "alt",
        "Shift" => "shift",
        "Meta" | "Command" | "Cmd" | "Super" => "super",
        "Enter" => "enter",
        "Escape" => "escape",
        "Space" => "space",
        "Tab" => "tab",
        "Backspace" => "backspace",
        "Delete" => "delete",
        "Insert" => "insert",
        "Home" => "home",
        "End" => "end",
        "PageUp" => "pageup",
        "PageDown" => "pagedown",
        "ArrowLeft" => "left",
        "ArrowRight" => "right",
        "ArrowUp" => "up",
        "ArrowDown" => "down",
        "Minus" => "minus",
        "Equal" => "equal",
        "Comma" => "comma",
        "Period" => "period",
        "Slash" => "slash",
        "Semicolon" => "semicolon",
        "Quote" => "quote",
        "BracketLeft" => "bracketleft",
        "BracketRight" => "bracketright",
        "Backslash" => "backslash",
        "Backquote" => "backquote",
        _ => return other_key(key),
    };
    Some(named)
}

/// Letters, digits and the function keys: `A` → `a`, `7` → `7`, `F5` → `f5`.
fn other_key(key: &str) -> Option<&'static str> {
    if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() {
        // `Shortcut::parse` lowercases its parts, so a static name is not needed for these.
        return Some(match key {
            "0" => "0",
            "1" => "1",
            "2" => "2",
            "3" => "3",
            "4" => "4",
            "5" => "5",
            "6" => "6",
            "7" => "7",
            "8" => "8",
            "9" => "9",
            _ => LETTERS.iter().copied().find(|name| name.eq_ignore_ascii_case(key))?,
        });
    }
    const FUNCTIONS: [&str; 24] = [
        "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12", "f13", "f14", "f15", "f16", "f17",
        "f18", "f19", "f20", "f21", "f22", "f23", "f24",
    ];
    let number: u32 = key.strip_prefix(['F', 'f'])?.parse().ok()?;
    FUNCTIONS.get(usize::try_from(number).ok()?.checked_sub(1)?).copied()
}

/// The alphabet, so a letter maps to a `'static` name.
const LETTERS: [&str; 26] = [
    "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p", "q", "r", "s", "t", "u", "v", "w",
    "x", "y", "z",
];

/// `Ctrl + Alt + D` → `ctrl+alt+d`, the chord [`rune::Shortcut`] parses. `None` when a part
/// is not a key name (the hotkey then cannot be registered).
pub fn accelerator(shortcut: &str) -> Option<String> {
    let mut parts = Vec::new();
    for key in shortcut.split(" + ") {
        let key = key.trim();
        if key.is_empty() {
            return None;
        }
        parts.push(rune_key(key)?);
    }
    let chord = parts.join("+");
    // Parse it here rather than at registration: a chord Rune refuses is a config problem
    // the shell reports, not a panic in the event loop.
    chord.parse::<rune::Shortcut>().ok().map(|shortcut| shortcut.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_hotkeys_become_rune_chords() {
        assert_eq!(accelerator("Ctrl + D").as_deref(), Some("ctrl+d"));
        assert_eq!(accelerator("Ctrl + Alt + D").as_deref(), Some("ctrl+alt+d"));
        assert_eq!(accelerator("Shift + Space").as_deref(), Some("shift+space"));
        assert_eq!(accelerator("Meta + C").as_deref(), Some("super+c"));
        assert_eq!(accelerator("Ctrl + Shift + F5").as_deref(), Some("ctrl+shift+f5"));
        assert_eq!(accelerator("Alt + ArrowUp").as_deref(), Some("alt+up"));
        assert_eq!(accelerator("Ctrl + Semicolon").as_deref(), Some("ctrl+semicolon"));
        // Rune prints modifiers in its own order, whatever the config's order was.
        assert_eq!(accelerator("Alt + Ctrl + F").as_deref(), Some("ctrl+alt+f"));
    }

    #[test]
    fn what_rune_cannot_parse_is_refused() {
        assert_eq!(accelerator("Ctrl + NumpadAdd"), None);
        assert_eq!(accelerator("Ctrl + NotAKey"), None);
        assert_eq!(accelerator("Ctrl + "), None);
        assert_eq!(accelerator(""), None);
        assert_eq!(accelerator("Ctrl + F25"), None);
    }
}
