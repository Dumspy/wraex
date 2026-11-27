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
//! - [`window`]: following the game window's focus and rectangle (`GameWindow.ts`), with the
//!   Win32 hook that finds and follows the window behind [`window::Watcher`].
//!
//! Choosing and writing the OS implementations is the phase 2 spike's job (PLAN.md): Win32
//! exists (the `windows` crate, `cfg(windows)` only); macOS, X11 and Wayland have no window
//! hook yet, so [`window::Watcher::start`] reports
//! [`window::WatchError::Unsupported`] there and the app keeps its ordinary windows.
//! [`http`] sends the trade API's requests, [`tray`] owns the system tray icon and
//! [`updater`] installs a downloaded setup after the app has quit.

pub mod clipboard;
pub mod http;
pub mod log;
pub mod overlay;
pub mod shortcuts;
pub mod time;
pub mod tray;
pub mod updater;
pub mod window;

#[cfg(test)]
mod test_support;
