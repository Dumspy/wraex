//! Finding the game window and following its moves, resizes and focus.
//!
//! Ported from `main/src/windowing/GameWindow.ts`, which is the same file in both
//! references: the host's view of the game's window. It is the state the rest of
//! the host reacts to —
//!
//! - hotkeys are only registered while the game is focused
//!   (`Shortcuts.ts`'s `active-change` listener, [`crate::shortcuts`]),
//! - the overlay gives focus to the game and takes it away ([`crate::overlay`]),
//! - the stash-scroll wheel and the widget area need the window's rectangle
//!   ([`crate::shortcuts::is_stash_area`], [`crate::overlay`]).
//!
//! [`GameWindow`] itself is pure state. The OS layer feeds it [`GameWindowEvents`]
//! (a focus/blur hook, `electron-overlay-window`'s attach result, the tracked
//! rectangle) and forwards the [`WindowEvent`]s it returns.
//!
//! # The OS layer
//!
//! There is no Rust equivalent to `electron-overlay-window`, so what it does natively is
//! written here, behind [`Watcher`] (PLAN.md, phase 2):
//!
//! - **Win32** (`window/win32.rs`, `cfg(windows)`): find the window by title (and class, when a
//!   target names one — [`WindowTarget`], [`matches_target`]); `SetWinEventHook` for the
//!   system foreground, restore-from-minimise, the foreground window's title change, the
//!   game window's moves and its destruction, all out-of-context on a dedicated thread
//!   with a message loop; an 83 ms timer for the foreground changes Windows' foreground
//!   lock swallows; the game's *client* rectangle (`GetClientRect` + `ClientToScreen`, so
//!   borderless-windowed and fullscreen both land right); the UIPI probe that says the
//!   game runs elevated (the "PoE window - No access" case); `SetForegroundWindow` with
//!   the foreground-lock workaround for the focus hand-back; a `WH_MOUSE_LL` hook for
//!   global mouse moves, buttons and wheel notches ([`MouseEvent`], `uiohook-napi`'s
//!   half that follows the price check and scrolls the stash tabs); and a
//!   `WH_KEYBOARD_LL` hook for every key's down and up ([`KeyEvent`]), which is the
//!   Alt peek and the key log `uiohook-napi` provides. Both hooks only observe.
//!   This is behaviour ported from that package's README and Win32 backend, not its code.
//! - **macOS / X11 / Wayland** have no backend yet, so [`Watcher::start`] reports
//!   [`WatchError::Unsupported`] and the shell keeps its ordinary windows. Follow-ups:
//!   macOS' Accessibility API (`AXUIElement`, which needs the user's permission — EE2's
//!   `main.ts` asks for it and quits without it), X11's `_NET_ACTIVE_WINDOW` and
//!   `ConfigureNotify`; Wayland has no cross-application window API at all, which PLAN.md
//!   notes as a known loss (Rune's overlays are likewise not interactive there).
//!
//! Rune supplies the overlay surface and a raw window handle
//! ([`crate::overlay::OverlayHandle`]); [`place_overlay`], [`dpi_scale`] and
//! [`focus_window`] are the few geometry/focus calls a Rune window cannot make for itself.
//!
//! Reading the game's pixels (`OverlayController.screenshot()`, the heist gem
//! finder's input) is OS work too — Win32 `BitBlt`, X11 `XGetImage`, and not
//! permitted at all on Wayland — and stays there with the same candidates.

#[cfg(not(windows))]
mod unsupported;
#[cfg(windows)]
mod win32;

#[cfg(not(windows))]
pub use unsupported::{Watcher, configure_overlay, dpi_scale, focus_window, place_overlay};
#[cfg(windows)]
pub use win32::{Watcher, configure_overlay, dpi_scale, focus_window, place_overlay};

use crate::shortcuts::InputEvent;

/// A rectangle in screen coordinates (Electron's `Rectangle`,
/// `OverlayController.targetBounds`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bounds {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// A point in screen coordinates (Electron's `Point`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    /// `Math.hypot(x - from.x, y - from.y)`.
    pub fn distance_to(self, from: Self) -> f64 {
        (self.x - from.x).hypot(self.y - from.y)
    }

    /// `isPointInsideRect`: strictly inside, edges excluded.
    pub fn is_inside(self, area: Bounds) -> bool {
        self.x > area.x && self.x < area.x + area.width && self.y > area.y && self.y < area.y + area.height
    }
}

