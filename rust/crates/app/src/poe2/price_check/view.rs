//! `PriceCheckWindow.vue` and `CheckedItem.vue` in Rune: the window's frame and the item it
//! shows.
//!
//! Ported: the window and its title bar (with the divine rate), the item header
//! (`FilterName.vue`), the trend row (`PriceTrend.vue`), the filters block
//! (`FiltersBlock.vue`), the Search row and the listings (`TradeListing.vue`,
//! `TradeBulk.vue`, `OnlineFilter.vue`, `TradeLinks.vue`).
//!
//! Not ported: the price sparkline and the volume block of `PriceTrend.vue` (poe.ninja's
//! volume needs a graph widget), `StackValue.vue`, the tips and support links, the
//! unidentified resolver and the related-items panel (all outside the price-check window's
//! own layout).

use rune::{Cx, El, col, el, row, text};
use wraex_core::js;
use wraex_game::ParseError;
use wraex_poe2::filters::create_item_filters::category_to_trade_id;
use wraex_poe2::filters::{ItemFilters, SearchFilter};
use wraex_poe2::parser::item::Item;
use wraex_poe2::prices::{DbQuery, display_rounding};
use wraex_poe2::trade::Api;

use crate::poe2::i18n;
use crate::poe2::price_check::trade;
use crate::poe2::price_check::{PriceCheck, WIDTH};
use crate::theme::{self, text as type_};
use crate::ui::art;
use crate::ui::{self, icon};

/// How a view reaches its own state. Rune runs a handler with the app's state (`&mut S`), so a
/// window that isn't the whole app is rendered with the projection that finds it.
pub type Project<S> = fn(&mut S) -> &mut PriceCheck;

/// `FilterName.vue`'s label when the filters carry neither a name, a base type nor a category.
const NO_LABEL: &str = "??? Report if you see this text";

/// The price-check window: `#price-window`, 28.75 rem wide.
pub fn view<S: 'static>(check: &PriceCheck, cx: &mut Cx<S>, project: Project<S>) -> El {
    if !check.open {
        return el().key("price-check");
    }
    // `text-gray-200` on `#price-window`, inherited by everything inside it.
    cx.foreground(theme::gray(200));
    let title_bar =
        ui::title_bar(check.title()).leading(divine_rate(check)).on_close(move |s: &mut S| project(s).close()).build();
    col()
        .key("price-check")
        .w(WIDTH)
        .h_full()
        .fill(theme::gray(800))
        // The card's background covers the whole card; nothing draws outside it.
        .clip_rounded(0.0)
        .child(title_bar)
        .child(body(check, project))
}

/// `PriceCheckWindow.vue`'s `<slot />`: the exchange rate (`stableOrbCost` divines in the
/// core currency), or an empty `w-8` spacer until poe.ninja's answer is in.
fn divine_rate(check: &PriceCheck) -> El {
    match (check.prices.xchg_rate, check.prices.xchg_rate_currency) {
        (Some(rate), Some(_)) => row()
            .key("titlebar-rate")
            .h_full()
            .px(theme::rem(0.5))
            .pt(1.0)
            .items_center()
            .gap(theme::rem(0.25))
            .focusable(true)
            .role("button")
            .label("exchange rate")
            .child(icon::exchange(theme::rem(0.875), theme::gray(600)))
            .child(
                text(js::number_to_string(rate.round()))
                    .key("titlebar-rate-text")
                    .font(type_::SM)
                    .color(theme::gray(600)),
            ),
        _ => el().key("titlebar-slot").w(theme::rem(2.0)).h_full(),
    }
}

/// The part under the title bar: an error, or the item.
fn body<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let mut body = col().key("price-check-body").w_full().grow(1.0).min_h(0.0);
    match &check.item {
        Err(ParseError::Rejected { key }) => {
            // `t(item.error.name)` and `t(item.error.message)`: `item.parse_error` with
            // `item.parse_error_help`, or `item.unknown` with `item.unknown_help`.
            let name = i18n::t(key);
            let help = i18n::t(&format!("{key}_help"));
            body = body
                .child(
                    ui::error_box(&name, &help)
                        .mx(theme::rem(1.0))
                        .mt(theme::rem(1.0))
                        .child(ui::actions([reload_trade_data("error-reload")])),
                )
                // `<pre class="bg-gray-900 rounded m-4 overflow-x-hidden p-2">`: the reference
                // lays the lines out as they are, and a trailing newline adds no line.
                .child(
                    text(check.text.trim_end())
                        .key("error-raw-text")
                        .font(type_::SM)
                        .by_line()
                        .mx(theme::rem(1.0))
                        .mb(theme::rem(1.0))
                        .padding(theme::rem(0.5))
                        .radius(theme::ROUNDED)
                        .fill(theme::gray(900)),
                );
        }
        Ok(item) => {
            // `p-4 layout-column min-h-0`
            let mut checked = col()
                .key("checked-item")
                .w_full()
                .grow(1.0)
                .padding(theme::rem(1.0))
                .min_h(0.0)
                .child(filter_name(check.active_preset().map(|preset| &preset.filters), project));
            if let Some(trend) = trend(check, item) {
                checked = checked.child(trend);
            }
            checked = checked.child(crate::poe2::price_check::filters::view(check, project));
            checked =
                checked.child(if check.do_search { trade::view(check, project) } else { search_row(check, project) });
            body = body.child(checked);
        }
    }
    body
}

