//! `TradeListing.vue` and `TradeItem.vue`: the matched row, the online filter and the
//! listings table (a row is [`TradeItem`](https://github.com/Kvan7/Exiled-Exchange-2)'s
//! `TradeItem.vue`, folded in here).

use rune::{Color, El, Span, col, el, rich, row, text};
use wraex_core::js;
use wraex_poe2::parser::meta::{GEM, GRANTS_REAL_SKILL};
use wraex_poe2::results::{AccountStatus, PricingResult};

use crate::poe2::i18n;
use crate::poe2::price_check::PriceCheck;
use crate::poe2::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui;

use super::online;

/// `SHOW_RESULTS`: rows a table always shows, empty ones included.
pub const SHOW_RESULTS: usize = 20;

/// The price column's width, measured off the reference screenshots (px-2 either side).
const PRICE_WIDTH: f32 = 72.0;
const LEVEL_WIDTH: f32 = 44.0;
const GEM_WIDTH: f32 = 44.0;
const QUALITY_WIDTH: f32 = 56.0;
/// A table row's height: `text-sm`'s 20 pt line plus the reference's 2 pt strut.
pub const ROW_HEIGHT: f32 = 22.0;

/// A row of the table: a `PricingResult` and how many listings it stands for
/// (`PricingResult & { listedTimes: number }`).
#[derive(Debug, Clone)]
pub struct Grouped {
    pub result: PricingResult,
    pub listed_times: u32,
}

/// `groupedResults`: listings by the same seller at the same price (or one of the last
/// two rows) fold into one, adding up stacks when they have them.
pub fn grouped(rows: &[PricingResult]) -> Vec<Grouped> {
    let mut out: Vec<Grouped> = Vec::new();
    for result in rows {
        if out.is_empty() {
            out.push(Grouped { result: result.clone(), listed_times: 1 });
            continue;
        }
        let last = out.len();
        let found = out.iter().enumerate().position(|(index, added)| {
            (added.result.account_name == result.account_name
                && added.result.price_currency == result.price_currency
                && added.result.price_amount == result.price_amount)
                // `out.length - idx <= 2`: the last row or the one before it.
                || (added.result.account_name == result.account_name && last - index <= 2)
        });
        match found {
            Some(index) => {
                let existing = &mut out[index];
                let stack = existing.result.stack_size.as_ref().and_then(|stack| stack.as_f64()).filter(|v| *v != 0.0);
                match stack {
                    Some(stack) => {
                        let added = result.stack_size.as_ref().and_then(|size| size.as_f64()).unwrap_or(0.0);
                        existing.result.stack_size = Some(serde_json::Value::from(stack + added));
                    }
                    None => existing.listed_times += 1,
                }
            }
            None => out.push(Grouped { result: result.clone(), listed_times: 1 }),
        }
    }
    out
}

/// `TradeListing.vue`: an error box, or the matched row and the table.
pub fn view<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    if let Some(error) = &check.trade.error {
        return super::error_box(error, project);
    }
    col().key("trade-listing").w_full().min_h(0.0).child(matched(check, project)).child(table(check))
}

/// `Matched:`, the total, the online label, a spacer and the trade link.
fn matched<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let total = check.trade.list.as_ref().map(|list| {
        format!("{}{}", js::number_to_string(list.total), if list.inexact == Some(true) { "+" } else { "" })
    });
    let mut counts = row().key("trade-matched-counts").mr(theme::rem(0.5)).items_center();
    let label = i18n::fmt("trade_result.matched", &[""]);
    counts = counts.child(
        text(label.trim_end()).key("trade-matched-label").font(type_::SM).color(theme::gray(500)).pr(theme::rem(0.25)),
    );
    counts = match total {
        Some(total) => counts.child(text(total).key("trade-matched-total").font(type_::SM).color(theme::gray(500))),
        None => counts.child(text("...").key("trade-matched-total").font(type_::SM).color(theme::gray(600))),
    };
    if check.trade.list.is_some() {
        counts = counts.child(online::view(check, wraex_poe2::trade::Api::Trade, project));
    }
    row()
        .key("trade-matched")
        .w_full()
        .mb(theme::rem(0.5))
        .pl(theme::rem(0.5))
        .items_center()
        .child(counts)
        .child(el().grow(1.0))
        .children(check.trade.list.is_some().then(|| super::links::trade_links(check, project)))
}