/// A mouse event from the OS-level hook, in physical screen pixels: the half of
/// `uiohook-napi` that `WidgetAreaTracker.ts` and `Shortcuts.ts` listen to.
///
/// Upstream's `mousemove` carries the modifier state the tracker compares with the price
/// check's `hotkeyHold`; `mousedown` only needs the position, and `wheel` the notch and
/// Ctrl for [`crate::shortcuts::Shortcuts::on_wheel`]. Only observed: the hook never blocks
/// or injects an event.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseEvent {
    /// `uIOhook`'s `mousemove`.
    Move { point: Point, ctrl: bool, alt: bool },
    /// `uIOhook`'s `mousedown` (any button).
    Down { point: Point },
    /// `uIOhook`'s `wheel`: whole notches, positive toward the user (scrolling down), as
    /// libuiohook reports them on Windows ([`wheel_rotation`]).
    Wheel { point: Point, rotation: i32, ctrl: bool },
}

/// A key event from the OS-level keyboard hook (`uiohook-napi`'s `keydown`/`keyup`).
///
/// Every key is reported, not only the modifiers: `OverlayVisibility`'s Alt peek hides the
/// widgets on an Alt-only keydown and brings them back on any other key, and `Shortcuts`'
/// `logKeys` logs them all. Only observed: the hook never blocks, swallows or injects a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// Activity outside both monitored windows cancels the Alt peek without disclosing a key.
    Outside,
    /// `uIOhook`'s `keydown`, with the modifiers held at the time.
    Down(InputEvent),
    /// `uIOhook`'s `keyup`. The modifier the released key names is already cleared, which is
    /// what `OverlayVisibility`'s `if (!e.altKey)` relies on.
    Up(InputEvent),
}

#[cfg(any(windows, test))]
fn scoped_key_event(
    foreground: isize,
    game: isize,
    overlay: Option<isize>,
    key: impl FnOnce() -> KeyEvent,
) -> KeyEvent {
    if foreground != 0 && (foreground == game || Some(foreground) == overlay) { key() } else { KeyEvent::Outside }
}

/// Which modifiers the keyboard hook has seen held, in `uiohook`'s `ctrlKey`/`shiftKey`/
/// `altKey` shape.
///
/// The hook reports every key's down and up, so this is its own bookkeeping rather than
/// `GetAsyncKeyState`'s snapshot (which can lag the key event it belongs to). The mouse hook
/// passes the same state along with its moves, as `uiohook`'s mouse events carry the keyboard
/// modifiers.
/// One wheel tick's `rotation`, as `uiohook-napi`'s libuiohook computes it on Windows
/// (`get_scroll_wheel_rotation`, vertical): the hook's signed delta (`WHEEL_DELTA`, 120, per
/// notch) is added to what earlier ticks left over, the whole notches are reported with the
/// sign inverted ("to conform with other platforms": positive is toward the user, scrolling
/// down), and the remainder is kept for the next tick. A high-resolution wheel's small deltas
/// therefore make no rotation until they add up to a notch. Same 16-bit arithmetic as the C.
#[cfg(any(windows, test))]
pub(crate) const fn wheel_rotation(leftover: &mut i16, delta: i16) -> i16 {
    *leftover = leftover.wrapping_add(delta);
    let rotation = *leftover / -120;
    *leftover %= 120;
    rotation
}

#[cfg(any(windows, test))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ModifierState {
    ctrl: bool,
    shift: bool,
    alt: bool,
}

#[cfg(any(windows, test))]
impl ModifierState {
    /// One key event: the modifier that key names goes down or up; any other key changes
    /// nothing. `vk` is `KBDLLHOOKSTRUCT.vkCode`; left and right variants name the same key.
    pub(crate) fn set(&mut self, vk: u32, down: bool) {
        match wraex_core::keys::vk_to_key(vk) {
            Some("Ctrl") => self.ctrl = down,
            Some("Shift") => self.shift = down,
            Some("Alt") => self.alt = down,
            _ => {}
        }
    }

    pub(crate) const fn ctrl(self) -> bool {
        self.ctrl
    }

