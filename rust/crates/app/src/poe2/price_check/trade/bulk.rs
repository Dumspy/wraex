//! `TradeBulk.vue` and `pathofexile-bulk.ts`: the currency exchange table, its three
//! totals and the lazy load of a currency the optimistic search left out.
//!
//! PoE2's bulk search trades for `exalted`, `chaos` and `divine` (EE2's three tables),
//! and shows eight rows per table.

use rune::{Align, Color, El, Span, col, el, rich, row, text};
use wraex_core::js;
use wraex_core::relative_time::EpochMillis;
use wraex_core::trade::api::ExchangeResult;

use crate::poe2::i18n;
use crate::poe2::price_check::PriceCheck;
use crate::poe2::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui::icon;
use wraex_poe2::results::AccountStatus;

use super::online;
use super::{BulkCurrency, listing};

/// The price column's width, from the reference screenshot.
const PRICE_WIDTH: f32 = 57.0;
/// The `exalt / bulk` column: `w-8` either side of the separator.
const EXCHANGE_WIDTH: f32 = 86.0;
const STOCK_WIDTH: f32 = 52.0;
const FULFILL_WIDTH: f32 = 58.0;

/// Rows a currency table shows (`SHOW_RESULTS` in `pathofexile-bulk.ts`).
pub const SHOW_RESULTS: usize = 8;
/// Listings the exchange answers with at most (`API_FETCH_LIMIT`).
const API_FETCH_LIMIT: f64 = 100.0;

/// An exchange listing (`FetchResult` of `pathofexile-bulk.ts`).
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ExchangeListing {
    pub id: String,
    pub listing: ExchangeOffers,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct ExchangeOffers {
    pub indexed: String,
    pub offers: Vec<Offer>,
    pub account: Account,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Offer {
    pub exchange: OfferExchange,
    pub item: OfferItem,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct OfferExchange {
    pub currency: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct OfferItem {
    pub amount: f64,
    pub stock: f64,
}

/// `Account` of `common.ts`: only the online status is optional.
#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    pub last_character_name: Option<String>,
    pub online: Option<Online>,
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize)]
pub struct Online {
    pub status: Option<String>,
}

/// A bulk row (`PricingResult` of `pathofexile-bulk.ts`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkPricingResult {
    pub id: String,
    pub relative_date: String,
    pub exchange_amount: f64,
    pub item_amount: f64,
    pub stock: f64,
    pub account_status: AccountStatus,
    pub is_mine: bool,
    pub account_name: String,
    pub ign: String,
}

/// The listings paid in one currency (`BulkSearch`).
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkSearch {
    pub query_id: String,
    pub have_tag: String,
    pub total: f64,
    pub listed: Vec<BulkPricingResult>,
}

/// `execBulkSearch` after the request: one group per `have` currency, cheapest first.
/// A table that would need loading on demand is `None`.
pub fn searches(
    query: &ExchangeResult<ExchangeListing>,
    have: &[String],
    account_name: &str,
    now: EpochMillis,
) -> Vec<Option<BulkSearch>> {
    let results: Vec<&ExchangeListing> = query.result.values().filter(|r| r.listing.offers.len() == 1).collect();

    have.iter()
        .map(|trade_tag| {
            let mut tagged: Vec<&ExchangeListing> =
                results.iter().copied().filter(|r| &r.listing.offers[0].exchange.currency == trade_tag).collect();

            let loaded_on_demand =
                trade_tag == "exalted" && tagged.len() < SHOW_RESULTS && query.total > API_FETCH_LIMIT;
            if loaded_on_demand {
                return None;
            }

            let ratio = |r: &ExchangeListing| {
                let offer = &r.listing.offers[0];
                offer.exchange.amount / offer.item.amount
            };
            // Stable, like `Array.prototype.sort`.
            tagged.sort_by(|a, b| ratio(a).partial_cmp(&ratio(b)).unwrap_or(std::cmp::Ordering::Equal));
            let listed = tagged
                .iter()
                .take(SHOW_RESULTS)
                .map(|r| {
                    let offer = &r.listing.offers[0];
                    let account = &r.listing.account;
                    BulkPricingResult {
                        id: r.id.clone(),
                        relative_date: wraex_core::relative_time::iso_to_relative_short(&r.listing.indexed, now)
                            .unwrap_or_default(),
                        exchange_amount: offer.exchange.amount,
                        item_amount: offer.item.amount,
                        stock: offer.item.stock,
                        account_status: match &account.online {
                            Some(online) if online.status.as_deref() == Some("afk") => AccountStatus::Afk,
                            Some(_) => AccountStatus::Online,
                            None => AccountStatus::Offline,
                        },
                        is_mine: account.name == account_name,
                        account_name: account.name.clone(),
                        ign: account.last_character_name.clone().unwrap_or_default(),
                    }
                })
                .collect();

            let chaos_is_loaded = trade_tag == "divine"
                && tagged.len() < results.len()
                && (results.len() - tagged.len() >= SHOW_RESULTS || query.total <= API_FETCH_LIMIT);

            Some(BulkSearch {
                query_id: query.id.clone(),
                have_tag: trade_tag.clone(),
                // A best guess when the request asked for several `have` currencies.
                total: if chaos_is_loaded {
                    tagged.len() as f64
                } else {
                    query.total - (results.len() - tagged.len()) as f64
                },
                listed,
            })
        })
        .collect()
}

/// `TradeBulk.vue`: error box, or the totals and the table.
pub fn view<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    if let Some(error) = &check.trade.error {
        return super::error_box(error, project);
    }
    col().key("trade-bulk").w_full().min_h(0.0).child(matched(check, project)).child(table(check))
}

