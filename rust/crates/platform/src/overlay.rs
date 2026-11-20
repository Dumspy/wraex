//! The transparent, click-through overlay that follows the game window.
//!
//! Upstream: `electron-overlay-window` and `OverlayController`. Rune provides the
//! overlay surface and a click-through toggle; following the game window is ours to
//! write per platform (Win32 `SetWinEventHook`, X11, Wayland layer-shell).
