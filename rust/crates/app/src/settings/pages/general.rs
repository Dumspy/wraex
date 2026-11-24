//! `web/settings/general.vue`, for both games: EE2 adds the preferred trade site, its two
//! overlay-close boxes, Client.txt reading and the experimental-features box.

use rune::{El, col, row, text};
use wraex_core::GameId;

use crate::settings::pages::{Limit, checkbox, label, number_field, page, text_field, tr};
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

/// `'rgba(255, 255, 255, 0)'`: the Transparent radio's value.
const TRANSPARENT: &str = "rgba(255, 255, 255, 0)";

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let game = settings.game;
    let language = settings.string(Field::Root("language"));
    let realm = settings.string(Field::Root("realm"));

    let mut page = page("page-general");

    // Language: `select.rounded bg-gray-700 w-24`, 6 rem wide.
    let languages: &[(&str, &str)] = if game == GameId::Poe2 { &LANGUAGES_POE2 } else { &LANGUAGES_POE1 };
    let choices: Vec<ui::select::Choice> =
        languages.iter().map(|(value, name)| ui::select::Choice::new(*value, *name)).collect();
    page = page.child(
        col()
            .key("general-language")
            .mb(theme::rem(0.5))
            .child(label("general-language-label", &tr(settings, "settings.language")))
            .child(ui::select::select(
                "general-language-select",
                &choices,
                &language,
                settings.select_open == Some("general-language-select"),
                theme::rem(6.0),
                move |s: &mut S| toggle_select(project, s, "general-language-select"),
                move |s: &mut S, value: String| pick_language(project(s), &value),
            )),
    );

    // EE2: `preferredTradeSite`, for any language but English.
    if game == GameId::Poe2 && language != "en" {
        let trade_url = trade_url(&language, &realm);
        let choices =
            vec![ui::select::Choice::new("default", trade_url), ui::select::Choice::new("www", "www.pathofexile.com")];
        page = page.child(
            col()
                .key("general-preferred-trade-site")
                .mb(theme::rem(1.0))
                .child(label("general-preferred-trade-site-label", &tr(settings, "settings.preferred_trade_site")))
                .child(ui::select::select(
                    "general-preferred-trade-site-select",
                    &choices,
                    &settings.string(Field::Root("preferredTradeSite")),
                    settings.select_open == Some("general-preferred-trade-site-select"),
                    theme::rem(6.0),
                    move |s: &mut S| toggle_select(project, s, "general-preferred-trade-site-select"),
                    move |s: &mut S, value: String| {
                        project(s).set_string(Field::Root("preferredTradeSite"), &value);
                        project(s).select_open = None;
                    },
                )),
        );
    }

    if language == "cmn-Hant" {
        page = page.child(
            col()
                .key("general-realm")
                .mb(theme::rem(0.5))
                .child(label("general-realm-label", &tr(settings, "realm")))
                .child(
                    row()
                        .gap(theme::rem(1.0))
                        .child(ui::radio(
                            "general-realm-pc-ggg",
                            &tr(settings, "realm_intl"),
                            realm == "pc-ggg",
                            false,
                            move |s: &mut S| pick_realm(project(s), "pc-ggg"),
                        ))
                        .child(ui::radio(
                            "general-realm-pc-garena",
                            &tr(settings, "Hotcool"),
                            realm == "pc-garena",
                            false,
                            move |s: &mut S| pick_realm(project(s), "pc-garena"),
                        )),
                ),
        );
    }

    // APT only: the international-site box, forced on for a Hant client on the global realm.
    if game != GameId::Poe2 && language != "en" && realm == "pc-ggg" {
        let forced = language == "cmn-Hant";
        let checked = settings.bool(Field::Root("useIntlSite"));
        page = page.child(
            row().key("general-use-intl-site-row").mb(theme::rem(1.0)).child(
                row()
                    .key("general-use-intl-site")
                    .gap(theme::rem(0.25))
                    .items_center()
                    .child(ui::checkbox(
                        "general-use-intl",
                        &tr(settings, "settings.use_intl_site"),
                        checked,
                        forced,
                        move |s: &mut S| {
                            let checked = project(s).bool(Field::Root("useIntlSite"));
                            project(s).set_bool(Field::Root("useIntlSite"), !checked);
                        },
                    ))
                    .child(ui::field::badge("general-use-intl-site-name", "www.pathofexile.com")),
            ),
        );
    }

    // Font size: `w-16` number field, then `px`.
    page = page.child(
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
                        "general-font-size-input",
                        Field::Root("fontSize"),
                        theme::rem(4.0),
                        Limit::Any,
                        project,
                    ))
                    .child(text("px").key("general-font-size-unit").font(type_::SM)),
            ),
    );

    page = page.child(row().key("general-restore-clipboard-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "general-restore-clipboard",
        Field::Root("restoreClipboard"),
        &tr(settings, "settings.restore_clipboard"),
        project,
    )));

    for (key, field, title_key, placeholder) in [
        (
            "general-client-log",
            "clientLog",
            "settings.poe_log_file",
            if game == GameId::Poe2 {
                "...?/Grinding Gear Games/Path of Exile 2/logs/Client.txt"
            } else {
                "...?/Grinding Gear Games/Path of Exile/logs/Client.txt"
            },
        ),
        (
            "general-game-config",
            "gameConfig",
            "settings.poe_cfg_file",
            if game == GameId::Poe2 {
                "...?/My Games/Path of Exile 2/poe2_production_Config.ini"
            } else {
                "...?/My Games/Path of Exile/production_Config.ini"
            },
        ),
    ] {
        // The log file's block is `mb-2`, the config file's `mb-4`.
        let margin = if key == "general-game-config" { theme::rem(1.0) } else { theme::rem(0.5) };
        page = page.child(
            col()
                .key(format!("{key}-field"))
                .mb(margin)
                .child(label(&format!("{key}-label"), &tr(settings, title_key)))
                .child(text_field(settings, key, Field::Root(field), Kind::Sans, None, placeholder, project)),
        );
    }

    // `<hr class="mb-4 mx-8 border-gray-700">`
    page = page.child(
        row().key("general-rule").h(1.0).mx(theme::rem(2.0)).mb(theme::rem(1.0)).fill(theme::gray(700)).border_layout(),
    );

    // The clickable-background colour, with its Transparent radio.
    let background = settings.string(Field::Root("overlayBackground"));
    page = page.child(
        col()
            .key("general-overlay-background")
            .mb(theme::rem(0.5))
            .child(label("general-overlay-background-label", &tr(settings, "settings.overlay_bg")))
            .child(
                row()
                    .gap(theme::rem(1.0))
                    .items_center()
                    .flex_wrap()
                    .child(text_field(
                        settings,
                        "general-overlay-background-input",
                        Field::Root("overlayBackground"),
                        Kind::Text,
                        Some(theme::rem(12.0)),
                        "",
                        project,
                    ))
                    .child(ui::radio(
                        "general-overlay-background-none",
                        &tr(settings, "settings.overlay_bg_none"),
                        background == TRANSPARENT,
                        false,
                        move |s: &mut S| {
                            project(s).set_string(Field::Root("overlayBackground"), TRANSPARENT);
                            // The text field's value is the working copy's string.
                            project(s).edit_mut("general-overlay-background-input").replace_text(TRANSPARENT);
                        },
                    )),
            ),
    );

    if background != TRANSPARENT {
        page = page.child(row().key("general-overlay-close-row").mb(theme::rem(0.5)).child(checkbox(
            settings,
            "general-overlay-background-close",
            Field::Root("overlayBackgroundClose"),
            &tr(settings, "settings.overlay_bg_focus_game"),
            project,
        )));
    }

    page = page.child(row().key("general-attach-notification-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "general-attach-notification",
        Field::Root("showAttachNotification"),
        &tr(settings, "settings.show_overlay_ready"),
        project,
    )));

    if game == GameId::Poe2 {
        page = page.child(row().key("general-overlay-always-close-row").mb(theme::rem(1.0)).child(checkbox(
            settings,
            "general-overlay-always-close",
            Field::Root("overlayAlwaysClose"),
            &tr(settings, "settings.overlay_always_close"),
            project,
        )));
        page = page.child(row().key("general-hide-overlay-row").mb(theme::rem(1.0)).child(checkbox(
            settings,
            "general-hide-overlay-on-blur",
            Field::Root("hideOverlayOnBlur"),
            &tr(settings, "settings.hide_overlay_on_blur"),
            project,
        )));
        page = page
            .child(ui::field::hint("general-hide-overlay-hint", &tr(settings, "settings.hide_overlay_on_blur_hint")));
    }

    // The window title, with the Restart required badge.
    page = page.child(
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
                "general-window-title-input",
                Field::Root("windowTitle"),
                Kind::Text,
                None,
                "",
                project,
            )),
    );

    if game == GameId::Poe2 {
        page = page.child(row().key("general-read-client-log-row").mb(theme::rem(1.0)).child(checkbox(
            settings,
            "general-read-client-log",
            Field::Root("readClientLog"),
            &tr(settings, "settings.read_client_log"),
            project,
        )));
        page = page.child(ui::field::hint("general-client-log-explain", &tr(settings, "settings.client_log_explain")));

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
            let library = settings
                .config
                .get("alphas")
                .and_then(|alphas| alphas.as_array())
                .is_some_and(|alphas| alphas.iter().any(|alpha| alpha == "library"));
            alphas = alphas.child(row().key("general-library-alpha-row").child(ui::checkbox(
                "general-library-alpha",
                &tr(settings, "settings.alpha_library"),
                library,
                false,
                move |s: &mut S| {
                    let settings = project(s);
                    let mut alphas: Vec<serde_json::Value> =
                        settings.config.get("alphas").and_then(|alphas| alphas.as_array()).cloned().unwrap_or_default();
                    if alphas.iter().any(|alpha| alpha == "library") {
                        alphas.retain(|alpha| alpha != "library");
                    } else {
                        alphas.push(serde_json::Value::String("library".to_owned()));
                    }
                    settings.set(Field::Root("alphas"), serde_json::Value::Array(alphas));
                },
            )));
        }
        page = page.child(alphas);
    }

    page
}

/// `preferredTradeSite`'s first option: the trade site the language's client uses
/// (`tradeUrl` in `settings.general`).
fn trade_url(language: &str, realm: &str) -> String {
    let host = match language {
        "ru" => "ru.pathofexile.com",
        "cmn-Hant" if realm == "pc-garena" => "pathofexile.tw",
        "cmn-Hant" => "www.pathofexile.com",
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
