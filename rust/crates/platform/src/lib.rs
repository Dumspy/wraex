//! Native platform services that upstream splits between Electron
//! (`electron-overlay-window`, `uiohook-napi`, `HostClipboard.ts`) and the renderer.
//!
//! Window tracking, hotkeys, overlay and clipboard are not implemented yet:
//! phase 2 (the Windows spike) picks the crates and proves they work before any
//! feature is ported on top of them. [`http`] sends the trade API's requests.

pub mod clipboard;
pub mod hotkeys;
pub mod http;
pub mod overlay;
pub mod window;
