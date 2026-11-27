//! Synthetic keys behind [`wraex_platform::shortcuts::KeySender`].
//!
//! Upstream presses keys with `uiohook-napi`'s `keyToggle`/`keyTap`, which on every platform
//! posts events into the system's input stream so the focused game sees them. Rune has no
//! input-injection API (its keys flow the other way: the OS into a Rune window), so this is
//! per-platform code:
//!
//! - **macOS**: `CGEventCreateKeyboardEvent` + `CGEventPost(kCGHIDEventTap)`, the same
//!   `CGEvent` API `uiohook` wraps. Posting into another application's process needs the
//!   user's Input Monitoring/Accessibility permission; without it the posts are silently
//!   dropped, as they are for any tool that types for the user. Physical (US) key positions
//!   are used, matching Rune's registration and the game's own hotkeys.
//! - **Windows**: `SendInput` with `KEYBDINPUT`s, which needs no permission.
//! - **Linux**: no implementation yet. `XTest` would need `libXtst` linked and an X11
//!   connection; Wayland has no portable injection. The sender logs and does nothing, so a
//!   hotkey still opens the widget on the clipboard's item text when there is one
//!   ([`crate::shell`]'s fallback) instead of failing.
//!
//! Delays are real sleeps: `Shortcuts` asks for upstream's `setTimeout` gaps (EE2 holds the
//! copy keys for 10 ms so the game reads them).
//!
//! # When keys may be pressed
//!
//! Upstream presses keys only while the game window is focused: its hotkeys are registered on
//! the game's `active-change` and unregistered when it loses the focus, so a press always lands
//! in the game. Wraex keeps that rule, and it decides whether [`OSSender`] synthesizes at all:
//!
//! - **The game's window is tracked** (Windows today, `wraex_platform::window::Watcher`;
//!   [`OSSender::set_game_focus_tracking`]): presses are safe by default, because the shell
//!   only registers the hotkeys while the game is focused and unregisters them the moment the
//!   overlay takes the keys.
//! - **No tracking** (macOS, X11, Wayland): the shell's hotkeys are system-wide, so a press
//!   would go to whatever application has the focus. Synthesis stays opt-in with
//!   `WRAEX_KEY_SYNTHESIS=1`, and without it a copy hotkey prices the item already on the
//!   clipboard instead (the shell's fallback).
//!
//! `WRAEX_KEY_SYNTHESIS=1` also turns synthesis on where tracking exists, which is what a
//! developer wants while testing a single piece of the chain.

use wraex_core::trade::rate_limit::Millis;
use wraex_platform::overlay::OverlayInput;
use wraex_platform::shortcuts::KeySender;

/// The keys a hotkey can press, mapped per platform below. Upstream's names
/// ([`wraex_core::keys::KEY_TO_CODE`]) are the input.
#[derive(Debug, Default)]
pub struct OSSender {
    /// The game's window is followed, so hotkeys only exist while it is focused
    /// ([`OSSender::set_game_focus_tracking`]).
    game_focus_tracking: bool,
    #[cfg(target_os = "macos")]
    flags: u64,
}

impl OSSender {
    pub fn new() -> Self {
        Self::default()
    }

    /// The shell started following the game's window (`wraex_platform::window::Watcher`):
    /// from here on a press can only happen while the game is focused, so synthesis no longer
    /// needs `WRAEX_KEY_SYNTHESIS`. See the module's note.
    pub const fn set_game_focus_tracking(&mut self) {
        self.game_focus_tracking = true;
    }

    /// Whether this build can press keys into the focused application. Where it cannot, the
    /// shell prices the item already on the clipboard instead of pressing the game's copy
    /// key ([`crate::shell`]).
    pub fn can_synthesize(&self) -> bool {
        if !platform::can_synthesize() {
            return false;
        }
        self.game_focus_tracking || std::env::var_os("WRAEX_KEY_SYNTHESIS").is_some_and(|value| value == "1")
    }
}

impl KeySender for OSSender {
    fn key_down(&mut self, key: &str) {
        if self.can_synthesize() {
            platform::key(self, key, true);
        }
    }

    fn key_up(&mut self, key: &str) {
        if self.can_synthesize() {
            platform::key(self, key, false);
        }
    }

