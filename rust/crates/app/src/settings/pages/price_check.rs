//! `web/price-check/settings-price-check.vue` for both games. APT's page is the leagues,
//! account and seller settings, the stat-range and stock/cursor/prediction boxes, the default
//! online filter and the rate-limit block; EE2's replaces the online filter with its core
//! currency, volume, tooltip and tier settings and adds an advanced-delay box.
//!
//! Not ported: the league list itself (`useLeagues` is not wired into the app yet: the
//! `leagueId` config field is edited as the private-league box), the core currency icons
//! (poe.ninja's item images belong to the images work) and the league radios of the default
//! filter (they are disabled in the settings context anyway).

use rune::{El, col, row, text};
use serde_json::Value;
use wraex_core::GameId;

use crate::settings::pages::{Limit, checkbox, label, number_field, page, text_field, tr};
use crate::settings::{Field, Project, Settings};
use crate::theme::{self, text as type_};
use crate::ui;
use crate::ui::field::Kind;

const PRICE_CHECK: &str = "price-check";

pub fn view<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let mut page = page("page-price-check");
    page = page.child(league(settings, project));
    page = page.child(account(settings, project));
    page = page.child(show_seller(settings, project));
    page = page.child(fill_rolls(settings, project));

    page = if settings.game == GameId::Poe2 {
        poe2_body(page, settings, project)
    } else {
        poe1_body(page, settings, project)
    };

    page
}

/// The `league` block: APT and EE2 both show the configured league as a private-league box
/// until the trade site's league list is available.
fn league<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    col()
        .key("price-check-league")
        .w_full()
        .mb(theme::rem(0.5))
        .child(label("price-check-league-label", &tr(settings, "league")))
        .child(
            row()
                .key("price-check-league-private")
                .gap(theme::rem(0.5))
                .items_center()
                .mb(theme::rem(1.0))
                .child(
                    text(tr(settings, "settings.private_league"))
                        .key("price-check-league-private-label")
                        .font(type_::SM)
                        .color(theme::gray(500)),
                )
                .child(text_field(
                    settings,
                    "price-check-league-custom",
                    Field::Root("leagueId"),
                    Kind::Grow,
                    None,
                    "My League (PL12345)",
                    project,
                )),
        )
}

/// `accountName`.
fn account<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    col()
        .key("price-check-account")
        .w_full()
        .mb(theme::rem(0.5))
        .child(label("price-check-account-label", &tr(settings, "settings.account_name")))
        .child(text_field(settings, "price-check-account", Field::Root("accountName"), Kind::Text, None, "", project))
}

/// `showSeller`: No / Account name / Last character name, and the highlight hint.
fn show_seller<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let show_seller = settings.string(Field::Widget(PRICE_CHECK, "showSeller"));
    let show_seller = if show_seller.is_empty() { "off".to_owned() } else { show_seller };
    col()
        .key("price-check-show-seller")
        .w_full()
        .mb(theme::rem(0.5))
        .child(label("price-check-show-seller-label", &tr(settings, "price_check.show_seller")))
        .child(
            row()
                .key("price-check-show-seller-radios")
                .mb(theme::rem(0.25))
                .items_center()
                .gap(theme::rem(1.0))
                .child(seller_radio("price-check-seller-off", "off", &tr(settings, "No"), &show_seller, project))
                .child(seller_radio(
                    "price-check-seller-account",
                    "account",
                    &tr(settings, "settings.account_name"),
                    &show_seller,
                    project,
                ))
                .child(seller_radio(
                    "price-check-seller-ign",
                    "ign",
                    &tr(settings, "settings.last_char_name"),
                    &show_seller,
                    project,
                )),
        )
        .child(
            text(tr(settings, "price_check.highlight_hint"))
                .key("price-check-highlight-hint")
                .font(type_::SM.italic())
                .color(theme::gray(500))
                .mb(theme::rem(1.0)),
        )
}

fn seller_radio<S: 'static>(key: &str, value: &'static str, label: &str, current: &str, project: Project<S>) -> El {
    let checked = current == value;
    ui::radio(key, label, checked, false, move |s: &mut S| {
        match value {
            "off" => project(s).set(Field::Widget(PRICE_CHECK, "showSeller"), Value::Bool(false)),
            other => project(s).set_string(Field::Widget(PRICE_CHECK, "showSeller"), other),
        };
    })
}