/// The three exchange totals, the online label and the trade link.
fn matched<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let results = check.trade.bulk.as_ref();
    let mut counts = row().key("bulk-matched-counts").items_center();
    let label = i18n::fmt("trade_result.matched", &[""]);
    counts = counts.child(
        text(label.trim_end()).key("trade-matched-label").font(type_::SM).color(theme::gray(500)).mr(theme::rem(0.25)),
    );
    if results.is_none() {
        return row()
            .key("trade-matched")
            .w_full()
            .mb(theme::rem(0.5))
            .pl(theme::rem(0.5))
            .items_center()
            .child(counts.child(text("...").key("trade-matched-total").font(type_::SM).color(theme::gray(600))));
    }
    for which in BulkCurrency::ALL {
        counts = counts.child(currency_button(check, which, project));
    }
    counts = counts.child(row().key("bulk-online").ml(theme::rem(0.25)).child(online::view(
        check,
        wraex_poe2::trade::Api::Bulk,
        project,
    )));
    let mut line =
        row().key("trade-matched").w_full().pl(theme::rem(0.5)).mb(theme::rem(0.5)).items_center().child(counts);
    if results.is_some() {
        line = line.child(el().grow(1.0)).child(super::links::trade_links(check, project));
    }
    line
}

/// One of the three currency buttons: its icon and how many offers it has (`?` until the
/// lazy load asks for it), on `bg-gray-700` while selected.
fn currency_button<S: 'static>(check: &PriceCheck, which: BulkCurrency, project: Project<S>) -> El {
    let total = check.trade.bulk.as_ref().and_then(|results| results.get(which));
    let label = total.map_or_else(|| "?".to_owned(), |group| js::number_to_string(group.total));
    row()
        .key(format!("bulk-currency-{}", which.have()))
        .mr(theme::rem(0.25))
        .px(theme::rem(0.5))
        .py(theme::rem(0.25))
        .items_center()
        .radius(theme::ROUNDED)
        .fill(if check.trade.selected == which { theme::gray(700) } else { Color::TRANSPARENT })
        .focusable(true)
        .role("button")
        .label(format!("{} {label}", which.have()))
        .on_press(move |s: &mut S| project(s).select_currency(which))
        // `$style.currencyIcon`: 1.75 rem with -0.4375 rem above and below, greyscale.
        .child(icon::image_placeholder(theme::rem(1.75)).mr(theme::rem(0.125)).my(-(theme::rem(0.4375))))
        .child(text(label).key(format!("bulk-currency-{}-total", which.have())).font(type_::SM).color(theme::gray(200)))
}