    pub(crate) const fn alt(self) -> bool {
        self.alt
    }

    /// The modifiers as the [`InputEvent`] for `key`.
    pub(crate) const fn input(self, key: Option<&'static str>) -> InputEvent {
        InputEvent { key, ctrl: self.ctrl, shift: self.shift, alt: self.alt }
    }
}

/// What the OS layer reports about the game window: the half of
/// `electron-overlay-window`'s event stream and `uiohook`'s input hook that this
/// state machine needs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GameWindowEvents {
    /// The game's window became the foreground window (`OverlayController`'s
    /// `focus` event).
    Focus,
    /// The game's window lost the foreground (`blur`).
    Blur,
    /// `electron-overlay-window` attached to the window. `has_access` is
    /// `Some(false)` when the game runs elevated and cannot be reached, `None`
    /// when the overlay library did not report it.
    Attach { has_access: Option<bool> },
    /// The window moved or was resized; this is also the first bounds report.
    Bounds(Bounds),
    /// The tracked window was destroyed (`electron-overlay-window`'s `detach`). The host
    /// forgets it and waits for a new one; finding it again reports `Attach` and `Focus`.
    Detach,
}

/// What a [`GameWindowEvents`] input changed, for the host to act on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WindowEvent {
    /// `active-change`: the game gained or lost focus. Only emitted on a change.
    ActiveChanged { is_active: bool },
    /// `onAttach`: the overlay attached (or was refused).
    Attached { has_access: Option<bool> },
    /// The tracked rectangle moved or was resized.
    BoundsChanged { bounds: Bounds },
    /// The game's window is gone (closed, crashed, restarted). `was_active` says whether
    /// it had the focus, so the host can report that loss like a blur.
    Detached { was_active: bool },
}

/// The game's window as the host sees it (`GameWindow.ts`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameWindow {
    is_active: bool,
    is_tracking: bool,
    bounds: Option<Bounds>,
}

impl GameWindow {
    pub fn new() -> Self {
        Self::default()
    }

    /// `this._isActive`: whether the game window has focus. Hotkeys are registered
    /// only while this is true.
    pub const fn is_active(&self) -> bool {
        self.is_active
    }

    /// `this._isTracking`: `attach()` ran, so the OS layer owns a window hook.
    pub const fn is_tracking(&self) -> bool {
        self.is_tracking
    }

    /// `this.bounds` (`OverlayController.targetBounds`); `None` until the OS
    /// layer reports the window's rectangle.
    pub const fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    /// `this.uiSidebarWidth`: the game's UI sidebar is 370 px wide in a 600 px
    /// tall window (the scale `game-ui-root` uses), so it scales with the
    /// window's height.
    pub fn ui_sidebar_width(&self) -> Option<f64> {
        Some((self.bounds?.height * 370.0 / 600.0).round())
    }

    /// Feeds one event from the OS layer. `None` means nothing changed: a
    /// repeated focus report, or an identical rectangle.
    pub fn feed(&mut self, event: GameWindowEvents) -> Option<WindowEvent> {
        match event {
            GameWindowEvents::Focus => self.set_active(true),
            GameWindowEvents::Blur => self.set_active(false),
            GameWindowEvents::Attach { has_access } => {
                // The OS layer reports this when it finds the window, so a second report
                // means it found it again (a restart, or another window with the title) and
                // upstream's `onAttach` forwards every one.
                self.is_tracking = true;
                Some(WindowEvent::Attached { has_access })
            }
            GameWindowEvents::Detach => {
                if !self.is_tracking {
                    return None;
                }
                self.is_tracking = false;
                self.bounds = None;
                Some(WindowEvent::Detached { was_active: std::mem::take(&mut self.is_active) })
            }
            GameWindowEvents::Bounds(bounds) => {
                if self.bounds == Some(bounds) {
                    return None;
                }
                self.bounds = Some(bounds);
                Some(WindowEvent::BoundsChanged { bounds })
            }
        }
    }

    /// `poeWindow.isActive = active`. The overlay controller uses it to take
    /// focus away from the game when the overlay becomes interactive
    /// ([`crate::overlay::OverlayController::assert_overlay_active`]) and to give
    /// it back. Returns the change, like the `active-change` event does.
    pub const fn set_active(&mut self, is_active: bool) -> Option<WindowEvent> {
        if self.is_active == is_active {
            return None;
        }
        self.is_active = is_active;
        Some(WindowEvent::ActiveChanged { is_active })
    }
}