/// `fill_rolls`: the range number field and the exact-value radio.
fn fill_rolls<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let range = Field::Widget(PRICE_CHECK, "searchStatRange");
    let exact = range.read(&settings.config).as_f64() == Some(0.0);
    col()
        .key("price-check-fill-rolls")
        .w_full()
        .mb(theme::rem(0.5))
        .child(label("price-check-fill-rolls-label", &tr(settings, "price_check.fill_rolls")))
        .child(
            row()
                .key("price-check-fill-rolls-row")
                .mb(theme::rem(1.0))
                .items_center()
                .gap(theme::rem(1.5))
                .child(
                    row()
                        .key("price-check-stat-range")
                        .gap(theme::rem(0.25))
                        .items_center()
                        .child(text("+-").key("price-check-stat-range-sign").font(type_::SM))
                        .child(number_field(
                            settings,
                            "price-check-stat-range-input",
                            range,
                            theme::rem(4.0),
                            Limit::Within(0.0, 50.0),
                            project,
                        ))
                        .child(text("%").key("price-check-stat-range-unit").font(type_::SM)),
                )
                .child(ui::radio(
                    "price-check-stat-range-exact",
                    &tr(settings, "price_check.fill_roll_exact"),
                    exact,
                    false,
                    move |s: &mut S| project(s).set_f64(Field::Widget(PRICE_CHECK, "searchStatRange"), 0.0),
                )),
        )
}

/// APT's boxes after the shared top: stock, prediction, cursor, the default filter, the
/// builtin browser and the rate-limit block.
fn poe1_body<S: 'static>(mut page: El, settings: &Settings, project: Project<S>) -> El {
    page = page.child(row().key("price-check-stock-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "price-check-stock",
        Field::Widget(PRICE_CHECK, "activateStockFilter"),
        &tr(settings, "price_check.select_stock"),
        project,
    )));
    page = page.child(row().key("price-check-prediction-row").mb(theme::rem(1.0)).child(prediction(settings, project)));
    page = page.child(row().key("price-check-cursor-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "price-check-cursor",
        Field::Widget(PRICE_CHECK, "showCursor"),
        &tr(settings, "price_check.cursor_pos"),
        project,
    )));

    // `border-2 rounded border-gray-700 mb-4` around the default online filter.
    let currency = settings.string(Field::Widget(PRICE_CHECK, "defaultCurrency"));
    let merchant = settings.bool(Field::Widget(PRICE_CHECK, "merchantOnly"));
    let mut box_ = col()
        .key("price-check-default-settings")
        .w_full()
        .mb(theme::rem(1.0))
        .border(2.0, theme::gray(700))
        .border_layout()
        .radius(theme::ROUNDED)
        .child(section_label("price-check-default-settings-header", &tr(settings, "price_check.default_settings")));
    let mut filters = row().key("price-check-default-filters").padding(theme::rem(0.5)).gap(theme::rem(2.0));
    let mut left = col().key("price-check-default-left").gap(theme::rem(0.25));
    left = left.child(disabled_toggle("price-check-default-offline", &tr(settings, "online_filter.offline_toggle")));
    left = left.child(ui::radio(
        "price-check-default-listed-any",
        &tr(settings, "online_filter.listed_any_time"),
        true,
        true,
        |_s: &mut S| {},
    ));
    let mut right = col().key("price-check-default-right").gap(theme::rem(0.25));
    right = right.child(ui::toggle(
        "price-check-default-merchant",
        &tr(settings, "online_filter.merchant_toggle"),
        merchant,
        move |s: &mut S| {
            let next = !project(s).bool(Field::Widget(PRICE_CHECK, "merchantOnly"));
            project(s).set_bool(Field::Widget(PRICE_CHECK, "merchantOnly"), next);
        },
    ));
    right = right.child(rune::el().h(theme::rem(0.5)));
    for (key, value, label_key) in [
        ("price-check-default-currency-any", "", "online_filter.currency_any"),
        ("price-check-default-currency-chaos", "chaos", "online_filter.currency_only_chaos"),
        ("price-check-default-currency-divine", "divine", "online_filter.currency_only_div"),
        ("price-check-default-currency-chaos-divine", "chaos_divine", "online_filter.currency_chaos_div"),
    ] {
        let checked = currency == value;
        right = right.child(ui::radio(key, &tr(settings, label_key), checked, false, move |s: &mut S| {
            if value.is_empty() {
                project(s).set(Field::Widget(PRICE_CHECK, "defaultCurrency"), Value::Null);
            } else {
                project(s).set_string(Field::Widget(PRICE_CHECK, "defaultCurrency"), value);
            }
        }));
    }
    filters = filters.child(left).child(right);
    box_ = box_.child(filters);
    page = page.child(box_);

    page = page.child(browser(settings, project));
    page.child(rate_limit(settings, project))
}

