//! `web/settings/general.vue`, for both games: EE2 adds the preferred trade site, its two
//! overlay-close boxes, Client.txt reading and the experimental-features box.

use rune::{El, col, row, text};
use serde_json::Value;
use wraex_core::GameId;

use crate::settings::pages::{Limit, NumberField, TextField, checkbox, label, number_field, page, text_field, tr};
use crate::settings::{Field, Project, Settings};
use crate::theme::{self, text as type_};
use crate::ui;
use crate::ui::field::Kind;

/// The languages each reference offers, in its `<select>`'s order.
const LANGUAGES_POE1: [(&str, &str); 4] =
    [("en", "English"), ("ru", "Русский"), ("cmn-Hant", "正體中文"), ("ko", "한국어")];
const LANGUAGES_POE2: [(&str, &str); 9] = [
    ("en", "English"),
    ("ru", "Русский"),
    ("cmn-Hant", "正體中文"),
    ("ko", "한국어"),
    ("ja", "日本語"),
    ("de", "Deutsch"),
    ("es", "Español"),
    ("pt", "Português (Brasil)"),
    ("fr", "Français"),
];

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let game = settings.game;
    let language = settings.string(Field::Root("language"));
    let realm = settings.string(Field::Root("realm"));

    let mut page = page("page-general").child(language_block(settings, &language, project));

    if game == GameId::Poe2 && language != "en" {
        page = page.child(preferred_trade_site(settings, &language, &realm, project));
    }

    if language == "cmn-Hant" {
        page = page.child(realm_block(settings, &realm, project));
    }

    // APT only: the international-site box, forced on for a Hant client on the global realm.
    if game == GameId::Poe1 && language != "en" && realm == "pc-ggg" {
        page = page.child(intl_site(settings, &language, project));
    }

    for block in general_options(settings, project) {
        page = page.child(block);
    }
    page
}

/// The language `<select>`: `select.rounded bg-gray-700 w-24`, 6 rem wide.
fn language_block<S: 'static>(settings: &Settings, language: &str, project: Project<S>) -> El {
    let languages: &[(&str, &str)] = if settings.game == GameId::Poe2 { &LANGUAGES_POE2 } else { &LANGUAGES_POE1 };
    let choices: Vec<ui::select::Choice> =
        languages.iter().map(|(value, name)| ui::select::Choice::new(*value, *name)).collect();
    col()
        .key("general-language")
        .mb(theme::rem(0.5))
        .child(label("general-language-label", &tr(settings, "settings.language")))
        .child(ui::select::select(
            &ui::select::SelectProps {
                key: "general-language-select",
                choices: &choices,
                value: language,
                open: settings.select_open == Some("general-language-select"),
                width: theme::rem(6.0),
            },
            move |s: &mut S| toggle_select(project, s, "general-language-select"),
            move |s: &mut S, value: String| pick_language(project(s), &value),
        ))
}

/// EE2: `preferredTradeSite`, for any language but English.
fn preferred_trade_site<S: 'static>(settings: &Settings, language: &str, realm: &str, project: Project<S>) -> El {
    let trade_url = trade_url(language, realm);
    let choices =
        vec![ui::select::Choice::new("default", trade_url), ui::select::Choice::new("www", "www.pathofexile.com")];
    col()
        .key("general-preferred-trade-site")
        .mb(theme::rem(1.0))
        .child(label("general-preferred-trade-site-label", &tr(settings, "settings.preferred_trade_site")))
        .child(ui::select::select(
            &ui::select::SelectProps {
                key: "general-preferred-trade-site-select",
                choices: &choices,
                value: &settings.string(Field::Root("preferredTradeSite")),
                open: settings.select_open == Some("general-preferred-trade-site-select"),
                width: theme::rem(6.0),
            },
            move |s: &mut S| toggle_select(project, s, "general-preferred-trade-site-select"),
            move |s: &mut S, value: String| {
                project(s).set_string(Field::Root("preferredTradeSite"), &value);
                project(s).select_open = None;
            },
        ))
}

