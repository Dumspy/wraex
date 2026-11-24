//! `TradeListing.vue`: the search's totals, the online/merchant label, the trade link
//! and the listings table.

use rune::{Color, El, Span, col, el, rich, row, text};
use wraex_core::js;
use wraex_poe1::parser::meta::ItemCategory;
use wraex_poe1::results::{AccountStatus, PricingResult};

use crate::poe1::i18n;
use crate::poe1::price_check::PriceCheck;
use crate::poe1::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui::art;

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

/// A row of the table: a `PricingResult` and how many listings it stands for.
#[derive(Debug, Clone)]
pub struct Grouped {
    pub result: PricingResult,
    pub listed_times: u32,
}

/// `groupedResults`: listings by the same seller at the same price (or one of the last
/// two rows) fold into one, adding up stacks when they have them.
pub fn grouped(rows: &[PricingResult], collapse_merchant: bool) -> Vec<Grouped> {
    let mut out: Vec<Grouped> = Vec::new();
    for result in rows {
        if out.is_empty() || (result.has_fee && !collapse_merchant) {
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
                match existing.result.stack_size.filter(|stack| *stack != 0.0) {
                    Some(stack) => existing.result.stack_size = Some(stack + result.stack_size.unwrap_or(0.0)),
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
    let mut root = col().key("trade-listing").w_full().min_h(0.0);
    if check.item().is_some_and(|item| item.info().exchangeable == Some(true)) {
        root = root.child(legacy_message());
    }
    root.child(matched(check, project)).child(table(check))
}

/// `:legacy_bulk_xchg_msg`: the item also trades on the Currency Exchange.
pub(super) fn legacy_message() -> El {
    row()
        .key("legacy-message")
        .w_full()
        .mb(theme::rem(0.75))
        .px(theme::rem(0.5))
        .py(theme::rem(0.5))
        .justify_center()
        .radius(theme::ROUNDED)
        .border(1.0, theme::gray(600))
        .border_layout()
        .fill(theme::gray(700))
        .child(text(i18n::t("trade_result.legacy_bulk_xchg_msg")).key("legacy-message-text").font(type_::SM))
}

/// `Matched:`, the total, the online label and the trade link.
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
    row()
        .key("trade-matched")
        .w_full()
        .pl(theme::rem(0.5))
        .mb(theme::rem(0.5))
        .items_center()
        .child(counts)
        .child(online::view(check, wraex_poe1::trade::Api::Trade, project))
        .child(el().grow(1.0))
        .child(super::links::trade_links(check, project))
}

/// The listings table: a heading row, the rows, then empty rows up to
/// [`SHOW_RESULTS`].
fn table(check: &PriceCheck) -> El {
    let item = check.item();
    let filters = check.item_filters();
    let stock = item.and_then(|item| item.stack_size.as_ref()).is_some();
    let ilvl = filters.and_then(|filters| filters.item_level.as_ref()).is_some();
    let gem = item.is_some_and(|item| item.category == Some(ItemCategory::Gem));
    let quality = filters.and_then(|filters| filters.quality.as_ref()).is_some() || gem;

    // Template order: price, stock, iLvl, gem level, quality, listed.
    let mut heading = row()
        .key("listing-head")
        .w_full()
        .h(theme::rem(1.375))
        .items_stretch()
        .border_sides([0.0, 0.0, 1.0, 0.0], theme::gray(700))
        .border_layout()
        .child(heading_cell("listing-head-price", "trade_result.price", PRICE_WIDTH));
    if stock {
        heading = heading.child(heading_cell("listing-head-stock", "trade_result.stock", LEVEL_WIDTH));
    }
    if ilvl {
        heading = heading.child(heading_cell("listing-head-ilvl", "trade_result.item_level", LEVEL_WIDTH));
    }
    if gem {
        heading = heading.child(heading_cell("listing-head-level", "trade_result.gem_level", GEM_WIDTH));
    }
    if quality {
        heading = heading.child(heading_cell("listing-head-quality", "trade_result.quality", QUALITY_WIDTH));
    }
    heading = heading.child(heading_listed("listing-head-listed"));

    let grouped = grouped(&check.trade.rows, check.trade.collapse_merchant);
    let mut body = col().key("listing-rows").w_full();
    for (index, entry) in grouped.iter().enumerate() {
        let mut line = row_for(index).child(price_cell(index, &entry.result, entry.listed_times));
        if stock {
            line = line.child(number_cell(index, &entry.result, Cell::Stock, LEVEL_WIDTH));
        }
        if ilvl {
            line = line.child(number_cell(index, &entry.result, Cell::ItemLevel, LEVEL_WIDTH));
        }
        if gem {
            line = line.child(number_cell(index, &entry.result, Cell::Level, GEM_WIDTH));
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

fn price_cell(index: usize, result: &PricingResult, listed_times: u32) -> El {
    let label = format!("{} {}", js::number_to_string(result.price_amount), result.price_currency);
    let mut price =
        text(label.clone()).key(format!("listing-{index}-price")).font(type_::SM).color(theme::gray(200)).truncate();
    if result.price_currency == "exalted" {
        price = price.strike(0, label.len(), None);
    }
    let mut cell = row().w(PRICE_WIDTH).pl(theme::rem(0.5)).items_center().child(price);
    if listed_times > 2 {
        // `rounded px-1 text-gray-800 bg-gray-400 ml-1 -mr-2`, the `×` in the sans face.
        cell = cell.child(
            row()
                .ml(theme::rem(0.25))
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
    } else if !result.has_fee {
        // `stashListing`: the stash icon, and a `?` when the listing has no note.
        // `$style.stashIcon`: `height: 1.25rem`, greyed out (`grayscale(1) opacity(.5)`; Rune
        // has no colour filter, so only the opacity is applied).
        let mut stash =
            row().key(format!("listing-{index}-stash")).items_center().ml(theme::rem(0.25)).mr(-(theme::rem(0.25)));
        stash = stash.child(
            art::local(wraex_core::GameId::Poe1, "stash.png", theme::rem(1.25))
                .opacity(0.5)
                .key(format!("listing-{index}-stash-icon")),
        );
        if !result.has_note {
            stash = stash.child(text("?").font(type_::XS).color(theme::gray(500)));
        }
        cell = cell.child(stash);
    }
    cell
}

enum Cell {
    Stock,
    ItemLevel,
    Level,
    Quality,
}

fn number_cell(index: usize, result: &PricingResult, cell: Cell, width: f32) -> El {
    let (key, value, blue) = match cell {
        Cell::Stock => (format!("listing-{index}-stock"), result.stack_size.map(js::number_to_string), false),
        Cell::ItemLevel => (format!("listing-{index}-ilvl"), Some(result.item_level.clone()), false),
        Cell::Level => (format!("listing-{index}-level"), result.level.clone(), false),
        Cell::Quality => (format!("listing-{index}-quality"), result.quality.clone(), true),
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
    stack = stack.child(account_status(index, result.account_status));
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

/// `.accountStatus`: a 6 pt dot, red when offline and orange when afk.
pub(super) fn account_status(index: usize, status: AccountStatus) -> El {
    let color = match status {
        AccountStatus::Offline => theme::red(600),
        AccountStatus::Afk => theme::orange(500),
        AccountStatus::Online => Color::TRANSPARENT,
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
    match selected {
        super::BulkCurrency::Chaos => "chaos",
        super::BulkCurrency::Stable => "divine",
    }
}