    /// `uIOhook.keyTap(key, modifiers)`: the modifiers down, the key down and up, the
    /// modifiers up again.
    fn key_tap(&mut self, key: &str, modifiers: &[&str]) {
        for modifier in modifiers {
            self.key_down(modifier);
        }
        if self.can_synthesize() {
            platform::key(self, key, true);
            platform::key(self, key, false);
        }
        for modifier in modifiers.iter().rev() {
            self.key_up(modifier);
        }
    }

    fn sleep(&mut self, millis: Millis) {
        std::thread::sleep(std::time::Duration::from_millis(millis));
    }
}

#[cfg(target_os = "macos")]
mod platform {
    #![expect(
        unsafe_code,
        reason = "CoreGraphics and CoreFoundation are C frameworks: the event calls and the \
                  display queries here are the platform's only way to post a key or read the cursor"
    )]

    use super::OSSender;
    use wraex_core::num::usize_to_f64;

    /// `CGEventFlags`: the bits `uiohook` sets for the modifiers it holds down.
    const SHIFT: u64 = 1 << 17;
    const CONTROL: u64 = 1 << 18;
    const ALTERNATE: u64 = 1 << 19;
    const COMMAND: u64 = 1 << 20;
    /// `kCGHIDEventTap`: the tap the system's own input passes through.
    const HID_EVENT_TAP: u32 = 0;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    type CGEventRef = *mut std::ffi::c_void;
    type CGEventSourceRef = *mut std::ffi::c_void;
    type CFTypeRef = *const std::ffi::c_void;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGEventCreate(source: CGEventSourceRef) -> CGEventRef;
        fn CGEventCreateKeyboardEvent(source: CGEventSourceRef, key: u16, down: bool) -> CGEventRef;
        fn CGEventSetFlags(event: CGEventRef, flags: u64);
        fn CGEventPost(tap: u32, event: CGEventRef);
        fn CGEventGetLocation(event: CGEventRef) -> CGPoint;
    }

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFRelease(cf: CFTypeRef);
    }

    /// `kVK_*`: the physical key codes `uiohook`'s key names map to (US layout positions).
    fn keycode(key: &str) -> Option<u16> {
        Some(match key {
            "A" => 0,
            "S" => 1,
            "D" => 2,
            "F" => 3,
            "H" => 4,
            "G" => 5,
            "Z" => 6,
            "X" => 7,
            "C" => 8,
            "V" => 9,
            "B" => 11,
            "Q" => 12,
            "W" => 13,
            "E" => 14,
            "R" => 15,
            "Y" => 16,
            "T" => 17,
            "1" => 18,
            "2" => 19,
            "3" => 20,
            "4" => 21,
            "6" => 22,
            "5" => 23,
            "Equal" => 24,
            "9" => 25,
            "7" => 26,
            "Minus" => 27,
            "8" => 28,
            "0" => 29,
            "BracketRight" => 30,
            "O" => 31,
            "U" => 32,
            "BracketLeft" => 33,
            "I" => 34,
            "P" => 35,
            "Enter" => 36,
            "L" => 37,
            "J" => 38,
            "Quote" => 39,
            "K" => 40,
            "Semicolon" => 41,
            "Backslash" => 42,
            "Comma" => 43,
            "Slash" => 44,
            "N" => 45,
            "M" => 46,
            "Period" => 47,
            "Tab" => 48,
            "Space" => 49,
            "Backquote" => 50,
            "Backspace" => 51,
            "Escape" => 53,
            "Meta" => 55,
            "Shift" => 56,
            "Alt" => 58,
            "Ctrl" => 59,
            "NumpadDecimal" => 65,
            "NumpadMultiply" => 67,
            "NumpadAdd" => 69,
            "NumpadDivide" => 75,
            "NumpadEnter" => 76,
            "NumpadSubtract" => 78,
            "Home" => 115,
            "PageUp" => 116,
            "Delete" => 117,
            "End" => 119,
            "PageDown" => 121,
            "ArrowLeft" => 123,
            "ArrowRight" => 124,
            "ArrowDown" => 125,
            "ArrowUp" => 126,
            _ => return function_key(key),
        })
    }

    /// `F1`–`F20`, whose codes are not in order.
    fn function_key(key: &str) -> Option<u16> {
        Some(match key {
            "F1" => 122,
            "F2" => 120,
            "F3" => 99,
            "F4" => 118,
            "F5" => 96,
            "F6" => 97,
            "F7" => 98,
            "F8" => 100,
            "F9" => 101,
            "F10" => 109,
            "F11" => 103,
            "F12" => 111,
            "F13" => 105,
            "F14" => 107,
            "F15" => 113,
            "F16" => 106,
            "F17" => 64,
            "F18" => 79,
            "F19" => 80,
            "F20" => 90,
            _ => return None,
        })
    }

    /// The flag a modifier key sets, if `key` is one.
    fn modifier_flag(key: &str) -> Option<(u16, u64)> {
        Some(match key {
            "Shift" => (keycode("Shift")?, SHIFT),
            "Ctrl" => (keycode("Ctrl")?, CONTROL),
            "Alt" => (keycode("Alt")?, ALTERNATE),
            "Meta" => (keycode("Meta")?, COMMAND),
            _ => return None,
        })
    }

    /// Post one key event at `flags` (`CGEventPost` into the HID tap).
    fn post(key: u16, down: bool, flags: u64) {
        // SAFETY: `CGEventCreateKeyboardEvent` returns a newly created event the caller owns,
        // or null.
        let event = unsafe { CGEventCreateKeyboardEvent(std::ptr::null_mut(), key, down) };
        if event.is_null() {
            return;
        }
        // SAFETY: `event` is the live event created above; CoreGraphics only reads it.
        unsafe { CGEventSetFlags(event, flags) };
        // SAFETY: the same live event, posted once into the HID tap.
        unsafe { CGEventPost(HID_EVENT_TAP, event) };
        // SAFETY: the event is not used after this release.
        unsafe { CFRelease(event.cast_const()) };
    }

    pub(super) fn key(sender: &mut OSSender, name: &str, down: bool) {
        let Some(key) = keycode(name) else {
            wraex_platform::log!("wraex: cannot synthesize the key `{name}`");
            return;
        };
        if let Some((_, flag)) = modifier_flag(name) {
            if down {
                sender.flags |= flag;
            } else {
                sender.flags &= !flag;
            }
        }
        post(key, down, sender.flags);
    }

    /// The cursor's position in points, from the top left of the main display: what
    /// `screen.getCursorScreenPoint()` is upstream. `None` when the system gives no event.
    pub fn cursor_position() -> Option<(f64, f64)> {
        // SAFETY: `CGEventCreate` returns a new event the caller owns, or null.
        let event = unsafe { CGEventCreate(std::ptr::null_mut()) };
        if event.is_null() {
            return None;
        }
        // SAFETY: `event` is the live event created above; the location is a value.
        let point = unsafe { CGEventGetLocation(event) };
        // SAFETY: the event is not used after this release.
        unsafe { CFRelease(event.cast_const()) };
        Some((point.x, point.y))
    }

    /// `CGPreflightPostEventAccess`: whether this process may post events into other
    /// applications (Input Monitoring/Accessibility in System Settings). Without it the posts
    /// are dropped, so the app prices the clipboard instead.
    pub fn can_synthesize() -> bool {
        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGPreflightPostEventAccess() -> u8;
        }

        // SAFETY: a plain CoreGraphics query with no borrowed state.
        unsafe { CGPreflightPostEventAccess() != 0 }
    }

    /// `CGDisplayPixelsWide(main) / CGDisplayBounds(main).width`: the main display's backing
    /// scale, which turns points into the physical pixels `AppHandle::set_position` takes.
    pub fn main_display_scale() -> f64 {
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct CGSize {
            width: f64,
            height: f64,
        }
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct CGRect {
            origin: CGPoint,
            size: CGSize,
        }

        #[link(name = "CoreGraphics", kind = "framework")]
        unsafe extern "C" {
            fn CGMainDisplayID() -> u32;
            fn CGDisplayPixelsWide(display: u32) -> usize;
            fn CGDisplayBounds(display: u32) -> CGRect;
        }

        // SAFETY: a plain CoreGraphics query with no borrowed state.
        let display = unsafe { CGMainDisplayID() };
        // SAFETY: the display id came from `CGMainDisplayID`; the bounds are a value.
        let bounds = unsafe { CGDisplayBounds(display) };
        let points = bounds.size.width;
        if points <= 0.0 {
            return 1.0;
        }
        // SAFETY: the same display id, which CoreGraphics only reads.
        let pixels = unsafe { CGDisplayPixelsWide(display) };
        usize_to_f64(pixels) / points
    }
}