/// The exchange table.
fn table(check: &PriceCheck) -> El {
    let heading = row()
        .key("bulk-head")
        .w_full()
        .h(theme::rem(1.375))
        .items_stretch()
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
        .border_layout()
        .child(heading_cell("bulk-head-price", &i18n::t("trade_result.price"), PRICE_WIDTH))
        .child(exchange_heading(check.trade.selected))
        .child(heading_cell("bulk-head-stock", &i18n::t("trade_result.stock"), STOCK_WIDTH))
        .child(heading_cell("bulk-head-fulfill", &i18n::t("trade_result.fulfill"), FULFILL_WIDTH))
        .child(heading_listed("bulk-head-listed"));

    let rows: &[BulkPricingResult] = check
        .trade
        .bulk
        .as_ref()
        .and_then(|results| results.get(check.trade.selected))
        .map_or(&[], |group| group.listed.as_slice());

    let mut body = col().key("bulk-rows").w_full();
    for (index, result) in rows.iter().enumerate() {
        body = body.child(
            listing::row_for(index)
                .child(price_cell(index, result))
                .child(exchange_cell(index, result))
                .child(number_cell(result.stock, &format!("bulk-{index}-stock"), STOCK_WIDTH, true))
                .child(fulfill_cell(index, result))
                .child(listed_cell(index, result)),
        );
    }
    for index in rows.len()..listing::SHOW_RESULTS {
        body = body.child(listing::empty_row(index));
    }
    col().key("bulk-table").w_full().min_h(0.0).child(heading).child(body)
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

/// The `exalt / bulk` heading: `w-8` either side of a thin-space slash.
fn exchange_heading(selected: BulkCurrency) -> El {
    let currency = match selected {
        BulkCurrency::Exalted => "exalt",
        BulkCurrency::Chaos => "chaos",
        BulkCurrency::Divine => "div",
    };
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
                .align(Align::End),
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
fn price_cell(index: usize, result: &BulkPricingResult) -> El {
    let price = result.exchange_amount / result.item_amount;
    let label =
        js::to_fixed(price, 4).parse::<f64>().map_or_else(|_| js::number_to_string(price), js::number_to_string);
    row()
        .w(PRICE_WIDTH)
        .pl(theme::rem(0.5))
        .items_center()
        .child(text(label).key(format!("bulk-{index}-price")).font(type_::SM).color(theme::gray(200)))
}

/// `exchangeAmount` over a thin-space slash over `itemAmount`, as sent.
fn exchange_cell(index: usize, result: &BulkPricingResult) -> El {
    row()
        .key(format!("bulk-{index}-exchange"))
        .w(EXCHANGE_WIDTH)
        .pl(theme::rem(0.25))
        .pr(theme::rem(0.5))
        .items_center()
        .child(
            text(js::number_to_string(result.exchange_amount))
                .key(format!("bulk-{index}-exchange-left"))
                .font(type_::SM)
                .color(theme::gray(200))
                .w(theme::rem(2.0))
                .align(Align::End),
        )
        .child(
            text("\u{2009}/\u{2009}").key(format!("bulk-{index}-exchange-sep")).font(type_::SM).color(theme::gray(200)),
        )
        .child(
            text(js::number_to_string(result.item_amount))
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
    stack = stack.child(listing::account_status(index, false, result.account_status));
    let date = result.relative_date.clone();
    stack = stack.child(
        rich(date.clone(), vec![Span::new(0, date.len()).mono()])
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn listing(id: &str, currency: &str, exchange: f64, item: f64, stock: f64) -> serde_json::Value {
        json!({
            "id": id,
            "listing": {
                "indexed": "2026-11-24T08:00:00Z",
                "offers": [{ "exchange": { "currency": currency, "amount": exchange },
                             "item": { "amount": item, "stock": stock } }],
                "account": { "name": "someone", "lastCharacterName": "Someone" }
            }
        })
    }

    fn answer(entries: Vec<(&str, serde_json::Value)>, total: f64) -> ExchangeResult<ExchangeListing> {
        let result = entries.into_iter().map(|(id, value)| (id.to_owned(), value)).collect::<serde_json::Map<_, _>>();
        serde_json::from_value(json!({ "id": "query", "total": total, "result": result })).expect("an exchange answer")
    }

    /// `execBulkSearch`: one group per `have` currency, cheapest first, and the divine
    /// table's total corrected for the currencies the optimistic search lumped together.
    #[test]
    fn exchange_offers_group_by_currency_cheapest_first() {
        let answer = answer(
            vec![
                ("a", listing("a", "divine", 2.0, 1.0, 40.0)),
                ("b", listing("b", "divine", 1.0, 1.0, 9.0)),
                ("c", listing("c", "chaos", 300.0, 1.0, 500.0)),
            ],
            3.0,
        );
        let groups = searches(&answer, &["divine".into(), "exalted".into(), "chaos".into()], "", 0);
        let divine = groups[0].as_ref().expect("a divine table");
        assert_eq!(divine.have_tag, "divine");
        // Cheapest first, and the total is the offers it found: the chaos offer isn't one.
        assert_eq!(divine.listed.iter().map(|row| row.exchange_amount).collect::<Vec<_>>(), [1.0, 2.0]);
        assert_eq!(divine.total, 2.0);
        // Eight rows is `SHOW_RESULTS`: three offers here, so nothing else is listed.
        assert_eq!(divine.listed.len(), 2);
        assert_eq!(divine.listed[0].exchange_amount / divine.listed[0].item_amount, 1.0);
        assert_eq!(groups[2].as_ref().expect("a chaos table").listed.len(), 1);
    }

    /// The exalt table is loaded on demand once an answer it would have to page through is
    /// long enough (`loadedOnDemand`).
    #[test]
    fn a_long_answer_defers_the_exalt_table() {
        let answer = answer(
            vec![("a", listing("a", "divine", 1.0, 1.0, 9.0)), ("b", listing("b", "chaos", 300.0, 1.0, 500.0))],
            150.0,
        );
        let groups = searches(&answer, &["divine".into(), "exalted".into(), "chaos".into()], "", 0);
        assert!(groups[1].is_none(), "the exalt table waits for its own search");
        // The divine table's total is the answer's, less the offers paying another currency.
        assert_eq!(groups[0].as_ref().unwrap().total, 149.0);
    }
}
