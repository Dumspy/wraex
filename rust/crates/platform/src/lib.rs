//! Native platform services that upstream splits between Electron
//! (`electron-overlay-window`, `uiohook-napi`, `HostClipboard.ts`) and the renderer.
//!
//! The platform-independent half of the host process is ported here as state
//! machines, each with the OS effects it needs behind a small trait:
//!
//! - [`shortcuts`]: which hotkeys register, and the key sequences one press sends
//!   (`Shortcuts.ts`, `text-box.ts`).
//! - [`clipboard`]: reading item text after a copy and restoring the previous
//!   clipboard (`HostClipboard.ts`).
//! - [`overlay`]: the overlay's interactive/click-through state and the events it
//!   sends the renderer (`OverlayWindow.ts`, `OverlayVisibility.ts`,
//!   `WidgetAreaTracker.ts`).
//! - [`window`]: following the game window's focus and rectangle (`GameWindow.ts`).
//!
//! Choosing and writing the OS implementations — Win32, X11, Wayland, macOS — is
//! the phase 2 spike's job (PLAN.md), so this crate names no platform crate for
//! them; [`http`] sends the trade API's requests and [`tray`] owns the system tray
//! icon.

pub mod clipboard;
pub mod http;
pub mod overlay;
pub mod shortcuts;
pub mod tray;
pub mod window;

#[cfg(test)]
mod test_support;