/// The Hant client's realm radios.
fn realm_block<S: 'static>(settings: &Settings, realm: &str, project: Project<S>) -> El {
    col().key("general-realm").mb(theme::rem(0.5)).child(label("general-realm-label", &tr(settings, "realm"))).child(
        row()
            .gap(theme::rem(1.0))
            .child(ui::radio(
                ui::RadioProps::new("general-realm-pc-ggg", &tr(settings, "realm_intl")).checked(realm == "pc-ggg"),
                move |s: &mut S| pick_realm(project(s), "pc-ggg"),
            ))
            .child(ui::radio(
                ui::RadioProps::new("general-realm-pc-garena", &tr(settings, "Hotcool")).checked(realm == "pc-garena"),
                move |s: &mut S| pick_realm(project(s), "pc-garena"),
            )),
    )
}

/// APT's international-site box, forced on for a Hant client on the global realm.
fn intl_site<S: 'static>(settings: &Settings, language: &str, project: Project<S>) -> El {
    let forced = language == "cmn-Hant";
    let checked = settings.bool(Field::Root("useIntlSite"));
    row().key("general-use-intl-site-row").mb(theme::rem(1.0)).child(
        row()
            .key("general-use-intl-site")
            .gap(theme::rem(0.25))
            .items_center()
            .child(ui::checkbox(
                ui::CheckboxProps::new("general-use-intl", &tr(settings, "settings.use_intl_site"))
                    .checked(checked)
                    .disabled(forced),
                move |s: &mut S| {
                    let checked = project(s).bool(Field::Root("useIntlSite"));
                    project(s).set_bool(Field::Root("useIntlSite"), !checked);
                },
            ))
            .child(ui::field::badge("general-use-intl-site-name", "www.pathofexile.com")),
    )
}

/// The settings both games share after the language block, in the template's order.
fn general_options<S: 'static>(settings: &Settings, project: Project<S>) -> Vec<El> {
    let mut blocks = vec![font_size_block(settings, project), restore_clipboard(settings, project)];
    blocks.extend(file_blocks(settings, project));
    // `<hr class="mb-4 mx-8 border-gray-700">`
    blocks.push(
        row().key("general-rule").h(1.0).mx(theme::rem(2.0)).mb(theme::rem(1.0)).fill(theme::gray(700)).border_layout(),
    );
    blocks.extend(overlay_options(settings, project));
    blocks.push(window_title_block(settings, project));
    if settings.game == GameId::Poe2 {
        blocks.extend(poe2_options(settings, project));
    }
    blocks
}

/// Font size: `w-16` number field, then `px`.
///
/// Upstream's `mt-4` collapses with the language block's `mb-2`; a Rune column adds margins
/// instead (see the drift note in the module docs), so the gap is 8 + 16 pt here.
fn font_size_block<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    col()
        .key("general-font-size")
        .my(theme::rem(1.0))
        .child(label("general-font-size-label", &tr(settings, "settings.font_size")))
        .child(
            row()
                .gap(theme::rem(0.25))
                .items_center()
                .child(number_field(
                    settings,
                    NumberField::new("general-font-size-input", Field::Root("fontSize"), Limit::Any),
                    project,
                ))
                .child(text("px").key("general-font-size-unit").font(type_::SM)),
        )
}

/// `restoreClipboard`.
fn restore_clipboard<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    row().key("general-restore-clipboard-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "general-restore-clipboard",
        Field::Root("restoreClipboard"),
        &tr(settings, "settings.restore_clipboard"),
        project,
    ))
}

