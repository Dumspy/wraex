//! `TradeBulk.vue`: the currency exchange table, its two totals and the market ratio.
//!
//! `useMarketRatioFinder`'s row sits among the offers at its price ([`market_ratio`]).
//! Not ported: the seller column (`showSeller: false` by default in the reference too).

use rune::{Color, El, col, el, row, text};
use wraex_core::js;
use wraex_poe1::results::BulkPricingResult;

use crate::poe1::i18n;
use crate::poe1::price_check::PriceCheck;
use crate::poe1::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui::art;
use crate::ui::icon;

use super::online;
use super::{BulkCurrency, listing, market_ratio};

/// The price column's width, from the reference screenshot.
const PRICE_WIDTH: f32 = 57.0;
/// The `chaos / bulk` column: `w-8` either side of the separator.
const EXCHANGE_WIDTH: f32 = 86.0;
const STOCK_WIDTH: f32 = 52.0;
const FULFILL_WIDTH: f32 = 58.0;

/// `TradeBulk.vue`: error box, or the legacy message, the totals and the table.
pub fn view<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    if let Some(error) = &check.trade.error {
        return super::error_box(error, project);
    }
    col()
        .key("trade-bulk")
        .w_full()
        .min_h(0.0)
        .child(listing::legacy_message())
        .child(matched(check, project))
        .child(table(check))
}

/// The two exchange totals and the online label.
fn matched<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let results = check.trade.bulk.as_ref();
    let mut counts = row().key("bulk-matched-counts").items_center();
    let label = i18n::fmt("trade_result.matched", &[""]);
    counts = counts.child(
        text(label.trim_end()).key("trade-matched-label").font(type_::SM).color(theme::gray(500)).pr(theme::rem(0.25)),
    );
    if results.is_none() {
        return row()
            .key("trade-matched")
            .w_full()
            .pl(theme::rem(0.5))
            .mb(theme::rem(0.5))
            .items_center()
            .child(counts.child(text("...").key("trade-matched-total").font(type_::SM).color(theme::gray(600))));
    }
    for which in [BulkCurrency::Chaos, BulkCurrency::Stable] {
        let selected = check.trade.selected == which;
        counts = counts.child(currency_button(check, which, selected, project));
    }
    counts = counts.child(row().key("bulk-online").ml(theme::rem(0.25)).child(online::view(
        check,
        super::Api::Bulk,
        project,
    )));
    let mut line =
        row().key("trade-matched").w_full().pl(theme::rem(0.5)).mb(theme::rem(0.5)).items_center().child(counts);
    if results.is_some() {
        line = line.child(el().grow(1.0)).child(super::links::trade_links(check, project));
    }
    line
}

/// One of the two currency buttons: its icon and how many offers it has (`?` until the
/// lazy load asks for it), on `bg-gray-700` while selected.
fn currency_button<S: 'static>(check: &PriceCheck, which: BulkCurrency, selected: bool, project: Project<S>) -> El {
    let total = check.trade.bulk.as_ref().and_then(|results| match which {
        BulkCurrency::Chaos => results.chaos.as_ref(),
        BulkCurrency::Stable => results.stable.as_ref(),
    });
    let label = total.map_or_else(|| "?".to_owned(), |group| js::number_to_string(group.total));
    let key = match which {
        BulkCurrency::Chaos => "chaos",
        BulkCurrency::Stable => "divine",
    };
    row()
        .key(format!("bulk-currency-{key}"))
        .mr(theme::rem(0.25))
        .px(theme::rem(0.5))
        .py(theme::rem(0.25))
        .items_center()
        .radius(theme::ROUNDED)
        .fill(if selected { theme::gray(700) } else { Color::TRANSPARENT })
        .focusable(true)
        .role("button")
        .label(format!("{key} {label}"))
        .on_press(move |s: &mut S| project(s).select_currency(which))
        // `$style.currencyIcon`: 1.75 rem with -0.4375 rem above and below, so it does not
        // grow the button (upstream also greys it out; Rune has no colour filter).
        .child(
            art::local(wraex_core::GameId::Poe1, &format!("{key}.png"), theme::rem(1.75))
                .mr(theme::rem(0.125))
                .my(-(theme::rem(0.4375)))
                .key(format!("bulk-currency-{key}-icon")),
        )
        .child(text(label).key(format!("bulk-currency-{key}-total")).font(type_::SM).color(theme::gray(200)))
}

