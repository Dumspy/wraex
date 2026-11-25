//! `ipc/KeyToCode.ts`, identical in both references: key names, Windows
//! virtual-key codes (which the game's config uses), and hotkey strings
//! (`Ctrl + Shift + K`). `KeyToElectron` maps to Electron accelerators and is
//! not needed here.

/// `KeyToCode`, in JavaScript's iteration order (integer keys first).
pub const KEY_TO_CODE: [(&str, u32); 105] = [
    ("0", 48),
    ("1", 49),
    ("2", 50),
    ("3", 51),
    ("4", 52),
    ("5", 53),
    ("6", 54),
    ("7", 55),
    ("8", 56),
    ("9", 57),
    ("Backspace", 8),
    ("Tab", 9),
    ("Enter", 13),
    ("Shift", 16),
    ("Ctrl", 17),
    ("Alt", 18),
    ("CapsLock", 20),
    ("Escape", 27),
    ("Space", 32),
    ("PageUp", 33),
    ("PageDown", 34),
    ("End", 35),
    ("Home", 36),
    ("ArrowLeft", 37),
    ("ArrowUp", 38),
    ("ArrowRight", 39),
    ("ArrowDown", 40),
    ("Insert", 45),
    ("Delete", 46),
    ("A", 65),
    ("B", 66),
    ("C", 67),
    ("D", 68),
    ("E", 69),
    ("F", 70),
    ("G", 71),
    ("H", 72),
    ("I", 73),
    ("J", 74),
    ("K", 75),
    ("L", 76),
    ("M", 77),
    ("N", 78),
    ("O", 79),
    ("P", 80),
    ("Q", 81),
    ("R", 82),
    ("S", 83),
    ("T", 84),
    ("U", 85),
    ("V", 86),
    ("W", 87),
    ("X", 88),
    ("Y", 89),
    ("Z", 90),
    ("Numpad0", 96),
    ("Numpad1", 97),
    ("Numpad2", 98),
    ("Numpad3", 99),
    ("Numpad4", 100),
    ("Numpad5", 101),
    ("Numpad6", 102),
    ("Numpad7", 103),
    ("Numpad8", 104),
    ("Numpad9", 105),
    ("NumpadMultiply", 106),
    ("NumpadAdd", 107),
    ("NumpadSubtract", 109),
    ("NumpadDecimal", 110),
    ("NumpadDivide", 111),
    ("F1", 112),
    ("F2", 113),
    ("F3", 114),
    ("F4", 115),
    ("F5", 116),
    ("F6", 117),
    ("F7", 118),
    ("F8", 119),
    ("F9", 120),
    ("F10", 121),
    ("F11", 122),
    ("F12", 123),
    ("F13", 124),
    ("F14", 125),
    ("F15", 126),
    ("F16", 127),
    ("F17", 128),
    ("F18", 129),
    ("F19", 130),
    ("F20", 131),
    ("F21", 132),
    ("F22", 133),
    ("F23", 134),
    ("F24", 135),
    ("Semicolon", 186),
    ("Equal", 187),
    ("Comma", 188),
    ("Minus", 189),
    ("Period", 190),
    ("Slash", 191),
    ("Backquote", 192),
    ("BracketLeft", 219),
    ("Backslash", 220),
    ("BracketRight", 221),
    ("Quote", 222),
];

/// `KeyToCode[key]`
pub fn key_to_code(key: &str) -> Option<u32> {
    KEY_TO_CODE.iter().find(|(k, _)| *k == key).map(|(_, c)| *c)
}

/// `CodeToKey[code]`: the key of a code as text (`"18"` is `Alt`).
pub fn code_to_key(code: &str) -> Option<&'static str> {
    KEY_TO_CODE.iter().find(|(_, c)| c.to_string() == code).map(|(k, _)| *k)
}

