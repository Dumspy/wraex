//! `PriceCheckWindow.vue` and `CheckedItem.vue` in Rune: the window's frame and the item it
//! shows.
//!
//! Ported: the window itself, the title bar with its `⇄ rate` popover, the item header
//! (`FilterName.vue`), the trend row (`PriceTrend.vue`, `ItemQuickPrice.vue`), the filters
//! block (`FiltersBlock.vue`), the Search row and the listings (`TradeListing.vue`,
//! `TradeBulk.vue`, `OnlineFilter.vue`, `TradeLinks.vue`), and the expected-value boxes
//! (`StackValue.vue`, `DustValue.vue`).

use rune::{Align, Color, Cx, El, Shape, Side, canvas, col, el, rich, row, text};
use wraex_core::js::number_to_string;
use wraex_game::ParseError;
use wraex_poe1::filters::create_item_filters::category_to_trade_id;
use wraex_poe1::filters::{ItemFilters, SearchFilter};
use wraex_poe1::parser::item::Item;
use wraex_poe1::prices::{CurrencyValue, display_rounding};
use wraex_poe1::trends::{Forecast, TrendChange, delta_from_graph, is_valuable_basetype};

use crate::poe1::i18n;
use crate::poe1::price_check::trade;
use crate::poe1::price_check::{PriceCheck, WIDTH};
use crate::theme::{self, text as type_};
use crate::ui::art::{self, Picture};
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
    let title_bar = ui::title_bar(check.title())
        // `PriceCheckWindow.vue`'s title-bar slot: the `⇄ 891` exchange-rate button,
        // which opens the divine-orb table, or an empty `w-8` spacer.
        .leading(title_leading(check, project))
        .on_close(move |s: &mut S| project(s).close())
        .build();
    let mut root =
        col().key("price-check").w(WIDTH).h_full().fill(theme::gray(800)).child(title_bar).child(body(check, project));
    if check.exchange_open {
        root = root.child(exchange_popover(check));
    }
    root
}

/// `PriceCheckWindow.vue`'s `<ui-popover v-if="stableOrbCost">`: the chaos-per-divine
/// button, with the `0.1 … 0.9 div` table in its popover.
fn title_leading<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    let Some(rate) = check.stable_orb_cost() else {
        // `v-else-if="xchgRateLoading()"` spins a glyph; the spacer stands in for both.
        return el().key("titlebar-slot").w(theme::rem(2.0)).h_full();
    };
    let label = number_to_string(rate);
    row()
        .key("titlebar-exchange")
        .h_full()
        .px(theme::rem(0.5))
        .pt(1.0)
        .items_center()
        .gap(theme::rem(0.25))
        .focusable(true)
        .role("button")
        .label(format!("\u{21c4} {label}"))
        .on_press(move |s: &mut S| project(s).toggle_exchange())
        .child(icon::exchange(theme::rem(0.875), theme::gray(600)))
        .child(text(label).key("titlebar-exchange-rate").font(type_::SM).color(theme::gray(600)))
}

/// The exchange-rate popover: `ItemQuickPrice` for one divine, then `0.1 … 0.9 div ⇒ N c`.
fn exchange_popover(check: &PriceCheck) -> El {
    let Some(rate) = check.stable_orb_cost() else { return el() };
    let mut content = col().key("exchange-popover-content");
    content = content.child(
        ui::QuickPrice::new("exchange", Some(CurrencyValue { min: rate, max: rate, currency: "chaos" }))
            .color(theme::gray(900))
            // `PriceCheckWindow.vue`'s `<item-quick-price item-img="/images/divine.png">`.
            .item_img(Picture::local("divine.png"))
            .build(),
    );
    for tenth in 1..=9 {
        let divine = tenth as f64 / 10.0;
        // `Math.round(stableOrbCost * i / 10)`
        let chaos = (rate * divine + 0.5).floor();
        content = content.child(
            row().key(format!("exchange-row-{tenth}")).pl(theme::rem(0.25)).child(
                text(format!("{} div \u{21d2} {} c", number_to_string(divine), number_to_string(chaos)))
                    .key(format!("exchange-{tenth}"))
                    .font(type_::BASE)
                    .color(theme::gray(900)),
            ),
        );
    }
    // tippy's light theme: a white frame with four points of padding.
    row()
        .key("exchange-popover")
        .padding(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(Color::WHITE)
        .child(content)
        .anchored("titlebar-exchange", Side::Bottom, Align::Start, 4.0)
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
                .child(ui::error_box(&name, &help).mx(theme::rem(1.0)).mt(theme::rem(1.0)))
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
                .child(filter_name(check.item_filters(), project));
            if let Some(trend) = trend(check, project) {
                checked = checked.child(trend);
            }
            checked = checked.child(crate::poe1::price_check::filters::view(check, project));
            checked =
                checked.child(if check.do_search { listings(check, project) } else { search_row(check, project) });
            if let Some(stack) = stack_value(check) {
                checked = checked.child(stack);
            }
            if let Some(dust) = dust_value(item) {
                checked = checked.child(dust);
            }
            body = body.child(checked);
        }
    }
    body
}

