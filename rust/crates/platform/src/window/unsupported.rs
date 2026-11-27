//! The window hook on platforms without one.
//!
//! macOS, X11 and Wayland are follow-ups (see the module's documentation): the shell keeps its
//! ordinary windows there, and every call that would move, style or focus a window is a
//! no-op. A stub rather than `cfg`s in the caller, so the shell's wiring stays one code path.

use super::{Bounds, CapturedKey, GameWindowEvents, KeyEvent, MouseEvent, WakeFn, WatchError, WindowTarget};

/// Nothing to follow: [`Watcher::start`] always fails with [`WatchError::Unsupported`].
#[derive(Debug)]
pub struct Watcher;

impl Watcher {
    pub fn start(_target: WindowTarget, _wake: WakeFn) -> Result<Self, WatchError> {
        Err(WatchError::Unsupported("only the Win32 backend exists so far"))
    }

    pub fn drain(&mut self) -> Vec<GameWindowEvents> {
        Vec::new()
    }

    pub fn drain_mouse(&mut self) -> Vec<MouseEvent> {
        Vec::new()
    }

    pub fn drain_keys(&mut self) -> Vec<KeyEvent> {
        Vec::new()
    }

    pub fn set_mouse_moves(&self, _enabled: bool) {}

    /// Nothing to arm: without a keyboard hook the quick-math bar cannot swallow the game's
    /// keys, so it takes the focus directly on these platforms instead (issue #6).
    pub fn set_capture_typing(&self, _armed: bool) {}

    pub fn hwnd(&self) -> Option<isize> {
        None
    }
}

pub fn place_overlay(_hwnd: isize, _bounds: Bounds) -> bool {
    false
}

pub fn configure_overlay(_hwnd: isize) {}

pub fn focus_window(_hwnd: isize) -> bool {
    false
}

/// Nothing to type into: no hook swallowed any key here (see
/// [`Watcher::set_capture_typing`]).
pub fn retype(_keys: &[CapturedKey]) {}

pub fn dpi_scale(_hwnd: isize) -> f64 {
    1.0
}