/// `ReloadTradeData.vue`'s button, as the parse error's action.
fn reload_trade_data(key: &str) -> El {
    ui::button(key, &i18n::t("Reload fallback data")).min_w(theme::rem(4.0))
}

/// `FilterName.vue`: the item name (or its category) on a dark bar, with the corruption state
/// on the right.
fn filter_name<S: 'static>(filters: Option<&ItemFilters>, project: Project<S>) -> El {
    let label = filters.map_or_else(|| NO_LABEL.to_owned(), name_label);
    let active = filters.is_some_and(show_as_active);
    let name = row()
        .key("filter-name")
        .grow(1.0)
        .min_w(0.0)
        .px(theme::rem(0.5))
        .radius(theme::ROUNDED)
        // `border-gray-500` while the exact search is the active one, `border-gray-900` (the
        // bar's own colour) while the relaxed search is: the border only shows the difference.
        .border(1.0, if active { theme::gray(500) } else { theme::gray(900) })
        // A CSS border takes room in the box, so the 20 pt line inside makes the button 22 pt
        // tall, as upstream's bordered `.px-2` button is.
        .border_layout()
        .center_y()
        .focusable(true)
        .role("button")
        .on_press(move |s: &mut S| project(s).toggle_accuracy())
        .child(text(label).key("filter-name-text").font(type_::SM).truncate());
    let mut bar = row()
        .key("filter-name-bar")
        .w_full()
        .justify_between()
        .fill(theme::gray(900))
        .radius(theme::ROUNDED)
        .mb(theme::rem(0.5))
        .child(name);
    if let Some(corrupted) = filters.and_then(|filters| filters.corrupted.as_ref()) {
        let (label, color) = if corrupted.value {
            (i18n::t("item.corrupted"), theme::red(500))
        } else {
            (i18n::t("item.not_corrupted"), theme::gray(600))
        };
        bar = bar.child(
            row()
                .key("corrupted")
                .px(theme::rem(0.5))
                .center_y()
                .focusable(true)
                .role("button")
                .on_press(move |s: &mut S| project(s).toggle_corrupted())
                .child(text(label).key("corrupted-label").font(type_::SM).color(color)),
        );
    }
    bar
}

/// `FilterName.vue`'s `label`: the active search's item name, else its base type, else
/// `Category: <the trade site's name for the category>`.
fn name_label(filters: &ItemFilters) -> String {
    let active = active_search(filters);
    if let Some(name) = &active.name {
        return name.clone();
    }
    if let Some(base_type) = &active.base_type {
        return base_type.clone();
    }
    if let Some(category) = &active.category {
        let id = category_to_trade_id(category).unwrap_or_default().replace('.', "_");
        return i18n::fmt("item_category.prop", &[&i18n::t(&format!("item_category.{id}"))]);
    }
    NO_LABEL.to_owned()
}

/// `FilterName.vue`'s `showAsActive`: `filters.searchRelaxed?.disabled`.
fn show_as_active(filters: &ItemFilters) -> bool {
    filters.search_relaxed.as_ref().and_then(|relaxed| relaxed.disabled).unwrap_or(false)
}

/// The search `FilterName.vue` reads: the relaxed one unless it's disabled, else the exact one.
fn active_search(filters: &ItemFilters) -> &SearchFilter {
    match &filters.search_relaxed {
        Some(relaxed) if relaxed.disabled != Some(true) => relaxed,
        _ => &filters.search_exact,
    }
}