/// EE2's boxes: core currency, remembered currency/listing type, volume, tooltip, tier
/// settings, the browser, and its two rate-limit boxes.
fn poe2_body<S: 'static>(mut page: El, settings: &Settings, project: Project<S>) -> El {
    // Primary base currency: `coreCurrency`, shown without poe.ninja's item icons.
    let core = settings.string(Field::Widget(PRICE_CHECK, "coreCurrency"));
    let mut currency = col()
        .key("price-check-core-currency")
        .w_full()
        .mb(theme::rem(1.0))
        .child(label("price-check-core-currency-label", &tr(settings, "price_check.core_currency")))
        .child(row().key("price-check-core-currency-radios").gap(theme::rem(1.0)).items_center());
    for (key, value, name) in
        [("price-check-core-exalted", "exalted", "Exalted Orb"), ("price-check-core-chaos", "chaos", "Chaos Orb")]
    {
        let checked = core == value;
        currency = currency.child(ui::radio(key, name, checked, false, move |s: &mut S| {
            project(s).set_string(Field::Widget(PRICE_CHECK, "coreCurrency"), value);
        }));
    }
    page = page.child(currency);

    page = page.child(row().key("price-check-remember-currency-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "price-check-remember-currency",
        Field::Widget(PRICE_CHECK, "rememberCurrency"),
        &tr(settings, "price_check.remember_currency"),
        project,
    )));

    let mut listing = col().key("price-check-remember-listing").w_full().mb(theme::rem(0.5));
    listing = listing.child(row().key("price-check-remember-listing-row").mb(theme::rem(0.25)).child(checkbox(
        settings,
        "price-check-remember-listing",
        Field::Widget(PRICE_CHECK, "rememberListingType"),
        &tr(settings, "price_check.remember_listing"),
        project,
    )));
    listing = listing.child(
        text(tr(settings, "price_check.remember_listing_note"))
            .key("price-check-remember-listing-note")
            .mb(theme::rem(1.0))
            .font(type_::SM.italic())
            .color(theme::gray(500))
            .wrap(),
    );
    page = page.child(listing);

    page = page.child(row().key("price-check-stock-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "price-check-stock",
        Field::Widget(PRICE_CHECK, "activateStockFilter"),
        &tr(settings, "price_check.select_stock"),
        project,
    )));

    // Currency exchange volume.
    let volume = settings.string(Field::Widget(PRICE_CHECK, "currencyVolume"));
    let mut volume_col = col()
        .key("price-check-volume")
        .w_full()
        .mb(theme::rem(1.0))
        .child(label("price-check-volume-label", &tr(settings, "price_check.show_volume")))
        .child(row().key("price-check-volume-radios").gap(theme::rem(1.0)).items_center());
    for (key, value, name) in [
        ("price-check-volume-none", "none", "None"),
        ("price-check-volume-value", "value", "Currency / Hour"),
        ("price-check-volume-item", "item", "Items / Hour"),
        ("price-check-volume-both", "both", "Both"),
    ] {
        let checked = volume == value;
        volume_col = volume_col.child(ui::radio(key, name, checked, false, move |s: &mut S| {
            project(s).set_string(Field::Widget(PRICE_CHECK, "currencyVolume"), value);
        }));
    }
    page = page.child(volume_col);

    page = page.child(row().key("price-check-prediction-row").mb(theme::rem(1.0)).child(prediction(settings, project)));
    page = page.child(row().key("price-check-cursor-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "price-check-cursor",
        Field::Widget(PRICE_CHECK, "showCursor"),
        &tr(settings, "price_check.cursor_pos"),
        project,
    )));

    let tooltip = settings.string(Field::Widget(PRICE_CHECK, "itemHoverTooltip"));
    let mut tooltip_col = col()
        .key("price-check-tooltip")
        .w_full()
        .mb(theme::rem(1.0))
        .child(label("price-check-tooltip-label", &tr(settings, "price_check.use_tooltip_hover")))
        .child(row().key("price-check-tooltip-radios").gap(theme::rem(1.0)).items_center());
    for (key, value, name_key) in [
        ("price-check-tooltip-off", "off", "price_check.use_tooltip_off"),
        ("price-check-tooltip-keybind", "keybind", "price_check.use_tooltip_keybind"),
        ("price-check-tooltip-always", "always", "price_check.use_tooltip_always"),
    ] {
        let checked = tooltip == value;
        let name = tr(settings, name_key);
        tooltip_col = tooltip_col.child(ui::radio(key, &name, checked, false, move |s: &mut S| {
            project(s).set_string(Field::Widget(PRICE_CHECK, "itemHoverTooltip"), value);
        }));
    }
    page = page.child(tooltip_col);

    page = page.child(row().key("price-check-all-selected-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "price-check-all-selected",
        Field::Widget(PRICE_CHECK, "defaultAllSelected"),
        &tr(settings, "price_check.default_all_selected"),
        project,
    )));
    page = page.child(row().key("price-check-always-tier-row").mb(theme::rem(1.0)).child(checkbox(
        settings,
        "price-check-always-tier",
        Field::Widget(PRICE_CHECK, "alwaysShowTier"),
        &tr(settings, "price_check.always_show_tier"),
        project,
    )));

    page = page.child(browser(settings, project));

    // The rate-limit compromise box, and EE2's advanced-delay box.
    page = page.child(rate_limit(settings, project));
    let mut advanced = col()
        .key("price-check-warn-advanced")
        .w_full()
        .mb(theme::rem(0.5))
        .border(2.0, theme::gray(700))
        .border_layout()
        .radius(theme::ROUNDED)
        .child(section_label("price-check-warn-advanced-header", &tr(settings, "price_check.warn_advanced")))
        .child(
            col()
                .key("price-check-initial-delay")
                .padding(theme::rem(0.5))
                .child(label("price-check-initial-delay-label", &tr(settings, "price_check.initial_delay")))
                .child(
                    row()
                        .gap(theme::rem(0.5))
                        .items_center()
                        .child(number_field(
                            settings,
                            "price-check-initial-delay-input",
                            Field::Widget(PRICE_CHECK, "initialDelay"),
                            theme::rem(4.0),
                            Limit::Clamp(5.0, 400.0),
                            project,
                        ))
                        .child(text("milliseconds").key("price-check-initial-delay-unit").font(type_::SM)),
                ),
        );
    advanced = advanced.child(rune::el().h(theme::rem(0.5)));
    page.child(advanced)
}