/// The listings table: a heading row, the rows, then empty rows up to
/// [`SHOW_RESULTS`].
fn table(check: &PriceCheck) -> El {
    let item = check.item();
    let filters = check.filters();
    let is_gem = item.is_some_and(|item| item.category.as_ref().is_some_and(|c| GEM.contains(c)));
    let grants_skill = item.is_some_and(|item| item.category.as_ref().is_some_and(|c| GRANTS_REAL_SKILL.contains(c)));
    let uncut = item.is_some_and(|item| item.category == Some(wraex_poe2::parser::meta::ItemCategory::UncutGem));
    let stock = item.and_then(|item| item.stack_size.as_ref()).is_some();
    let ilvl = filters.item_level.is_some();
    let quality = filters.quality.as_ref().is_some_and(|quality| !quality.disabled) || is_gem;

    let grouped = grouped(&check.trade.rows);
    // A real `<table>` sizes its price column to its longest row, which matters once the
    // normalised price is in the label (`11111 exalted (84 div)`); the flex rows here need
    // one width for all of them, measured off the longest label.
    let price_width = grouped
        .iter()
        .map(|entry| price_label(&entry.result))
        .chain([i18n::t("trade_result.price")])
        .map(|label| label.chars().count() as f32 * 6.9 + 10.0)
        .fold(PRICE_WIDTH, f32::max);

    // Template order: price, stock, iLvl, gem level, gem sockets, quality, listed.
    let mut heading = row()
        .key("listing-head")
        .w_full()
        .h(theme::rem(1.375))
        .items_stretch()
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
        .border_layout()
        .child(heading_cell("listing-head-price", "trade_result.price", price_width).pl(theme::rem(0.5)));
    if stock {
        heading = heading.child(heading_cell("listing-head-stock", "trade_result.stock", LEVEL_WIDTH));
    }
    if ilvl {
        heading = heading.child(heading_cell("listing-head-ilvl", "trade_result.item_level", LEVEL_WIDTH));
    }
    if is_gem || uncut {
        heading = heading.child(heading_cell("listing-head-level", "trade_result.gem_level", GEM_WIDTH));
    }
    if is_gem || grants_skill {
        heading = heading.child(heading_cell("listing-head-sockets", "trade_result.gem_sockets", GEM_WIDTH));
    }
    if quality {
        heading = heading.child(heading_cell("listing-head-quality", "trade_result.quality", QUALITY_WIDTH));
    }
    heading = heading.child(heading_listed("listing-head-listed"));

    let mut body = col().key("listing-rows").w_full();
    for (index, entry) in grouped.iter().enumerate() {
        let mut line = row_for(index).child(price_cell(index, &entry.result, entry.listed_times, price_width));
        if stock {
            line = line.child(number_cell(index, &entry.result, Cell::Stock, LEVEL_WIDTH));
        }
        if ilvl {
            line = line.child(number_cell(index, &entry.result, Cell::ItemLevel, LEVEL_WIDTH));
        }
        if is_gem || uncut {
            line = line.child(number_cell(index, &entry.result, Cell::Level, GEM_WIDTH));
        }
        if is_gem || grants_skill {
            line = line.child(number_cell(index, &entry.result, Cell::Sockets, GEM_WIDTH));
        }
        if quality {
            line = line.child(number_cell(index, &entry.result, Cell::Quality, QUALITY_WIDTH));
        }
        body = body.child(line.child(listed_cell(index, &entry.result)));
    }
    for index in grouped.len()..SHOW_RESULTS {
        body = body.child(empty_row(index));
    }
    col().key("trade-table").w_full().min_h(0.0).child(heading).child(body)
}