#[cfg(target_os = "windows")]
mod platform {
    #![expect(
        unsafe_code,
        reason = "`SendInput` and `GetCursorPos` are user32 calls: Win32 has no safe wrapper here, \
                  and the structs are the SDK's `INPUT` layout"
    )]

    use super::OSSender;

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct KeybdInput {
        virtual_key: u16,
        scan_code: u16,
        flags: u32,
        time: u32,
        extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct MouseInput {
        dx: i32,
        dy: i32,
        mouse_data: u32,
        flags: u32,
        time: u32,
        extra_info: usize,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct HardwareInput {
        message: u32,
        param_low: u16,
        param_high: u16,
    }

    #[repr(C)]
    union InputUnion {
        key: KeybdInput,
        mouse: MouseInput,
        hardware: HardwareInput,
    }

    #[repr(C)]
    struct Input {
        kind: u32,
        data: InputUnion,
    }

    const INPUT_KEYBOARD: u32 = 1;
    const KEY_UP: u32 = 0x0002;
    /// Arrows, Home/End, Insert/Delete and the like are "extended" keys.
    const EXTENDED: u32 = 0x0001;

    #[link(name = "user32")]
    unsafe extern "system" {
        fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
    }

    /// The virtual-key code for one of upstream's names (`KeyToCode` is exactly Windows'
    /// `VK_*` table), plus whether the key is an extended one.
    fn virtual_key(name: &str) -> Option<(u16, bool)> {
        let extended = matches!(
            name,
            "ArrowLeft"
                | "ArrowRight"
                | "ArrowUp"
                | "ArrowDown"
                | "Home"
                | "End"
                | "PageUp"
                | "PageDown"
                | "Insert"
                | "Delete"
                | "NumpadEnter"
                | "NumpadDivide"
        );
        // `KeyToCode` is the Windows table; `Meta` is upstream's name for the Windows key.
        let name = match name {
            "Meta" => "LWin",
            other => other,
        };
        let code = match name {
            "LWin" => 0x5B,
            other => u16::try_from(wraex_core::keys::key_to_code(other)?).ok()?,
        };
        Some((code, extended))
    }

    /// One `SendInput` of a keyboard event with `flags` (`KEY_UP` for the release).
    fn send(virtual_key: u16, flags: u32) {
        let input = Input {
            kind: INPUT_KEYBOARD,
            data: InputUnion { key: KeybdInput { virtual_key, scan_code: 0, flags, time: 0, extra_info: 0 } },
        };
        // `INPUT` is 40 bytes at most: the fallback never happens.
        let size = i32::try_from(std::mem::size_of::<Input>()).unwrap_or(i32::MAX);
        // SAFETY: `input` is a live, fully initialised `INPUT` of the size reported.
        unsafe {
            SendInput(1, &raw const input, size);
        }
    }

    pub(super) fn key(_sender: &mut OSSender, name: &str, down: bool) {
        let Some((virtual_key, extended)) = virtual_key(name) else {
            wraex_platform::log!("wraex: cannot synthesize the key `{name}`");
            return;
        };
        let mut flags = if down { 0 } else { KEY_UP };
        if extended {
            flags |= EXTENDED;
        }
        send(virtual_key, flags);
    }

    /// `GetCursorPos`: the cursor in physical desktop pixels.
    pub fn cursor_position() -> Option<(f64, f64)> {
        #[repr(C)]
        #[derive(Clone, Copy, Default)]
        struct Point {
            x: i32,
            y: i32,
        }

        #[link(name = "user32")]
        unsafe extern "system" {
            fn GetCursorPos(point: *mut Point) -> i32;
        }

        let mut point = Point::default();
        // SAFETY: `point` is a live, writable `POINT`.
        let ok = unsafe { GetCursorPos(&raw mut point) } != 0;
        ok.then(|| (f64::from(point.x), f64::from(point.y)))
    }

    /// `SendInput` needs no permission.
    pub const fn can_synthesize() -> bool {
        true
    }

    /// Windows reports physical pixels already.
    pub const fn main_display_scale() -> f64 {
        1.0
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use super::OSSender;

    pub(super) fn key(_sender: &mut OSSender, name: &str, down: bool) {
        // One line per press rather than silence: a user debugging a hotkey on Linux should
        // know the app cannot type into the game yet.
        static WARNED: std::sync::Once = std::sync::Once::new();
        WARNED.call_once(|| {
            wraex_platform::log!(
                "wraex: this build cannot synthesize keys (no XTest/Wayland implementation yet); \
                 hotkeys act on the clipboard's item text instead"
            );
        });
        let action = if down { "down" } else { "up" };
        wraex_platform::log!("wraex: key {name} {action} (not synthesized)");
    }

    /// No implementation yet (see the module's note): `XTest` would need `libXtst` and an X11
    /// connection, and Wayland has no portable injection.
    pub const fn can_synthesize() -> bool {
        false
    }

    /// No portable cursor query without X11/Wayland dependencies yet.
    pub const fn cursor_position() -> Option<(f64, f64)> {
        None
    }

    pub const fn main_display_scale() -> f64 {
        1.0
    }
}

/// The cursor in physical desktop pixels (the units `AppHandle::set_position` takes), for
/// placing a widget next to it.
pub fn cursor_position() -> Option<(f64, f64)> {
    platform::cursor_position().map(|(x, y)| {
        let scale = platform::main_display_scale();
        (x * scale, y * scale)
    })
}

/// A key pressed in the overlay window, as `OverlayWindow.handleExtraCommands` reads it.
///
/// It is Electron's `before-input-event` `input.code`, which [`OverlayInput::hotkey`] turns into
/// a hotkey string. Only the keys that handler reacts to are mapped: Escape, Ctrl+W, and the
/// overlay key, which is a letter or Space.
///
/// Rune reports a letter with its primary modifier as `Key::Command` (Ctrl off macOS, ⌘ on
/// it) and Space as `Key::Text(" ")`; `mods` still says which modifiers were held, so
/// Ctrl+W off macOS arrives as `Command('w')` with `ctrl` set.
pub fn overlay_input(event: &rune::KeyEvent) -> Option<OverlayInput> {
    let code = match &event.key {
        rune::Key::Escape => "Escape".to_owned(),
        rune::Key::Command(letter) if letter.is_ascii_alphabetic() => {
            format!("Key{}", letter.to_ascii_uppercase())
        }
        rune::Key::Text(text) if text == " " => "Space".to_owned(),
        _ => return None,
    };
    Some(OverlayInput { code, ctrl: event.mods.ctrl, shift: event.mods.shift, alt: event.mods.alt })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune::{Key, KeyEvent, Mods};

    /// The keys `OverlayWindow.handleExtraCommands` handles reach it in Rune's shape.
    #[test]
    fn the_overlays_keys_are_mapped_from_runes_names() {
        let event = |key: Key, mods: Mods| KeyEvent { key, mods };
        let ctrl = Mods { cmd: true, ctrl: true, ..Mods::default() };

        let escape = overlay_input(&event(Key::Escape, Mods::default())).expect("Escape is handled");
        assert_eq!(escape.hotkey(), "Escape");
        // Ctrl+W off macOS: `Command('w')` with `ctrl` (and `cmd`) set, which upstream's
        // `input.code` spells `KeyW` + control.
        let ctrl_w = overlay_input(&event(Key::Command('w'), ctrl)).expect("Ctrl+W is handled");
        assert_eq!(ctrl_w.hotkey(), "Ctrl + W");
        // Shift+Space, the default overlay key.
        let overlay_key = overlay_input(&event(Key::Text(" ".to_owned()), Mods { shift: true, ..Mods::default() }))
            .expect("Space is handled");
        assert_eq!(overlay_key.hotkey(), "Shift + Space");
        // Anything else is left to the widget (a typed letter, a scroll key).
        assert!(overlay_input(&event(Key::Text("a".to_owned()), Mods::default())).is_none());
        assert!(overlay_input(&event(Key::Enter, Mods::default())).is_none());
        assert!(overlay_input(&event(Key::Command('1'), Mods::default())).is_none());
    }

    #[test]
    fn the_sender_can_press_keys_where_it_is_implemented() {
        let sender = OSSender::new();
        // macOS without Input Monitoring permission cannot post either, so the build's
        // support is the upper bound, not the answer. Without game tracking the environment
        // decides (see the module's note); a test may run with it set.
        if !cfg!(any(target_os = "macos", target_os = "windows")) {
            assert!(!sender.can_synthesize());
        }
        let mut tracked = OSSender::new();
        tracked.set_game_focus_tracking();
        // Tracking makes synthesis the platform's answer alone.
        assert_eq!(tracked.can_synthesize(), platform::can_synthesize());
    }

    /// A press of a key the platform does not know must not panic (it logs instead).
    #[test]
    fn an_unknown_key_is_harmless() {
        let mut sender = OSSender::new();
        sender.key_down("NotAKey");
        sender.key_up("NotAKey");
        sender.key_tap("NotAKey", &["Ctrl"]);
        sender.sleep(0);
    }
}