/// `requestPricePrediction`, with the `www.poeprices.info` badge beside it.
fn prediction<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    row()
        .key("price-check-prediction")
        .gap(theme::rem(0.25))
        .items_center()
        .child(checkbox(
            settings,
            "price-check-prediction-toggle",
            Field::Widget(PRICE_CHECK, "requestPricePrediction"),
            &tr(settings, "price_check.show_prediction"),
            project,
        ))
        .child(
            row()
                .key("price-check-prediction-site")
                .px(theme::rem(0.25))
                .radius(theme::ROUNDED)
                .fill(theme::gray(700))
                .child(
                    text("www.poeprices.info")
                        .key("price-check-prediction-site-label")
                        .font(type_::SM)
                        .color(theme::gray(400)),
                ),
        )
}

/// The builtin browser box, which turns orange while enabled.
fn browser<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let enabled = settings.bool(Field::Widget(PRICE_CHECK, "builtinBrowser"));
    let mut block = col().key("price-check-builtin-browser").w_full().mb(theme::rem(1.0));
    if enabled {
        block = block.padding(theme::rem(0.5)).radius(theme::ROUNDED).fill(theme::orange(600));
    }
    block = block.child(checkbox(
        settings,
        "price-check-builtin-browser-toggle",
        Field::Widget(PRICE_CHECK, "builtinBrowser"),
        &tr(settings, "price_check.enable_browser"),
        project,
    ));
    if enabled {
        block = block.child(
            text(tr(settings, "price_check.builtin_browser_warning"))
                .key("price-check-builtin-browser-warning")
                .mt(theme::rem(0.25))
                .font(type_::SM)
                .wrap(),
        );
    }
    block
}