/// `FilterName.vue`: the item name (or its category) on a dark bar, with the corruption state
/// on the right and the two toggles both of them carry.
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
        let (label, color) = if !corrupted.value {
            (i18n::t("item.not_corrupted"), theme::gray(600))
        } else if corrupted.twice {
            (i18n::t("item.twice_corrupted"), theme::red(500))
        } else {
            (i18n::t("item.corrupted"), theme::red(500))
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

/// `FilterName.vue`'s `showAsActive`: the exact search is in use, and the name's border says so.
fn show_as_active(filters: &ItemFilters) -> bool {
    let active = active_search(filters);
    if active.sub.as_ref().is_some_and(|sub| sub.disabled != Some(true)) {
        return false;
    }
    filters.search_relaxed.as_ref().and_then(|relaxed| relaxed.disabled).unwrap_or(false)
}

/// The search `FilterName.vue` reads: the relaxed one unless it's disabled, else the exact one.
fn active_search(filters: &ItemFilters) -> &SearchFilter {
    match &filters.search_relaxed {
        Some(relaxed) if relaxed.disabled != Some(true) => relaxed,
        _ => &filters.search_exact,
    }
}

/// `PriceTrend.vue`: the item's price from poe.ninja with the 7-day sparkline, or the
/// fallback row that just names the item. `None` where the reference draws no row.
fn trend<S: 'static>(check: &PriceCheck, _project: Project<S>) -> Option<El> {
    let item = check.item()?;
    let info = item.info();
    // `fraction`: `filters.stackSize != null`
    let fraction = check.item_filters().is_some_and(|filters| filters.stack_size.is_some());

    let Some(price) = check.price_info() else {
        // `showFallback`: a craftable item, a Mercenary Warrant and Viscera get no row.
        let fallback = info.craftable.is_none() && info.ref_name != "Mercenary Warrant" && info.ref_name != "Viscera";
        if !fallback {
            return None;
        }
        let base = item.unique_base.as_ref().unwrap_or(info);
        return Some(
            row().key("trend").w_full().min_h(theme::rem(3.0)).pb(theme::rem(1.0)).items_center().child(
                ui::QuickPrice::new("trend", None)
                    .currency_text()
                    .item_size(base.w, base.h)
                    .item_img(Picture::cdn(info.icon.clone()))
                    .build()
                    .grow(1.0)
                    .justify_center(),
            ),
        );
    };

    // `item.info.refName === 'Divine Orb'` keeps its chaos price; anything else switches to
    // divines once it is worth one (`autoCurrency`).
    let currency = if info.ref_name == "Divine Orb" {
        CurrencyValue { min: price.info.chaos, max: price.info.chaos, currency: "chaos" }
    } else {
        check.prices.prices().auto_currency(price.info.chaos)
    };
    let mut quick =
        ui::QuickPrice::new("trend", Some(currency)).fraction(fraction).item_img(Picture::cdn(info.icon.clone()));
    quick = if is_valuable_basetype(item) {
        // `<template #item><span class="text-gray-400">{{ t(':base_item') }}</span></template>`
        quick.slot(
            text(i18n::t("trade_result.base_item")).key("trend-base-item").font(type_::BASE).color(theme::gray(400)),
        )
    } else {
        // `:item-base="item.uniqueBase ?? item.info"`
        let base = item.unique_base.as_ref().unwrap_or(info);
        quick.item_size(base.w, base.h)
    };

    let mut line = row()
        .key("trend")
        .w_full()
        .min_h(theme::rem(3.0))
        .pb(theme::rem(1.0))
        .items_center()
        .child(quick.build().grow(1.0).justify_center());
    // The trend button keeps its natural width beside the centred price row.
    if let Some(change) = delta_from_graph(&price.info.graph) {
        line = line.child(trend_change::<S>(&change, &price.url));
    }
    Some(line)
}

