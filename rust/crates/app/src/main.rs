//! The Wraex desktop app's entry point; the app itself is the `wraex_app` library.

// A release build on Windows is a GUI program, as upstream's Electron app is: started from a
// shortcut or Explorer it opens no console window, and so no taskbar button beside the
// overlay. The log goes to `wraex.log` beside the settings (`wraex_platform::log`); a debug
// build keeps the console for `cargo run`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() {
    wraex_app::main();
}