/// The exchange table.
fn table(check: &PriceCheck) -> El {
    let stable = check.trade.selected == BulkCurrency::Stable;
    let heading = row()
        .key("bulk-head")
        .w_full()
        .h(theme::rem(1.375))
        .items_stretch()
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
        .border_layout()
        .child(heading_cell("bulk-head-price", &i18n::t("trade_result.price"), PRICE_WIDTH))
        .child(exchange_heading(stable))
        .child(heading_cell("bulk-head-stock", &i18n::t("trade_result.stock"), STOCK_WIDTH))
        .child(heading_cell("bulk-head-fulfill", &i18n::t("trade_result.fulfill"), FULFILL_WIDTH))
        .child(heading_listed("bulk-head-listed"));

    let columns = if stable {
        check.trade.bulk.as_ref().and_then(|results| results.stable.as_ref())
    } else {
        check.trade.bulk.as_ref().and_then(|results| results.chaos.as_ref())
    };
    let listed: &[BulkPricingResult] = columns.map_or(&[], |group| group.listed.as_slice());

    // `selectedResults`: the ratio merged into the first nineteen offers, twenty rows in all.
    let rows = match market_ratio_row(check) {
        Some(ratio) => market_ratio::merge(&listed[..listed.len().min(19)], ratio),
        None => (0..listed.len().min(listing::SHOW_RESULTS)).map(market_ratio::Row::Offer).collect(),
    };

    let mut body = col().key("bulk-rows").w_full();
    for (index, row) in rows.iter().enumerate() {
        let line = match row {
            market_ratio::Row::Offer(offer) => {
                let result = &listed[*offer];
                listing::row_for(index)
                    .child(price_cell(index, &result.exchange_amount, &result.item_amount))
                    .child(exchange_cell(index, &result.exchange_amount, &result.item_amount))
                    .child(number_cell(result.stock, &format!("bulk-{index}-stock"), STOCK_WIDTH, true))
                    .child(fulfill_cell(index, result))
                    .child(listed_cell(index, result))
            }
            market_ratio::Row::Ratio(ratio) => ratio_row(index, ratio),
        };
        body = body.child(line);
    }
    for index in rows.len()..listing::SHOW_RESULTS {
        body = body.child(listing::empty_row(index));
    }
    col().key("bulk-table").w_full().min_h(0.0).child(heading).child(body)
}

/// `useMarketRatioFinder`'s row for the open item, when the league and the price data
/// allow one.
fn market_ratio_row(check: &PriceCheck) -> Option<market_ratio::MarketRatio> {
    let item = check.item()?;
    let filters = check.item_filters()?;
    // `if (filters.trade.league !== defaultLeague.value?.id) return`
    if filters.trade.league != check.league {
        return None;
    }
    let rows = market_ratio::find(check.prices.prices(), item);
    market_ratio::for_currency(rows, check.trade.selected == BulkCurrency::Stable)
}

fn heading_cell(key: &str, label: &str, width: f32) -> El {
    row()
        .key(key)
        .w(width)
        .h_full()
        .items_center()
        .px(theme::rem(0.25))
        .child(text(label.to_owned()).key(format!("{key}-text")).font(type_::SM).color(theme::gray(200)))
}

/// The `chaos / bulk` heading: `w-8` either side of a thin-space slash.
fn exchange_heading(stable: bool) -> El {
    let currency = if stable { "div" } else { "chaos" };
    row()
        .key("bulk-head-exchange")
        .w(EXCHANGE_WIDTH)
        .h_full()
        .items_center()
        .pl(theme::rem(0.25))
        .pr(theme::rem(0.5))
        .gap(0.0)
        .child(
            text(currency)
                .key("bulk-head-exchange-left")
                .font(type_::XS)
                .color(theme::gray(200))
                .w(theme::rem(2.0))
                .align(rune::Align::End),
        )
        .child(text("\u{2009}/\u{2009}").key("bulk-head-exchange-sep").font(type_::XS).color(theme::gray(200)))
        .child(
            text(i18n::t("trade_result.bulk"))
                .key("bulk-head-exchange-right")
                .font(type_::XS)
                .color(theme::gray(200))
                .w(theme::rem(2.0)),
        )
}

fn heading_listed(key: &str) -> El {
    row().key(key).h_full().grow(1.0).min_w(0.0).items_center().pl(theme::rem(1.0)).pr(theme::rem(0.5)).child(
        text(i18n::t("trade_result.listed"))
            .key(format!("{key}-text"))
            .font(type_::SM)
            .color(theme::gray(200))
            .pl(theme::rem(0.625)),
    )
}

/// `Number((exchangeAmount / itemAmount).toFixed(4))`.
fn price_cell(index: usize, exchange_amount: &f64, item_amount: &f64) -> El {
    let price = exchange_amount / item_amount;
    let label =
        js::to_fixed(price, 4).parse::<f64>().map_or_else(|_| js::number_to_string(price), js::number_to_string);
    row()
        .w(PRICE_WIDTH)
        .pl(theme::rem(0.5))
        .items_center()
        .child(text(label).key(format!("bulk-{index}-price")).font(type_::SM).color(theme::gray(200)))
}