/// `PriceTrend.vue`'s `trend-btn`: the forecast, the label and the sparkline. It opens
/// the item's poe.ninja page.
fn trend_change<S: 'static>(change: &TrendChange, url: &str) -> El {
    let unavailable = url.to_owned();
    let mut button = row()
        .key("trend-change-button")
        .items_center()
        .gap(theme::rem(0.5))
        .my(-(theme::rem(0.125)))
        .py(theme::rem(0.125))
        .px(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .hover(theme::gray(700))
        .focusable(true)
        .role("button")
        .label(format!("{} {}", change.text, i18n::t("trade_result.graph_7d")));
    if !unavailable.is_empty() {
        // `openNinja`: the item's poe.ninja page.
        button = button.on_press_tx(move |_s: &mut S, tx: &mut rune::Tx| {
            let _ = tx.open_url(&unavailable);
        });
    }

    let mut text_line = row().key("trend-change-line").items_center();
    match change.forecast {
        // `fa-angle-double-down pr-1 text-red-600`, `fa-angle-double-up pr-1 text-green-500`
        Forecast::Down => {
            text_line = text_line.child(icon::chevrons(theme::rem(0.875), false, theme::red(600)).mr(theme::rem(0.25)));
        }
        Forecast::Up => {
            text_line =
                text_line.child(icon::chevrons(theme::rem(0.875), true, theme::green(500)).mr(theme::rem(0.25)));
        }
        // `<span class="pr-1 text-gray-600 font-sans leading-none">±</span>`
        Forecast::Const => {
            text_line = text_line.child(
                rune::rich("\u{b1}", vec![rune::Span::new(0, 2).mono().color(theme::gray(600))])
                    .key("trend-change-sign")
                    .font(theme::leading_none(type_::SM))
                    .mr(theme::rem(0.25)),
            );
        }
    }
    text_line = text_line.child(text(change.text.clone()).key("trend-change").font(type_::BASE));

    let column = col().key("trend-change-text").items_center().child(text_line).child(
        text(i18n::t("trade_result.graph_7d"))
            .key("trend-change-label")
            .font(theme::leading_none(type_::XS))
            .color(theme::gray(500)),
    );

    button.child(column).child(sparkline(change))
}

/// The 7-day graph: an area under a thin line, in the `w-12 h-8` box ApexCharts draws it
/// in. The plot is the box's lower 16 points, as the reference's chart is.
fn sparkline(change: &TrendChange) -> El {
    const W: f32 = 48.0;
    const H: f32 = 16.0;
    let span = (change.draw_max - change.draw_min).max(f64::MIN_POSITIVE);
    let last = change.points.len().saturating_sub(1).max(1) as f32;
    let points: Vec<[f32; 2]> = change
        .points
        .iter()
        .enumerate()
        .map(|(index, value)| [index as f32 * W / last, ((change.draw_max - value) / span * f64::from(H)) as f32])
        .collect();
    let mut area = points.clone();
    area.push([W, H]);
    area.push([0.0, H]);
    let shape = Shape::new().fill(area, theme::gray(700).alpha(0.85)).stroke(points, 1.0, theme::gray(500));
    col()
        .key("trend-sparkline")
        .w(theme::rem(3.0))
        .h(theme::rem(2.0))
        .justify_end()
        .child(canvas(shape).key("trend-sparkline-chart").size(W, H))
}

/// `TradeListing.vue` / `TradeBulk.vue`, and `OnlineFilter.vue`'s panel while open.
fn listings<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    if check.item().is_none() {
        return el().key("trade-area");
    }
    let api = check.trade.api(check);
    let mut area = col().key("trade-area").w_full().min_h(0.0).child(trade::view(check, project));
    if check.online_open {
        area = area.child(trade::online::panel(check, api, project));
    }
    area
}

/// `CheckedItem.vue`'s `!doSearch` row: the Search button, and the trade-site link.
fn search_row<S: 'static>(check: &PriceCheck, project: Project<S>) -> El {
    row()
        .key("search-row")
        .w_full()
        .items_center()
        .justify_between()
        // `<div class="flex w-40">` around the button, which is at least `min-width: 5rem`.
        .child(
            row().key("search-group").w(theme::rem(10.0)).child(
                ui::button("search", "Search")
                    .min_w(theme::rem(5.0))
                    .focusable(true)
                    .on_press(move |s: &mut S| project(s).start_search()),
            ),
        )
        // `TradeLinks.vue`: the query URL until a search has an id.
        .child(trade::links::trade_links(check, project))
}