/// The `Client.txt` and game-config paths.
fn file_blocks<S: 'static>(settings: &Settings, project: Project<S>) -> Vec<El> {
    let mut blocks = Vec::new();
    for (key, field, title_key, placeholder) in [
        (
            "general-client-log",
            "clientLog",
            "settings.poe_log_file",
            if settings.game == GameId::Poe2 {
                "...?/Grinding Gear Games/Path of Exile 2/logs/Client.txt"
            } else {
                "...?/Grinding Gear Games/Path of Exile/logs/Client.txt"
            },
        ),
        (
            "general-game-config",
            "gameConfig",
            "settings.poe_cfg_file",
            if settings.game == GameId::Poe2 {
                "...?/My Games/Path of Exile 2/poe2_production_Config.ini"
            } else {
                "...?/My Games/Path of Exile/production_Config.ini"
            },
        ),
    ] {
        // The log file's block is `mb-2`, the config file's `mb-4`.
        let margin = if key == "general-game-config" { theme::rem(1.0) } else { theme::rem(0.5) };
        blocks.push(
            col()
                .key(format!("{key}-field"))
                .mb(margin)
                .child(label(&format!("{key}-label"), &tr(settings, title_key)))
                .child(text_field(
                    settings,
                    TextField::new(key, Field::Root(field), Kind::Sans).placeholder(placeholder),
                    project,
                )),
        );
    }
    blocks
}

/// The overlay-close behaviour boxes.
///
/// Upstream's "Background when the overlay is clickable" colour and its Transparent radio are
/// left out: Wraex draws no overlay dim, so the colour had no effect (and the radio could not
/// be turned off again). `overlayBackground` stays in the config as read.
fn overlay_options<S: 'static>(settings: &Settings, project: Project<S>) -> Vec<El> {
    let mut blocks = vec![
        row().key("general-overlay-close-row").mb(theme::rem(0.5)).child(checkbox(
            settings,
            "general-overlay-background-close",
            Field::Root("overlayBackgroundClose"),
            &tr(settings, "settings.overlay_bg_focus_game"),
            project,
        )),
        row().key("general-attach-notification-row").mb(theme::rem(1.0)).child(checkbox(
            settings,
            "general-attach-notification",
            Field::Root("showAttachNotification"),
            &tr(settings, "settings.show_overlay_ready"),
            project,
        )),
    ];
    if settings.game == GameId::Poe2 {
        blocks.push(row().key("general-overlay-always-close-row").mb(theme::rem(1.0)).child(checkbox(
            settings,
            "general-overlay-always-close",
            Field::Root("overlayAlwaysClose"),
            &tr(settings, "settings.overlay_always_close"),
            project,
        )));
        blocks.push(row().key("general-hide-overlay-row").mb(theme::rem(1.0)).child(checkbox(
            settings,
            "general-hide-overlay-on-blur",
            Field::Root("hideOverlayOnBlur"),
            &tr(settings, "settings.hide_overlay_on_blur"),
            project,
        )));
        blocks.push(ui::field::hint("general-hide-overlay-hint", &tr(settings, "settings.hide_overlay_on_blur_hint")));
    }
    blocks
}

/// The window title, with the Restart required badge.
fn window_title_block<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    col()
        .key("general-window-title")
        .mb(theme::rem(1.0))
        .child(
            row()
                .key("general-window-title-row")
                .mb(theme::rem(0.25))
                .gap(theme::rem(0.25))
                .items_center()
                .child(label("general-window-title-label", &tr(settings, "settings.window_title")))
                .child(ui::field::badge("general-window-title-restart", "Restart required")),
        )
        .child(text_field(
            settings,
            TextField::new("general-window-title-input", Field::Root("windowTitle"), Kind::Text),
            project,
        ))
}

/// EE2's own boxes: Client.txt reading and the experimental features.
fn poe2_options<S: 'static>(settings: &Settings, project: Project<S>) -> Vec<El> {
    vec![
        row().key("general-read-client-log-row").mb(theme::rem(1.0)).child(checkbox(
            settings,
            "general-read-client-log",
            Field::Root("readClientLog"),
            &tr(settings, "settings.read_client_log"),
            project,
        )),
        ui::field::hint("general-client-log-explain", &tr(settings, "settings.client_log_explain")),
        alphas_block(settings, project),
    ]
}

