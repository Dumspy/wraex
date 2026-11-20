//! Native platform services that upstream splits between Electron
//! (`electron-overlay-window`, `uiohook-napi`, `HostClipboard.ts`) and the renderer.
//!
//! Nothing is implemented yet: phase 2 (the Windows spike) picks the crates and
//! proves that window tracking, click-through, system-wide hotkeys and clipboard
//! round-tripping actually work before any feature is ported on top of them.

pub mod clipboard;
pub mod hotkeys;
pub mod overlay;
pub mod window;
