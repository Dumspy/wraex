//! `web/settings/debug.vue`: the log-keys toggle over the host's log tail.
//!
//! The app has no log buffer yet (logging comes with the tray/updater wiring), so the
//! `<pre>` is drawn empty where upstream prints `Host.logs`.

use rune::{El, col, row, text};

use crate::settings::pages::{checkbox, tr};
use crate::settings::{Field, Project, Settings};
use crate::theme::{self, text as type_};

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    col()
        .key("page-debug")
        .w_full()
        .h_full()
        .min_h(0.0)
        .child(row().key("debug-options").w_full().padding(theme::rem(0.5)).child(checkbox(
            settings,
            "debug-log-keys",
            Field::Root("logKeys"),
            &tr(settings, "settings.debug_hotkeys"),
            project,
        )))
        .child(row().key("debug-logs").w_full().grow(1.0).min_h(0.0).padding(theme::rem(0.5)).items_start().child(
            text(String::new()).key("debug-logs-text").font(type_::SM.family(theme::FAMILY_EXO2)).by_line().grow(1.0),
        ))
}
