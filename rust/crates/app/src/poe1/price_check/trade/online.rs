//! `OnlineFilter.vue` / `OnlineFilterCore.vue`: the "Merchant Only" label under the
//! matched row, and the panel it opens with the offline, listed-time, currency and
//! league filters.
//!
//! The reference opens the panel on hover (a tippy popover); this opens it on press.
//! The panel is anchored under the label with [`El::anchored`]. The trade-league radios
//! are not drawn yet: the app has no league list (`useLeagues`), only the selected id.

use rune::{Align, Color, El, Side, col, el, row, text};
use wraex_poe1::filters::ItemFilters;
use wraex_poe1::trade::Api;

use crate::poe1::i18n;
use crate::poe1::price_check::PriceCheck;
use crate::poe1::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui::{self, icon};

/// The listed-time radios (`OnlineFilterCore.vue`), in template order.
const LISTED: [(&str, &str); 7] = [
    ("", "online_filter.listed_any_time"),
    ("1day", "online_filter.listed_1day"),
    ("3days", "online_filter.listed_3days"),
    ("1week", "online_filter.listed_1week"),
    ("2weeks", "online_filter.listed_2weeks"),
    ("1month", "online_filter.listed_1month"),
    ("2months", "online_filter.listed_2months"),
];

/// The currency radios (`OnlineFilterCore.vue`), in template order.
const CURRENCY: [(&str, &str); 4] = [
    ("", "online_filter.currency_any"),
    ("chaos", "online_filter.currency_only_chaos"),
    ("divine", "online_filter.currency_only_div"),
    ("chaos_divine", "online_filter.currency_chaos_div"),
];

/// `OnlineFilter.vue`'s label: a history icon, the filter's name and the warning colour.
pub fn view<S: 'static>(check: &PriceCheck, api: Api, project: Project<S>) -> El {
    let filters = check.item_filters();
    let offline = filters.is_some_and(|filters| filters.trade.offline);
    let label = if offline {
        i18n::t("Offline")
    } else if api == Api::Trade && filters.is_some_and(|filters| filters.trade.merchant_only) {
        i18n::t("online_filter.merchant_toggle")
    } else {
        i18n::t("Online")
    };
    let warning = filters.is_some_and(show_warning);
    let color = if warning { theme::orange(500) } else { theme::gray(500) };
    row()
        .key("online-filter")
        .mr(theme::rem(0.25))
        .px(theme::rem(0.5))
        .items_center()
        .gap(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .focusable(true)
        .role("button")
        .label(label.clone())
        .on_press(move |s: &mut S| project(s).toggle_online_filter())
        .child(icon::history(theme::rem(0.75), color))
        .child(text(label).key("online-filter-label").font(type_::SM).color(color).truncate())
}

/// `showWarning`: a restricted listed time or currency turns the label orange.
fn show_warning(filters: &ItemFilters) -> bool {
    let trade = &filters.trade;
    trade.listed.as_deref().is_some_and(|listed| ["1day", "3days", "1week"].contains(&listed))
        || trade.currency.as_deref().is_some_and(|currency| currency != "chaos_divine")
}

/// The panel `OnlineFilter.vue` puts in its popover (`v-if="showRateLimitState"`'s
/// neighbour: only while open). Anchored under the label.
pub fn panel<S: 'static>(check: &PriceCheck, api: Api, project: Project<S>) -> El {
    let Some(filters) = check.item_filters() else { return el() };
    let trade = &filters.trade;
    let offline = trade.offline;
    let mut left = col().key("online-filter-left").gap(theme::rem(0.25));
    left = left.child(row().key("online-filter-offline-row").mb(theme::rem(0.25)).child(ui::toggle::toggle(
        "online-filter-offline",
        &i18n::t("online_filter.offline_toggle"),
        offline,
        move |s: &mut S| project(s).set_offline(!offline),
    )));
    if api == Api::Trade {
        for (value, key) in LISTED {
            let checked = trade.listed.as_deref().unwrap_or("") == value;
            left =
                left.child(radio(format!("online-filter-listed-{value}"), &i18n::t(key), checked, move |s: &mut S| {
                    project(s).set_listed(Some(value).filter(|value| !value.is_empty()))
                }));
        }
    }
    let mut right = col().key("online-filter-right").gap(theme::rem(0.25));
    let toggle = if api == Api::Bulk {
        ("online-filter-in-league", "online_filter.in_league_toggle", trade.online_in_league)
    } else {
        ("online-filter-merchant", "online_filter.merchant_toggle", trade.merchant_only)
    };
    right = right.child(
        row().key("online-filter-toggle-row").mb(theme::rem(0.25)).opacity(if offline { 0.0 } else { 1.0 }).child(
            ui::toggle::toggle(toggle.0, &i18n::t(toggle.1), toggle.2, move |s: &mut S| match api {
                Api::Bulk => project(s).toggle_online_in_league(),
                Api::Trade => project(s).toggle_merchant_only(),
            }),
        ),
    );
    if api == Api::Trade {
        right = right.child(el().h(theme::rem(0.75)));
        for (value, key) in CURRENCY {
            let checked = trade.currency.as_deref().unwrap_or("") == value;
            right = right.child(radio(
                format!("online-filter-currency-{value}"),
                &i18n::t(key),
                checked,
                move |s: &mut S| project(s).set_currency(Some(value).filter(|value| !value.is_empty())),
            ));
        }
    }
    let panel = row().gap(theme::rem(2.0)).child(left).child(right);
    // tippy's light theme: a white frame around the content, four points of padding.
    row()
        .key("online-filter-panel")
        .padding(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(Color::WHITE)
        .child(
            row()
                .key("online-filter-content")
                .px(theme::rem(0.5))
                .py(theme::rem(0.5))
                .fill(theme::gray(800))
                .color(theme::gray(400))
                .child(panel),
        )
        .anchored("online-filter", Side::Bottom, Align::Start, 0.0)
}

/// `UiRadio.vue`: a circle or a check circle and its label.
fn radio<S: 'static>(key: String, label: &str, checked: bool, on_press: impl Fn(&mut S) + 'static) -> El {
    let label_key = format!("{key}-label");
    row()
        .key(key)
        .gap(theme::rem(0.25))
        .items_center()
        .focusable(true)
        .role("radio")
        .label(label.to_owned())
        .on_press(on_press)
        .child(icon::radio(theme::rem(0.875), checked, theme::gray(400)))
        .child(text(label.to_owned()).key(label_key).font(type_::SM).color(theme::gray(400)))
}
