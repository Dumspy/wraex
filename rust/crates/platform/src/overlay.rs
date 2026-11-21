//! The transparent, click-through overlay that follows the game window.
//!
//! Upstream: `electron-overlay-window` and its `OverlayController`, driven from
//! `main/src/windowing/OverlayWindow.ts`. Rune provides the overlay surface and a
//! click-through toggle; following the game window is ours to write per platform
//! (Win32 `SetWinEventHook`, X11, Wayland layer-shell).
