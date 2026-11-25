//! `ref/ee2/renderer/src/web/leveling/settings-leveling.vue`: EE2's Leveling page.
//!
//! It repeats the general page's `Client.txt` fields (EE2 has them on both pages) and adds the
//! experience tracker's `showExp` switch with its two explanatory paragraphs. The
//! `PoE-Leveling-Guide` link is drawn as plain text: the settings pages have no external-link
//! control yet (EE2 opens it with `shell.openExternal`).

use rune::{El, row, text};

use crate::settings::pages::{checkbox, label, page, text_field, tr};
use crate::settings::{Field, Project, Settings};
use crate::theme::{self, text as type_};
use crate::ui;
use crate::ui::field::Kind;
use wraex_core::GameId;

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let mut page = page("page-leveling");

    // `{{ t(':enable_read_client_logs') }}`, `mb-4`.
    page = page.child(
        text(tr(settings, "leveling.enable_read_client_logs"))
            .key("leveling-enable-read-client-logs")
            .font(type_::SM)
            .mb(theme::rem(1.0))
            .wrap(),
    );

    // The log-file field, as `settings.general`'s (same config key, `v-model.trim`).
    let placeholder = if settings.game == GameId::Poe2 {
        "...?/Grinding Gear Games/Path of Exile 2/logs/Client.txt"
    } else {
        "...?/Grinding Gear Games/Path of Exile/logs/Client.txt"
    };
    page = page.child(
        rune::col()
            .key("leveling-client-log-field")
            .mb(theme::rem(0.5))
            .child(label("leveling-client-log-label", &tr(settings, "settings.poe_log_file")))
            .child(text_field(
                settings,
                "leveling-client-log",
                Field::Root("clientLog"),
                Kind::Sans,
                None,
                placeholder,
                project,
            )),
    );

    page = page.child(row().key("leveling-read-client-log-row").mb(theme::rem(0.25)).child(checkbox(
        settings,
        "leveling-read-client-log",
        Field::Root("readClientLog"),
        &tr(settings, "settings.read_client_log"),
        project,
    )));
    page = page.child(ui::field::hint("leveling-client-log-explain", &tr(settings, "settings.client_log_explain")));

    // `<div class="mb-4 border-b min-w-full" />`
    page = page.child(row().key("leveling-rule").h(1.0).mb(theme::rem(1.0)).fill(theme::gray(700)).border_layout());

    // `text-lg` heading, the TL;DR line, then the long explanation.
    page = page.child(
        text(tr(settings, "leveling.exp_tracking"))
            .key("leveling-exp-tracking")
            .font(type_::LG)
            .color(theme::gray(100)),
    );
    page = page.child(
        text(tr(settings, "leveling.exp_tracking_help_tldr")).key("leveling-exp-tracking-tldr").font(type_::SM).wrap(),
    );
    page = page.child(ui::field::hint("leveling-exp-tracking-help", &tr(settings, "leveling.exp_tracking_help")));
    page = page.child(ui::field::hint(
        "leveling-exp-inspire",
        &format!("{}PoE-Leveling-Guide", tr(settings, "leveling.exp_inspire")),
    ));

    page = page.child(row().key("leveling-show-exp-row").mb(theme::rem(0.5)).child(checkbox(
        settings,
        "leveling-show-exp",
        Field::Widget("experience-tracker", "showExp"),
        &tr(settings, "leveling.show_exp"),
        project,
    )));
    page.child(ui::field::hint("leveling-exp-astrict", &tr(settings, "leveling.exp_astrict")))
}
