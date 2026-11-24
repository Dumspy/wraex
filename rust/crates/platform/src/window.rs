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
//! What stays OS-specific, and is ours to write because there is no Rust
//! equivalent to `electron-overlay-window` (PLAN.md, phase 2): finding the game's
//! window by title/class, and the move/resize/focus hooks. Win32:
//! `SetWinEventHook` with `EVENT_OBJECT_FOCUS`/`EVENT_SYSTEM_FOREGROUND` and
//! `EVENT_OBJECT_LOCATIONCHANGE`, plus `DwmGetWindowAttribute`/`GetClientRect` for
//! the borderless-windowed client area. X11: `_NET_ACTIVE_WINDOW` and
//! `ConfigureNotify`. macOS: the Accessibility API (`AXUIElement`), which needs
//! the user's permission — EE2's `main.ts` asks for it and quits without it.
//! Wayland has no cross-application window API at all, which PLAN.md notes as a
//! known loss. Candidate crates: `windows`, `x11rb` and `objc2`; PLAN.md leaves
//! the choice to the spike, so this module chooses none. Rune supplies the overlay
//! surface and a raw window handle ([`crate::overlay::OverlayHandle`]).
//!
//! Reading the game's pixels (`OverlayController.screenshot()`, the heist gem
//! finder's input) is OS work too — Win32 `BitBlt`, X11 `XGetImage`, and not
//! permitted at all on Wayland — and stays there with the same candidates.

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
    pub fn distance_to(self, from: Point) -> f64 {
        (self.x - from.x).hypot(self.y - from.y)
    }

    /// `isPointInsideRect`: strictly inside, edges excluded.
    pub fn is_inside(self, area: Bounds) -> bool {
        self.x > area.x && self.x < area.x + area.width && self.y > area.y && self.y < area.y + area.height
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
    pub fn is_active(&self) -> bool {
        self.is_active
    }

    /// `this._isTracking`: `attach()` ran, so the OS layer owns a window hook.
    pub fn is_tracking(&self) -> bool {
        self.is_tracking
    }

    /// `this.bounds` (`OverlayController.targetBounds`); `None` until the OS
    /// layer reports the window's rectangle.
    pub fn bounds(&self) -> Option<Bounds> {
        self.bounds
    }

    /// `this.uiSidebarWidth`: the game's UI sidebar is 370 px wide in a 600 px
    /// tall window (the scale `game-ui-root` uses), so it scales with the
    /// window's height.
    pub fn ui_sidebar_width(&self) -> Option<f64> {
        Some((self.bounds?.height * 370.0 / 600.0).round())
    }

    /// Feeds one event from the OS layer. `None` means nothing changed: a
    /// repeated focus report, a second attach (upstream attaches once), or an
    /// identical rectangle.
    pub fn feed(&mut self, event: GameWindowEvents) -> Option<WindowEvent> {
        match event {
            GameWindowEvents::Focus => self.set_active(true),
            GameWindowEvents::Blur => self.set_active(false),
            GameWindowEvents::Attach { has_access } => {
                // `attach()` is a no-op once tracking started.
                if self.is_tracking {
                    return None;
                }
                self.is_tracking = true;
                Some(WindowEvent::Attached { has_access })
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
    pub fn set_active(&mut self, is_active: bool) -> Option<WindowEvent> {
        if self.is_active == is_active {
            return None;
        }
        self.is_active = is_active;
        Some(WindowEvent::ActiveChanged { is_active })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn attach_happens_once_and_moving_bounds_is_reported() {
        let mut window = GameWindow::new();
        assert!(!window.is_tracking());
        assert_eq!(
            window.feed(GameWindowEvents::Attach { has_access: Some(true) }),
            Some(WindowEvent::Attached { has_access: Some(true) })
        );
        assert!(window.is_tracking());
        assert_eq!(window.feed(GameWindowEvents::Attach { has_access: Some(false) }), None);

        assert_eq!(window.bounds(), None);
        assert_eq!(window.feed(GameWindowEvents::Bounds(RECT)), Some(WindowEvent::BoundsChanged { bounds: RECT }));
        assert_eq!(window.bounds(), Some(RECT));
        assert_eq!(window.feed(GameWindowEvents::Bounds(RECT)), None);
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
}