/// `enableAlphas` and, while it is on, the listed experimental features (`alphas`).
fn alphas_block<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let alphas_enabled = settings.bool(Field::Root("enableAlphas"));
    let mut alphas = col().key("general-alphas").mb(theme::rem(1.0));
    if alphas_enabled {
        alphas = alphas.padding(theme::rem(0.5)).radius(theme::ROUNDED).fill(theme::slate(800));
    }
    alphas = alphas.child(row().key("general-enable-alphas-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "general-enable-alphas",
        Field::Root("enableAlphas"),
        &tr(settings, "settings.enable_alphas"),
        project,
    )));
    if alphas_enabled {
        let library = has_library(settings);
        alphas = alphas.child(row().key("general-library-alpha-row").child(ui::checkbox(
            ui::CheckboxProps::new("general-library-alpha", &tr(settings, "settings.alpha_library")).checked(library),
            move |s: &mut S| {
                let settings = project(s);
                let mut alphas: Vec<Value> = alphas_list(&settings.config).to_vec();
                if has_library(settings) {
                    alphas.retain(|alpha| !is_library(alpha));
                } else {
                    alphas.push(Value::String("library".to_owned()));
                }
                settings.set(Field::Root("alphas"), Value::Array(alphas));
            },
        )));
    }
    alphas
}

/// `config.alphas` (`AppConfig().alphas`), or the empty list.
fn alphas_list(config: &Value) -> &[Value] {
    match Field::Root("alphas").read(config) {
        Value::Array(alphas) => alphas,
        _ => &[],
    }
}

/// `alphas.includes('library')`
fn has_library(settings: &Settings) -> bool {
    alphas_list(&settings.config).iter().any(is_library)
}

/// Whether an `alphas` entry is the `'library'` flag.
fn is_library(alpha: &Value) -> bool {
    matches!(alpha, Value::String(value) if value == "library")
}

/// `preferredTradeSite`'s first option: the trade site the language's client uses
/// (`tradeUrl` in `settings.general`).
fn trade_url(language: &str, realm: &str) -> String {
    let host = match language {
        "ru" => "ru.pathofexile.com",
        "cmn-Hant" if realm == "pc-garena" => "pathofexile.tw",
        "ko" => "poe.kakaogames.com",
        "ja" => "jp.pathofexile.com",
        "de" => "de.pathofexile.com",
        "es" => "es.pathofexile.com",
        "pt" => "br.pathofexile.com",
        "fr" => "fr.pathofexile.com",
        _ => "www.pathofexile.com",
    };
    host.to_owned()
}

/// Open or close a `<select>`'s list.
fn toggle_select<S: 'static>(project: Project<S>, s: &mut S, key: &'static str) {
    let settings = project(s);
    settings.select_open = (settings.select_open != Some(key)).then_some(key);
    settings.recording = None;
}

/// `language`'s setter: a realm switch for Hant, and APT's `useIntlSite`.
fn pick_language(settings: &mut Settings, value: &str) {
    settings.set_string(Field::Root("language"), value);
    if value != "cmn-Hant" {
        settings.set_string(Field::Root("realm"), "pc-ggg");
    }
    if settings.game != GameId::Poe2 {
        let realm = settings.string(Field::Root("realm"));
        settings.set_bool(Field::Root("useIntlSite"), realm == "pc-ggg" && value == "cmn-Hant");
    }
    settings.select_open = None;
}

/// `realm`'s setter: APT's `useIntlSite` follows it.
fn pick_realm(settings: &mut Settings, value: &str) {
    settings.set_string(Field::Root("realm"), value);
    if settings.game != GameId::Poe2 {
        let language = settings.string(Field::Root("language"));
        settings.set_bool(Field::Root("useIntlSite"), value == "pc-ggg" && language == "cmn-Hant");
    }
}
