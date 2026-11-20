//! System-wide hotkeys and synthetic input.
//!
//! Upstream: `uiohook-napi` and Electron's `globalShortcut`, plus the key taps that
//! send Ctrl+C or Ctrl+Alt+C to the game. Candidates: `rdev`, `global-hotkey`,
//! `enigo`, or a low-level Win32 hook.