fn heading_cell(key: &str, label: &str, width: f32) -> El {
    row()
        .key(key)
        .w(width)
        .h_full()
        .items_center()
        .px(theme::rem(0.5))
        .child(text(i18n::t(label)).key(format!("{key}-text")).font(type_::SM).color(theme::gray(200)).truncate())
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

/// A body row, zebra-striped on odd positions (`.table-stripped`).
pub(super) fn row_for(index: usize) -> El {
    let mut line = row().key(format!("listing-row-{index}")).w_full().h(ROW_HEIGHT).items_center();
    if index.is_multiple_of(2) {
        line = line.fill(theme::table_stripe());
    }
    line
}

/// A striped row with nothing in it, as the reference pads a short table.
pub(super) fn empty_row(index: usize) -> El {
    row_for(index).child(text("***").font(type_::SM).color(Color::TRANSPARENT).pl(theme::rem(0.5)))
}

/// The price cell: `priceAmount priceCurrency (normalizedPrice normalizedPriceCurrency)`.
///
/// The parenthesised part only shows when the row's price was converted, and not for a
/// price already in divines — `TradeItem.vue`'s condition, which the reference screenshot
/// of the Djinn Barya check spells out (`11111 exalted (84 div)`).
fn price_label(result: &PricingResult) -> String {
    let mut label = format!("{} {}", js::number_to_string(result.price_amount), result.price_currency);
    let converted = result
        .normalized_price_currency
        .as_ref()
        .zip(result.normalized_price.as_ref())
        .filter(|(currency, _)| result.price_currency != currency.id && result.price_currency != "divine");
    if let Some((currency, normalized)) = converted {
        label.push_str(&format!(" ({normalized} {})", currency.abbrev));
    }
    label
}

fn price_cell(index: usize, result: &PricingResult, listed_times: u32, width: f32) -> El {
    let label = price_label(result);
    let mut cell =
        row().w(width).pl(theme::rem(0.5)).items_center().child(
            text(label).key(format!("listing-{index}-price")).font(type_::SM).color(theme::gray(200)).truncate(),
        );
    if listed_times > 2 {
        // `rounded px-1 text-gray-800 bg-gray-400 -mr-2`, the `×` in the sans face.
        cell = cell.child(
            row()
                .mr(-(theme::rem(0.5)))
                .px(theme::rem(0.25))
                .items_center()
                .radius(theme::ROUNDED)
                .fill(theme::gray(400))
                .child(
                    rich(format!("× {listed_times}"), vec![Span::new(0, "×".len()).mono()])
                        .key(format!("listing-{index}-times"))
                        .font(type_::XS)
                        .color(theme::gray(800)),
                ),
        );
    } else if !result.has_note {
        cell = cell.child(ui::icon::question(theme::rem(0.875), theme::gray(400)).ml(theme::rem(0.25)));
    }
    cell
}

enum Cell {
    Stock,
    ItemLevel,
    Level,
    Sockets,
    Quality,
}

/// A JSON value as `{{ value }}` renders it.
fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Number(number) => number.as_f64().map_or_else(|| number.to_string(), js::number_to_string),
        other => other.to_string(),
    }
}

fn number_cell(index: usize, result: &PricingResult, cell: Cell, width: f32) -> El {
    let (key, value, blue) = match cell {
        Cell::Stock => (
            format!("listing-{index}-stock"),
            result.stack_size.as_ref().and_then(|size| size.as_f64()).map(js::number_to_string),
            false,
        ),
        Cell::ItemLevel => (format!("listing-{index}-ilvl"), Some(result.item_level.clone()), false),
        Cell::Level => (format!("listing-{index}-level"), result.level.as_ref().map(value_text), false),
        Cell::Sockets => {
            (format!("listing-{index}-sockets"), result.gem_sockets.map(|sockets| sockets.to_string()), false)
        }
        Cell::Quality => (format!("listing-{index}-quality"), result.quality.as_ref().map(value_text), true),
    };
    let mut out = row().w(width).px(theme::rem(0.5)).items_center().justify_end();
    if let Some(value) = value {
        let mut label = text(value).key(key).font(type_::SM);
        label = if blue { label.color(theme::blue(400)) } else { label.color(theme::gray(200)) };
        out = out.child(label);
    }
    out
}

fn listed_cell(index: usize, result: &PricingResult) -> El {
    let mut cell = row().grow(1.0).min_w(0.0).pl(theme::rem(1.0)).pr(theme::rem(0.5)).items_center();
    let mut stack = row().items_center().gap(theme::rem(0.25));
    stack = stack.child(account_status(index, result.is_instant_buyout, result.account_status));
    let date = result.relative_date.clone();
    stack = stack.child(
        rich(date.clone(), vec![Span::new(0, date.len()).mono()])
            .key(format!("listing-{index}-date"))
            .font(type_::XS)
            .color(theme::gray(200)),
    );
    cell = cell.child(stack);
    if result.is_mine {
        cell = cell.child(mine_badge(index));
    }
    cell
}

/// `account-status`: 6 pt, red when offline, orange when afk, pink when online, and
/// nothing at all for an instant buyout (`&.instantBuyout` has no fill upstream either).
pub(super) fn account_status(index: usize, instant_buyout: bool, status: AccountStatus) -> El {
    let color = if instant_buyout {
        Color::TRANSPARENT
    } else {
        match status {
            AccountStatus::Offline => theme::red(600),
            AccountStatus::Afk => theme::orange(500),
            AccountStatus::Online => theme::pink(400),
        }
    };
    el().key(format!("listing-{index}-status")).size(6.0, 6.0).radius(3.0).fill(color)
}

/// `rounded px-1 text-gray-800 bg-gray-400`: a `You` badge.
pub(super) fn mine_badge(index: usize) -> El {
    row()
        .ml(theme::rem(0.25))
        .px(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(400))
        .child(text(i18n::t("You")).key(format!("listing-{index}-mine")).font(type_::XS).color(theme::gray(800)))
}

/// Which currency a bulk table's selected side asks for in its trade link.
pub(super) fn have_for(selected: super::BulkCurrency) -> &'static str {
    selected.have()
}
