//! `OnlineFilter.vue`: the "Instant"/"Online" label under the matched row and the panel it
//! opens with the listing type, listed-time and currency filters.
//!
//! EE2 merged `OnlineFilterCore.vue` into `OnlineFilter.vue`: the label names the listing
//! type (`Instant` for `securable`, `Offline` for `any`, `Online` otherwise) and the panel
//! is a popover with radios. The reference opens it on hover (a tippy popover); this opens
//! it on press. The trade-league radios are not drawn: the app has no league list
//! (`useLeagues`), only the selected id.

use rune::{Align, Color, El, Side, col, el, row, text};
use wraex_poe2::filters::ItemFilters;
use wraex_poe2::trade::Api;

use crate::poe2::i18n;
use crate::poe2::price_check::PriceCheck;
use crate::poe2::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui::icon;

/// The listing types, in the panel's order (`OnlineFilter.vue`).
const TRADE_TYPES: [(&str, &str); 4] =
    [("available", "Instant or Online"), ("securable", "Instant"), ("online", "Online"), ("any", ":offline_toggle")];

/// The bulk side's listing types.
const BULK_TYPES: [(&str, &str); 3] =
    [("onlineleague", ":in_league_toggle"), ("online", "Online"), ("any", ":offline_toggle")];

/// The listed-time radios, in template order.
const LISTED: [(&str, &str); 7] = [
    ("", "online_filter.listed_any_time"),
    ("1day", "online_filter.listed_1day"),
    ("3days", "online_filter.listed_3days"),
    ("1week", "online_filter.listed_1week"),
    ("2weeks", "online_filter.listed_2weeks"),
    ("1month", "online_filter.listed_1month"),
    ("2months", "online_filter.listed_2months"),
];

/// The currency radios (`OnlineFilter.vue`), in template order.
const CURRENCY: [(&str, &str); 5] = [
    ("", "online_filter.currency_any"),
    ("exalted", "online_filter.currency_only_exalted"),
    ("chaos", "online_filter.currency_only_chaos"),
    ("divine", "online_filter.currency_only_div"),
    ("exalted_divine", "online_filter.currency_exalted_div"),
];

/// A key of the `online_filter` namespace, or one EE2 writes out in English.
fn label(key: &str) -> String {
    key.strip_prefix(':').map_or_else(|| i18n::t(key), |key| i18n::t(&format!("online_filter.{key}")))
}

/// `OnlineFilter.vue`'s label: a history icon, the listing type and the warning colour.
pub fn view<S: 'static>(check: &PriceCheck, _api: Api, project: Project<S>) -> El {
    let filters = check.filters();
    let listing_type = filters.trade.listing_type.as_str();
    let name = match listing_type {
        "securable" => "Instant",
        "any" => "Offline",
        _ => "Online",
    };
    let mut label_text = i18n::t(name);
    if check.league != filters.trade.league {
        label_text.push_str(", ");
        label_text.push_str(&filters.trade.league);
    }
    let warning = show_warning(filters);
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
        .label(label_text.clone())
        .on_press(move |s: &mut S| project(s).toggle_online_filter())
        .child(icon::history(theme::rem(0.75), color))
        .child(text(label_text).key("online-filter-label").font(type_::SM).color(color).truncate())
        .child(el().key("online-filter-spacer"))
}

/// `showWarning`: a restricted listed time or a currency turns the label orange.
fn show_warning(filters: &ItemFilters) -> bool {
    let trade = &filters.trade;
    trade.listed.as_deref().is_some_and(|listed| ["1day", "3days", "1week"].contains(&listed))
        || trade.currency.is_some()
}

/// The panel `OnlineFilter.vue` puts in its popover, anchored under the label.
pub fn panel<S: 'static>(check: &PriceCheck, api: Api, project: Project<S>) -> El {
    if check.filters().trade.listing_type.is_empty() {
        return el();
    }
    let trade = check.filters().trade.clone();
    let by_time = api == Api::Trade;
    let mut left = col().key("online-filter-left").gap(theme::rem(0.25));
    for (value, text) in if by_time { TRADE_TYPES.as_slice() } else { BULK_TYPES.as_slice() } {
        let value = *value;
        left = left.child(radio(
            format!("online-filter-type-{value}"),
            &label(text),
            trade.listing_type == value,
            move |s: &mut S| project(s).set_listing_type(value),
        ));
    }
    if by_time {
        left = left.child(el().h(theme::rem(0.75))).child(radio(
            "online-filter-listed-".to_owned(),
            &label(LISTED[0].1),
            trade.listed.as_deref().is_none_or(str::is_empty),
            move |s: &mut S| project(s).set_listed(None),
        ));
        for (value, text) in &LISTED[1..] {
            let value = *value;
            left = left.child(radio(
                format!("online-filter-listed-{value}"),
                &label(text),
                trade.listed.as_deref() == Some(value),
                move |s: &mut S| project(s).set_listed(Some(value)),
            ));
        }
    }
    let mut right = col().key("online-filter-right").gap(theme::rem(0.25));
    if by_time {
        right = right.child(el().key("online-filter-right-spacer").h(theme::rem(0.75)));
        for (value, text) in &CURRENCY {
            let value = *value;
            right = right.child(radio(
                format!("online-filter-currency-{value}"),
                &label(text),
                trade.currency.as_deref().unwrap_or("") == value,
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