/// `StackValue.vue`: what the stack on screen is worth, and what one full stack is.
///
/// `v-if="show"`: a stack-size filter, a poe.ninja entry and a price for it.
fn stack_value(check: &PriceCheck) -> Option<El> {
    let filters = check.item_filters()?;
    let stack = filters.stack_size.as_ref()?;
    let item = check.item()?;
    let one = check.price_info()?;

    let price_for = |count: f64| {
        let chaos = count * one.info.chaos;
        // A divine's own stack stays in chaos; everything else switches once it is worth one.
        let price = if item.info().ref_name == "Divine Orb" {
            CurrencyValue { min: chaos, max: chaos, currency: "chaos" }
        } else {
            check.prices.prices().auto_currency(chaos)
        };
        format!("{} {}", display_rounding(price.min, false), price.currency)
    };

    // `<div class="flex text-gray-400 leading-none">`: a block in CSS, so each `w-1/2` half is
    // half of the box. Rune's row is content-sized without `w_full`, which would shrink both
    // halves to their text and pull the two readings together.
    let half = |key: &str| row().key(key.to_owned()).w_frac(0.5).min_w(0.0).items_center();
    // `<i class="fas fa-arrow-right text-gray-600 px-1 text-xs"></i>`, with a space on each
    // side of it in the template: 4 pt around a 12 pt glyph, plus the text's own space. A
    // margin, because `.px` would sit inside the icon's fixed 12 pt box.
    let arrow = || icon::arrow_right(theme::rem(0.75), theme::rem(0.75), theme::gray(600)).mx(theme::rem(0.5));
    let left = half("stack-value-have")
        .child(muted(i18n::t("trade_result.you_have")))
        .child(muted_times())
        .child(muted(format!(" {}", number_to_string(stack.value))).key("stack-value-have-amount"))
        .child(arrow())
        .child(muted(price_for(stack.value)).key("stack-value-have-price"));
    let mut value = row().key("stack-value-row").w_full().child(left);
    if let Some(max) = item.stack_size.as_ref().and_then(|stack| stack.max) {
        value = value.child(
            half("stack-value-stack")
                .pl(theme::rem(0.5))
                .child(muted(i18n::t("trade_result.stack")))
                .child(muted_times())
                .child(muted(format!(" {}", number_to_string(max))).key("stack-value-stack-amount"))
                .child(arrow())
                .child(muted(price_for(max)).key("stack-value-stack-price")),
        );
    }

    Some(dashed_box("stack-value").child(value))
}

/// `p-2 border-dashed border border-gray-600 rounded mt-2 text-gray-400 leading-none`.
fn dashed_box(key: &str) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .mt(theme::rem(0.5))
        .padding(theme::rem(0.5))
        .radius(theme::ROUNDED)
        .border(1.0, theme::gray(600))
        .border_dash(4.0, 4.0)
        .border_layout()
}

/// A `text-gray-400 leading-none` run inside an expected-value box.
fn muted(label: impl Into<String>) -> El {
    text(label).font(theme::leading_none(type_::SM)).color(theme::gray(400))
}

/// `<span class="font-sans">×</span>`, with the space the template puts before it.
fn muted_times() -> El {
    rich(" \u{d7}", vec![rune::Span::new(0, 3).mono().color(theme::gray(400))]).font(theme::leading_none(type_::SM))
}

/// `DustValue.vue`: `Disenchanting → 36,225 × dust` for an item with a dust value.
fn dust_value(item: &Item) -> Option<El> {
    let dust = item.dust_equivalent?;
    Some(
        dashed_box("dust-value")
            .items_center()
            .child(muted(i18n::t("item.disenchanting")))
            .child(icon::arrow_right(theme::rem(0.75), theme::rem(0.75), theme::gray(600)).px(theme::rem(0.25)))
            .child(muted(grouped(dust)).key("dust-value-amount"))
            .child(muted_times())
            // `<img class="w-5 -my-5 inline-block" src="/images/dust.png">`: the dust icon,
            // pulled out of the line.
            .child(
                art::local(wraex_core::GameId::Poe1, "dust.png", theme::rem(1.25))
                    .my(-(theme::rem(1.25)))
                    .key("dust-value-icon"),
            ),
    )
}

/// `Number#toLocaleString()`'s grouping for a whole number ("36,225").
fn grouped(value: f64) -> String {
    let negative = value < 0.0;
    let text = number_to_string(value.abs().trunc());
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut out = String::with_capacity(whole.len() + whole.len() / 3);
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if negative {
        out.insert(0, '-');
    }
    if !fraction.is_empty() {
        out.push('.');
        out.push_str(fraction);
    }
    out
}