/// Where the game's window is looked for: the config's `windowTitle` plus an optional
/// window class.
///
/// That is upstream's `OverlayWindow.updateOpts(overlayKey, windowTitle)` →
/// `GameWindow.attach`; `electron-overlay-window` compares the title exactly (`strcmp`).
///
/// The reference never passes a class: it matches the title alone. PoE's own window classes
/// are not documented anywhere in either reference, so Wraex does not guess one; a class
/// is only compared when a caller sets it, to disambiguate a title another application
/// shares. The Win32 backend logs the class it found, so it can be filled in from a real
/// game window.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WindowTarget {
    pub title: String,
    pub class: Option<String>,
}

impl WindowTarget {
    pub fn new(title: impl Into<String>) -> Self {
        Self { title: title.into(), class: None }
    }

    /// [`WindowTarget::new`] with a window class to require as well.
    pub fn with_class(title: impl Into<String>, class: impl Into<String>) -> Self {
        Self { title: title.into(), class: Some(class.into()) }
    }
}

/// Whether a top-level window is the game's: its title exactly, and its class when the
/// target names one (`electron-overlay-window`'s `strcmp` on the title; the class is
/// Wraex's addition, see [`WindowTarget`]).
pub fn matches_target(target: &WindowTarget, title: &str, class: Option<&str>) -> bool {
    if title != target.title {
        return false;
    }
    target.class.as_ref().is_none_or(|expected| class == Some(expected.as_str()))
}

/// Called when [`Watcher`] queued events, so a threaded app's poll loop wakes up. Rune's
/// `AppHandle::wake` is one; the platform crate does not depend on Rune.
pub type WakeFn = Box<dyn Fn() + Send + Sync>;

/// Why the OS layer cannot follow the game's window.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchError {
    /// This build has no window hook: macOS, X11 and Wayland are follow-ups (see the
    /// module's documentation).
    Unsupported(&'static str),
    /// The hook thread could not be started.
    Thread(String),
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "game window tracking is not implemented here yet ({what})"),
            Self::Thread(error) => write!(f, "starting the window hook failed: {error}"),
        }
    }
}

impl std::error::Error for WatchError {}