/// [`KEY_TO_CODE`]'s reverse for a Windows virtual-key code, as the low-level keyboard hook
/// reports it (`KBDLLHOOKSTRUCT.vkCode`). The left/right modifier variants (`VK_LCONTROL` 0xA2,
/// `VK_RCONTROL` 0xA3, ...) map to the plain name: they are the same key as far as a hotkey or
/// `uiohook`'s events are concerned, and `KeyToCode` only has the plain one.
pub fn vk_to_key(code: u32) -> Option<&'static str> {
    match code {
        0xA0 | 0xA1 => return Some("Shift"), // VK_LSHIFT, VK_RSHIFT
        0xA2 | 0xA3 => return Some("Ctrl"),  // VK_LCONTROL, VK_RCONTROL
        0xA4 | 0xA5 => return Some("Alt"),   // VK_LMENU, VK_RMENU
        _ => {}
    }
    KEY_TO_CODE.iter().find(|(_, vk)| *vk == code).map(|(name, _)| *name)
}

/// `isModKey`
pub fn is_mod_key(key: &str) -> bool {
    matches!(key, "Ctrl" | "Shift" | "Alt")
}

/// `hotkeyToString`: modifiers first, in a fixed order, then the keys.
pub fn hotkey_to_string(keys: &[&str], mut ctrl: bool, mut shift: bool, mut alt: bool) -> String {
    ctrl |= keys.contains(&"Ctrl");
    shift |= keys.contains(&"Shift");
    alt |= keys.contains(&"Alt");
    let keys: Vec<&str> = keys.iter().copied().filter(|k| !is_mod_key(k)).collect();

    let modifier = match (ctrl, shift, alt) {
        (true, true, true) => "Ctrl + Shift + Alt",
        (_, true, true) => "Shift + Alt",
        (true, true, false) => "Ctrl + Shift",
        (true, false, true) => "Ctrl + Alt",
        (false, false, true) => "Alt",
        (true, false, false) => "Ctrl",
        (false, true, false) => "Shift",
        (false, false, false) => "",
    };
    let joined = keys.join(" + ");
    if !modifier.is_empty() && !keys.is_empty() {
        format!("{modifier} + {joined}")
    } else if joined.is_empty() {
        modifier.to_owned()
    } else {
        joined
    }
}

/// `mergeTwoHotkeys`: both hotkeys' keys, each once, as one hotkey.
pub fn merge_two_hotkeys(a: &str, b: &str) -> String {
    let mut keys: Vec<&str> = Vec::new();
    for key in a.split(" + ").chain(b.split(" + ")) {
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    hotkey_to_string(&keys, false, false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_and_hotkeys() {
        assert_eq!(code_to_key("18"), Some("Alt"));
        assert_eq!(code_to_key("222"), Some("Quote"));
        assert_eq!(code_to_key("999"), None);
        assert_eq!(key_to_code("F5"), Some(116));
        assert_eq!(hotkey_to_string(&["D"], true, false, false), "Ctrl + D");
        assert_eq!(hotkey_to_string(&["Alt"], false, false, false), "Alt");
        assert_eq!(hotkey_to_string(&["D", "Shift", "Ctrl"], false, false, true), "Ctrl + Shift + Alt + D");
        assert_eq!(merge_two_hotkeys("Ctrl + D", "Shift + D"), "Ctrl + Shift + D");
    }

    #[test]
    fn virtual_key_codes_name_their_keys() {
        // `KeyToCode`'s pairs, looked up the other way (what the keyboard hook has).
        assert_eq!(vk_to_key(18), Some("Alt"));
        assert_eq!(vk_to_key(68), Some("D"));
        assert_eq!(vk_to_key(116), Some("F5"));
        assert_eq!(vk_to_key(222), Some("Quote"));
        // The left/right modifiers are the plain names, which is all `KeyToCode` has.
        assert_eq!(vk_to_key(0xA0), Some("Shift"));
        assert_eq!(vk_to_key(0xA1), Some("Shift"));
        assert_eq!(vk_to_key(0xA2), Some("Ctrl"));
        assert_eq!(vk_to_key(0xA3), Some("Ctrl"));
        assert_eq!(vk_to_key(0xA4), Some("Alt"));
        assert_eq!(vk_to_key(0xA5), Some("Alt"));
        // Media keys and the like have no name in the hotkey config.
        assert_eq!(vk_to_key(0xAD), None);
        assert_eq!(vk_to_key(0), None);
    }
}