/// The market ratio's own row (`.marketRatioRow`): `bg-gray-700` with a `gray-600` rim,
/// and its label across the columns an offer fills.
fn ratio_row(index: usize, ratio: &market_ratio::MarketRatio) -> El {
    let amount = |value: f64| {
        js::to_fixed(value, 1).parse::<f64>().map_or_else(|_| js::number_to_string(value), js::number_to_string)
    };
    listing::row_for(index)
        .fill(theme::gray(700))
        .border(1.0, theme::gray(600))
        .child(price_cell(index, &ratio.exchange_amount, &ratio.item_amount))
        .child(
            row()
                .key(format!("bulk-{index}-exchange"))
                .w(EXCHANGE_WIDTH)
                .pl(theme::rem(0.25))
                .pr(theme::rem(0.5))
                .items_center()
                .child(
                    text(amount(ratio.exchange_amount))
                        .key(format!("bulk-{index}-exchange-left"))
                        .font(type_::SM)
                        .color(theme::gray(200))
                        .w(theme::rem(2.0))
                        .align(rune::Align::End),
                )
                .child(
                    text("\u{2009}/\u{2009}")
                        .key(format!("bulk-{index}-exchange-sep"))
                        .font(type_::SM)
                        .color(theme::gray(200)),
                )
                .child(
                    text(amount(ratio.item_amount))
                        .key(format!("bulk-{index}-exchange-right"))
                        .font(type_::SM)
                        .color(theme::gray(200))
                        .w(theme::rem(2.0)),
                ),
        )
        // `<td :colspan="3" class="pl-1 pr-2 whitespace-nowrap text-center">`
        .child(
            row()
                .key(format!("bulk-{index}-ratio-cell"))
                .grow(1.0)
                .pl(theme::rem(0.25))
                .pr(theme::rem(0.5))
                .justify_center()
                .child(
                    text(i18n::t("trade_result.market_ratio"))
                        .key(format!("bulk-{index}-market-ratio"))
                        .font(type_::SM)
                        .color(theme::gray(200)),
                ),
        )
}

/// `Number(exchange.toFixed(1))` over a thin-space slash over `Number(item.toFixed(1))`.
fn exchange_cell(index: usize, exchange_amount: &f64, item_amount: &f64) -> El {
    let amount = |value: f64| {
        js::to_fixed(value, 1).parse::<f64>().map_or_else(|_| js::number_to_string(value), js::number_to_string)
    };
    row()
        .key(format!("bulk-{index}-exchange"))
        .w(EXCHANGE_WIDTH)
        .pl(theme::rem(0.25))
        .pr(theme::rem(0.5))
        .items_center()
        .child(
            text(amount(*exchange_amount))
                .key(format!("bulk-{index}-exchange-left"))
                .font(type_::SM)
                .color(theme::gray(200))
                .w(theme::rem(2.0))
                .align(rune::Align::End),
        )
        .child(
            text("\u{2009}/\u{2009}").key(format!("bulk-{index}-exchange-sep")).font(type_::SM).color(theme::gray(200)),
        )
        .child(
            text(amount(*item_amount))
                .key(format!("bulk-{index}-exchange-right"))
                .font(type_::SM)
                .color(theme::gray(200))
                .w(theme::rem(2.0)),
        )
}

fn number_cell(value: f64, key: &str, width: f32, right: bool) -> El {
    let mut cell = row().w(width).px(theme::rem(0.25)).items_center();
    cell = if right { cell.justify_end() } else { cell };
    cell.child(text(js::number_to_string(value)).key(key).font(type_::SM).color(theme::gray(200)))
}

/// `Math.floor(stock / itemAmount)`, with the warning icon when the stock is short.
fn fulfill_cell(index: usize, result: &BulkPricingResult) -> El {
    let mut cell = row().w(FULFILL_WIDTH).px(theme::rem(0.25)).items_center().justify_end();
    if result.stock < result.item_amount {
        cell = cell.child(icon::warning(theme::rem(0.875), theme::gray(500)).mr(theme::rem(0.25)));
    }
    let fulfill = (result.stock / result.item_amount).floor();
    cell.child(
        text(js::number_to_string(fulfill))
            .key(format!("bulk-{index}-fulfill"))
            .font(type_::SM)
            .color(theme::gray(200)),
    )
}

fn listed_cell(index: usize, result: &BulkPricingResult) -> El {
    let mut cell = row().grow(1.0).min_w(0.0).pl(theme::rem(1.0)).pr(theme::rem(0.5)).items_center();
    let mut stack = row().items_center().gap(theme::rem(0.25));
    stack = stack.child(listing::account_status(index, result.account_status));
    let date = result.relative_date.clone();
    stack = stack.child(
        rune::rich(date.clone(), vec![rune::Span::new(0, date.len()).mono()])
            .key(format!("bulk-{index}-date"))
            .font(type_::XS)
            .color(theme::gray(200)),
    );
    cell = cell.child(stack);
    if result.is_mine {
        cell = cell.child(listing::mine_badge(index));
    }
    cell
}