/// The OS layer that finds the game's window and reports what happens to it, behind
/// [`Watcher`] on every platform:
///
/// - [`Watcher::start`] looks for `target` and keeps following it: the foreground changes,
///   moves/resizes, the window being destroyed. Each report is queued as a
///   [`GameWindowEvents`] for the app's poll loop to feed [`GameWindow`].
/// - [`Watcher::drain`] takes what happened since the last call.
/// - [`Watcher::drain_mouse`] takes the [`MouseEvent`]s the global low-level mouse hook queued
///   (`uiohook`'s `mousemove`/`mousedown`/`wheel`). [`Watcher::set_mouse_moves`] turns off the
///   flood of move events while nothing tracks the mouse (a wheel still arrives).
/// - [`Watcher::drain_keys`] takes the [`KeyEvent`]s the global low-level keyboard hook queued
///   (`uiohook`'s `keydown`/`keyup`): the Alt peek and the key log. Keys the app synthesizes
///   itself are flagged injected and are not reported, so they cannot toggle the peek.
/// - [`Watcher::hwnd`] is the tracked window's native handle, for [`focus_window`] and the
///   other calls that take one.
///
/// Only the Win32 backend exists (see the module's documentation); elsewhere
/// [`Watcher::start`] returns [`WatchError::Unsupported`].
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrelated_foreground_keys_never_reach_diagnostics() {
        let key = InputEvent { key: Some("S"), alt: false, ctrl: false, shift: false };
        for event in [KeyEvent::Down(key), KeyEvent::Up(key)] {
            for foreground in [1, 3, 0, 2] {
                let captured = scoped_key_event(foreground, 1, Some(2), || {
                    assert!(matches!(foreground, 1 | 2), "external key was decoded");
                    event
                });
                assert_eq!(captured, if matches!(foreground, 1 | 2) { event } else { KeyEvent::Outside });
            }
        }
    }

    /// libuiohook's sign: Windows reports a notch away from the user (scrolling up) as +120,
    /// `uiohook` as rotation -1, and upstream's `Shortcuts.ts` turns negative into `ArrowLeft`
    /// (#13: wheel down goes right, up goes left).
    #[test]
    fn a_wheel_notch_is_inverted_as_uiohook_reports_it() {
        let mut leftover = 0;
        assert_eq!(wheel_rotation(&mut leftover, 120), -1, "up, away from the user");
        assert_eq!(wheel_rotation(&mut leftover, -120), 1, "down, toward the user");
        assert_eq!(wheel_rotation(&mut leftover, -240), 2, "two notches at once");
        assert_eq!(leftover, 0);
    }

    /// A high-resolution wheel's partial deltas add up to a notch before any rotation.
    #[test]
    fn partial_wheel_deltas_wait_for_a_whole_notch() {
        let mut leftover = 0;
        assert_eq!(wheel_rotation(&mut leftover, -40), 0);
        assert_eq!(wheel_rotation(&mut leftover, -40), 0);
        assert_eq!(wheel_rotation(&mut leftover, -40), 1, "three thirds of a notch down");
        assert_eq!(leftover, 0);
        assert_eq!(wheel_rotation(&mut leftover, -100), 0);
        assert_eq!(wheel_rotation(&mut leftover, 60), 0, "turning back undoes the leftover");
        assert_eq!(leftover, -40);
    }

    const RECT: Bounds = Bounds { x: 100.0, y: 50.0, width: 1600.0, height: 1200.0 };

    #[test]
    fn focus_changes_are_reported_once() {
        let mut window = GameWindow::new();
        assert!(!window.is_active());
        assert_eq!(window.feed(GameWindowEvents::Focus), Some(WindowEvent::ActiveChanged { is_active: true }));
        assert!(window.is_active());
        // `set isActive` emits only when the value changes.
        assert_eq!(window.feed(GameWindowEvents::Focus), None);
        assert_eq!(window.feed(GameWindowEvents::Blur), Some(WindowEvent::ActiveChanged { is_active: false }));
        assert_eq!(window.feed(GameWindowEvents::Blur), None);
        assert!(!window.is_active());
    }

    #[test]
    fn set_active_is_the_overlays_hand_on_the_focus() {
        let mut window = GameWindow::new();
        assert_eq!(window.set_active(false), None, "already inactive");
        assert_eq!(window.set_active(true), Some(WindowEvent::ActiveChanged { is_active: true }));
        assert!(window.is_active());
        // ... and the Shortcuts registration follows the same value.
        assert_eq!(window.set_active(true), None);
    }

    #[test]
    fn attach_is_reported_every_time_the_window_is_found_again() {
        let mut window = GameWindow::new();
        assert!(!window.is_tracking());
        assert_eq!(
            window.feed(GameWindowEvents::Attach { has_access: Some(true) }),
            Some(WindowEvent::Attached { has_access: Some(true) })
        );
        assert!(window.is_tracking());
        // Found again: the OS layer only reports an attach when it has a window, and the
        // renderer is told each time (upstream's `onAttach` forwards every one).
        assert_eq!(
            window.feed(GameWindowEvents::Attach { has_access: Some(true) }),
            Some(WindowEvent::Attached { has_access: Some(true) })
        );

        assert_eq!(window.bounds(), None);
        assert_eq!(window.feed(GameWindowEvents::Bounds(RECT)), Some(WindowEvent::BoundsChanged { bounds: RECT }));
        assert_eq!(window.bounds(), Some(RECT));
        assert_eq!(window.feed(GameWindowEvents::Bounds(RECT)), None);
    }

    #[test]
    fn a_destroyed_window_stops_tracking_and_drops_its_rectangle() {
        let mut window = GameWindow::new();
        assert_eq!(window.feed(GameWindowEvents::Detach), None, "nothing was tracked");
        window.feed(GameWindowEvents::Attach { has_access: Some(true) });
        window.feed(GameWindowEvents::Bounds(RECT));
        window.feed(GameWindowEvents::Focus);
        assert_eq!(
            window.feed(GameWindowEvents::Detach),
            Some(WindowEvent::Detached { was_active: true }),
            "the game had the focus when it closed"
        );
        assert!(!window.is_tracking());
        assert!(!window.is_active());
        assert_eq!(window.bounds(), None);

        // The game restarts: a new attach and focus, as upstream's overlay library does.
        assert!(window.feed(GameWindowEvents::Attach { has_access: Some(true) }).is_some());
        assert!(window.is_tracking());
        assert_eq!(window.feed(GameWindowEvents::Detach), Some(WindowEvent::Detached { was_active: false }));
    }

    #[test]
    fn the_target_matches_its_title_exactly() {
        let target = WindowTarget::new("Path of Exile 2");
        assert!(matches_target(&target, "Path of Exile 2", None));
        assert!(matches_target(&target, "Path of Exile 2", Some("whatever")));
        // `electron-overlay-window` compares with `strcmp`: no trimming, no partial match.
        assert!(!matches_target(&target, "Path of Exile 2 ", None));
        assert!(!matches_target(&target, "Path of Exile", None));
        assert!(!matches_target(&target, "path of exile 2", None));
        assert!(!matches_target(&target, "", None));
    }

    #[test]
    fn a_target_with_a_class_requires_it() {
        let target = WindowTarget::with_class("Path of Exile", "ApExWindowClass");
        assert!(matches_target(&target, "Path of Exile", Some("ApExWindowClass")));
        assert!(!matches_target(&target, "Path of Exile", Some("Chrome_WidgetWin_1")));
        // A window whose class cannot be read is not a match for a class-qualified target.
        assert!(!matches_target(&target, "Path of Exile", None));
        // The title is still the first gate.
        assert!(!matches_target(&target, "Other", Some("ApExWindowClass")));
    }

    #[test]
    fn the_sidebar_width_scales_with_the_window_height() {
        let mut window = GameWindow::new();
        assert_eq!(window.ui_sidebar_width(), None, "no bounds yet");
        window.feed(GameWindowEvents::Bounds(RECT));
        // 370 / 600 of 1200 px, `Math.round`ed like upstream.
        assert_eq!(window.ui_sidebar_width(), Some(740.0));
        window.feed(GameWindowEvents::Bounds(Bounds { height: 601.0, ..RECT }));
        assert_eq!(window.ui_sidebar_width(), Some(371.0), "370.6 rounds to 371");
    }

    #[test]
    fn points_are_strictly_inside_a_rectangle() {
        let area = Bounds { x: 0.0, y: 0.0, width: 10.0, height: 10.0 };
        assert!(Point { x: 5.0, y: 5.0 }.is_inside(area));
        assert!(!Point { x: 0.0, y: 5.0 }.is_inside(area));
        assert!(!Point { x: 5.0, y: 10.0 }.is_inside(area));
        assert_eq!(Point { x: 3.0, y: 4.0 }.distance_to(Point { x: 0.0, y: 0.0 }), 5.0);
    }

    #[test]
    fn the_keyboard_hook_tracks_the_modifiers_it_reports() {
        let mut keys = ModifierState::default();
        assert_eq!(keys.input(None), InputEvent { key: None, ctrl: false, shift: false, alt: false });

        // `VK_MENU` (18) is the Alt a low-level hook reports; `VK_LMENU` (0xA4) is the same key.
        keys.set(0x12, true);
        assert!(keys.alt(), "Alt down is the state the mouse hook reads");
        assert!(keys.input(Some("Alt")).alt, "Alt down shows in the event");
        keys.set(0x12, false);
        assert!(!keys.input(Some("Alt")).alt, "and Alt up clears it, so `!e.altKey` is true");

        // A letter changes no modifier: only the modifier it names goes down or up.
        keys.set(0x44, true);
        assert_eq!(keys.input(Some("D")), InputEvent { key: Some("D"), ctrl: false, shift: false, alt: false });

        keys.set(0x10, true); // VK_SHIFT
        keys.set(0xA2, true); // VK_LCONTROL
        assert_eq!(keys.input(Some("D")), InputEvent { key: Some("D"), ctrl: true, shift: true, alt: false });
        keys.set(0xA3, false); // VK_RCONTROL up: the plain Ctrl flag goes with it
        assert!(!keys.ctrl());
        keys.set(0xA1, false); // VK_RSHIFT up
        assert_eq!(keys.input(None), InputEvent { key: None, ctrl: false, shift: false, alt: false });
    }
}