/// The `warn_expensive` box: the collapsed-listings box, the auto-search toggles and the
/// extra delay.
fn rate_limit<S: 'static>(settings: &Settings, project: Project<S>) -> El {
    let collapse = settings.string(Field::Widget(PRICE_CHECK, "collapseListings"));
    let mut box_ = col()
        .key("price-check-rate-limit")
        .w_full()
        .mb(theme::rem(0.5))
        .border(2.0, theme::gray(700))
        .border_layout()
        .radius(theme::ROUNDED)
        .child(section_label("price-check-rate-limit-header", &tr(settings, "price_check.warn_expensive")));
    box_ =
        box_.child(row().key("price-check-collapse-row").mx(theme::rem(0.5)).mb(theme::rem(1.0)).child(ui::checkbox(
            "price-check-collapse",
            &tr(settings, "price_check.accurate_collapsed"),
            collapse == "app",
            false,
            move |s: &mut S| {
                let current = project(s).string(Field::Widget(PRICE_CHECK, "collapseListings"));
                let next = if current == "app" { "api" } else { "app" };
                project(s).set_string(Field::Widget(PRICE_CHECK, "collapseListings"), next);
            },
        )));

    let quick = quick_hotkey(settings);
    let locked = settings.string(Field::Widget(PRICE_CHECK, "hotkeyLocked"));
    let mut auto = col().key("price-check-auto-search").mx(theme::rem(0.5)).mb(theme::rem(0.5));
    auto = auto.child(label("price-check-auto-search-label", &tr(settings, "price_check.auto_search")));
    let mut toggles = row().key("price-check-auto-search-toggles").mb(theme::rem(1.0)).gap(theme::rem(1.5));
    if let Some(quick) = quick {
        let on = settings.bool(Field::Widget(PRICE_CHECK, "smartInitialSearch"));
        toggles = toggles.child(hotkey_toggle("price-check-smart-search", &quick, on, move |s: &mut S| {
            let on = project(s).bool(Field::Widget(PRICE_CHECK, "smartInitialSearch"));
            project(s).set_bool(Field::Widget(PRICE_CHECK, "smartInitialSearch"), !on);
        }));
    }
    if !locked.is_empty() {
        let on = settings.bool(Field::Widget(PRICE_CHECK, "lockedInitialSearch"));
        toggles = toggles.child(hotkey_toggle("price-check-locked-search", &locked, on, move |s: &mut S| {
            let on = project(s).bool(Field::Widget(PRICE_CHECK, "lockedInitialSearch"));
            project(s).set_bool(Field::Widget(PRICE_CHECK, "lockedInitialSearch"), !on);
        }));
    }
    auto = auto.child(toggles);
    box_ = box_.child(auto);

    box_ = box_.child(
        col()
            .key("price-check-extra-delay")
            .mx(theme::rem(0.5))
            .mb(theme::rem(0.5))
            .child(label("price-check-extra-delay-label", &tr(settings, "price_check.extra_delay")))
            .child(
                row()
                    .gap(theme::rem(0.5))
                    .items_center()
                    .child(number_field(
                        settings,
                        "price-check-api-latency-input",
                        Field::Widget(PRICE_CHECK, "apiLatencySeconds"),
                        theme::rem(4.0),
                        Limit::Clamp(0.5, 10.0),
                        project,
                    ))
                    .child(text(tr(settings, "seconds")).key("price-check-extra-delay-unit").font(type_::SM)),
            ),
    );
    box_
}

/// `{hotkeyHold} + {hotkey}` (`hotkeyQuick`), when there is one.
fn quick_hotkey(settings: &Settings) -> Option<String> {
    let key = settings.string(Field::Widget(PRICE_CHECK, "hotkey"));
    if key.is_empty() {
        return None;
    }
    let hold = settings.string(Field::Widget(PRICE_CHECK, "hotkeyHold"));
    Some(format!("{hold} + {key}"))
}

/// `UiToggle` with a `bg-gray-900 text-gray-500 rounded px-2` hotkey badge as its label.
fn hotkey_toggle<S: 'static>(key: &str, hotkey: &str, on: bool, on_press: impl Fn(&mut S) + 'static) -> El {
    row()
        .key(key.to_owned())
        .h(theme::rem(1.375))
        .items_center()
        .gap(theme::rem(0.25))
        .focusable(true)
        .role("switch")
        .a11y_value(if on { "on" } else { "off" })
        .label(hotkey.to_owned())
        .on_press(on_press)
        .child(ui::icon::toggle(on, if on { theme::gray(300) } else { theme::gray(600) }))
        .child(
            row().key(format!("{key}-hotkey")).px(theme::rem(0.5)).radius(theme::ROUNDED).fill(theme::gray(900)).child(
                text(hotkey.to_owned()).key(format!("{key}-hotkey-label")).font(type_::SM).color(theme::gray(500)),
            ),
        )
}

/// A disabled toggle: the settings context's `offline` switch, greyed (`text-gray-600`).
fn disabled_toggle(key: &str, label: &str) -> El {
    row()
        .key(key.to_owned())
        .h(theme::rem(1.375))
        .items_center()
        .gap(theme::rem(0.25))
        .child(ui::icon::toggle(false, theme::gray(600)))
        .child(text(label.to_owned()).key(format!("{key}-label")).font(type_::SM).color(theme::gray(600)))
}

/// `bg-gray-700 p-2` box header.
fn section_label(key: &str, label: &str) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .px(theme::rem(0.5))
        .py(theme::rem(0.5))
        .fill(theme::gray(700))
        .child(text(label.to_owned()).key(format!("{key}-label")).font(type_::SM))
}