/// `PriceTrend.vue`: the item's price from poe.ninja, or — for anything poe.ninja has no
/// entry for and that `item.info.craftable` does not mark as craftable — the item's icon, an
/// arrow and `?`. The sparkline and the volume block are not ported.
fn trend(check: &PriceCheck, item: &Item) -> Option<El> {
    let info = item.info();
    let query = DbQuery {
        ns: info.namespace.as_str().to_owned(),
        name: info.ref_name.clone()?,
        variant: info.unique.as_ref().map(|unique| unique.base.clone()),
    };
    // `ItemQuickPrice.vue`'s `imgSize`: a tall base type is `w-8 h-10`, a wide one `w-12 h-8`.
    let (icon_w, icon_h) = match (info.w.unwrap_or(1), info.h.unwrap_or(1)) {
        (_, h) if h > 1 => (theme::rem(2.0), theme::rem(2.5)),
        (w, _) if w > 1 => (theme::rem(3.0), theme::rem(2.0)),
        _ => (theme::rem(2.0), theme::rem(2.0)),
    };
    // `:item-img="item.info.icon"`: the item's own art on the CDN, and `UiItemImg.vue`'s
    // `404.png` for the data's missing icon. Built per drawn row only: a row the reference
    // drops must not start a load.
    let item_art = || {
        let size = (icon_w, icon_h);
        if info.icon.is_empty() || info.icon == "%NOT_FOUND%" {
            art::local_sized(wraex_core::GameId::Poe2, "404.png", size).key("trend-item-icon")
        } else {
            art::cdn(wraex_core::GameId::Poe2, &info.icon, size).key("trend-item-icon")
        }
    };
    let arrow = || icon::arrow_right(theme::rem(0.875), theme::rem(0.875), theme::gray(600)).mx(theme::rem(0.25));
    match check.prices.find_price(&query) {
        Some(price) => {
            let fraction = item.stack_size.is_some();
            let value =
                check.prices.auto_currency(price.info.primary_value, info.ref_name.as_deref() == Some("Divine Orb"));
            let min = display_rounding(value.min, fraction, false);
            let max = display_rounding(value.max, fraction, false);
            let mut line =
                row().grow(1.0).justify_center().items_center().gap(theme::rem(0.25)).child(item_art()).child(arrow());
            line = line.child(text(min.clone()).key("trend-price").font(type_::BASE));
            if min != max {
                line = line
                    .child(text(" ~ ").font(type_::BASE).color(theme::gray(600)))
                    .child(text(max).key("trend-price-max").font(type_::BASE));
            }
            line = line.child(text("×").key("trend-times").font(type_::BASE).color(theme::gray(600)));
            line = line.child(currency_icon(value.currency, theme::rem(2.0)).key("trend-currency-icon"));
            Some(row().key("trend").w_full().min_h(theme::rem(3.0)).pb(theme::rem(1.0)).items_center().child(line))
        }
        // `<div v-else-if="!item.info.craftable">`: `ItemQuickPrice` with no price and
        // `currency-text`, so the row is the icon, the arrow and a `?`.
        None if info.craftable.is_none() => Some(
            row().key("trend").w_full().min_h(theme::rem(3.0)).pb(theme::rem(1.0)).items_center().child(
                row()
                    .grow(1.0)
                    .justify_center()
                    .items_center()
                    .gap(theme::rem(0.25))
                    .child(item_art())
                    .child(arrow())
                    .child(text("?").key("trend-price").font(type_::BASE).px(theme::rem(0.25))),
            ),
        ),
        None => None,
    }
}

/// `CoreCurrencyImg.vue`/`ItemQuickPrice.vue`'s image for a price's currency: `divine`,
/// `chaos` and `annul` by name, `exa` for anything else (`exalted`).
fn currency_icon(currency: &str, size: f32) -> El {
    let name = match currency {
        "div" => "divine.png",
        "chaos" => "chaos.png",
        "annul" => "annul.png",
        _ => "exa.png",
    };
    art::local(wraex_core::GameId::Poe2, name, size)
}

/// `CheckedItem.vue`'s `!doSearch` row: the Search button, and the trade-site link for a
/// trade search (`trade-links v-if="tradeAPI === 'trade'"`).
fn search_row<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let trade = check.trade.api(check) == Api::Trade;
    row()
        .key("search-row")
        .w_full()
        .items_center()
        .justify_between()
        // `<div class="flex w-40">` around the button, which is at least `min-width: 5rem`.
        .child(
            row().key("search-group").w(theme::rem(10.0)).child(
                ui::button("search", &i18n::t("Search"))
                    .min_w(theme::rem(5.0))
                    .focusable(true)
                    .on_press(move |s: &mut S| project(s).start_search()),
            ),
        )
        .child(
            row()
                .key("search-links")
                .gap(theme::rem(0.25))
                .children(trade.then(|| trade::links::trade_links(check, project))),
        )
}
