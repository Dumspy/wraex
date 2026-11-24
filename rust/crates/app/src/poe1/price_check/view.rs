//! `PriceCheckWindow.vue` and `CheckedItem.vue` in Rune: the window's frame and the top of
//! the item it shows.
//!
//! The trend, the filter block and the listings are placeholders (PLAN phase 5's next steps);
//! what is ported is the window itself, the title bar, the item header (`FilterName.vue`) and
//! the Search row.

use rune::{Cx, El, col, el, row, text};
use wraex_game::ParseError;
use wraex_poe1::filters::create_item_filters::category_to_trade_id;
use wraex_poe1::filters::{ItemFilters, SearchFilter};
use wraex_poe1::parser::item::Item;

use crate::poe1::i18n;
use crate::poe1::price_check::{PriceCheck, WIDTH};
use crate::theme::{self, text as type_};
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
        // `PriceCheckWindow.vue`'s `<div class="w-8" />`: room for the divine-orb rate, until
        // the price data is ported.
        .leading(el().key("titlebar-slot").w(theme::rem(2.0)).h_full())
        .on_close(move |s: &mut S| project(s).close())
        .build();
    col().key("price-check").w(WIDTH).h_full().fill(theme::gray(800)).child(title_bar).child(body(check, project))
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
                .padding(theme::rem(1.0))
                .min_h(0.0)
                .child(filter_name(check.item_filters(), project));
            if let Some(trend) = trend(item) {
                checked = checked.child(trend);
            }
            checked = checked.child(filters_placeholder(check));
            checked = checked.child(if check.do_search { listings_placeholder() } else { search_row(project) });
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

/// `PriceTrend.vue`'s fallback row: the item's icon, an arrow and the price — `?` until the
/// poe.ninja prices are ported. Items the reference draws no trend row for get none here either.
fn trend(item: &Item) -> Option<El> {
    let info = item.info();
    if info.craftable.is_some() || info.ref_name == "Mercenary Warrant" || info.ref_name == "Viscera" {
        return None;
    }
    // `min-height: 3rem` with `pb-4`, centred: `ItemQuickPrice` with no price to show.
    Some(
        row().key("trend").w_full().min_h(theme::rem(3.0)).pb(theme::rem(1.0)).items_center().child(
            row()
                .grow(1.0)
                .justify_center()
                .items_center()
                .gap(theme::rem(0.25))
                .child(icon_placeholder("trend-item-icon"))
                .child(icon::arrow_right(theme::rem(0.875), theme::rem(0.875), theme::gray(600)).mx(theme::rem(0.25)))
                .child(text("?").key("trend-price").font(type_::BASE).px(theme::rem(0.25)))
                .child(icon_placeholder("trend-currency-icon")),
        ),
    )
}

/// The item's and the currency's 32×32 images (`ItemQuickPrice.vue`): the game's CDN and
/// upstream's `public/images` are not wired up yet, so a plain square stands in.
fn icon_placeholder(key: &str) -> El {
    el().key(key).size(theme::rem(2.0), theme::rem(2.0)).radius(theme::ROUNDED).fill(theme::gray(700))
}

/// `FiltersBlock.vue`: not ported yet.
fn filters_placeholder(check: &PriceCheck) -> El {
    let note = match &check.presets {
        Some(Err(error)) => format!("Filters unavailable: {error}"),
        _ => "Filters".to_owned(),
    };
    // A stand-in height, picked so the Search row below it lands where upstream's does for a
    // fully packed rare (`adv-rare-ring-mirrored-catalyst`); the real block sizes to its rows.
    placeholder("filters-placeholder", &note, theme::rem(15.0))
}

/// `TradeBulk.vue` / `TradeListing.vue`: not ported yet.
fn listings_placeholder() -> El {
    placeholder("listings-placeholder", "Listings", theme::rem(6.0))
}

/// A dashed box where a part of the window isn't ported yet.
fn placeholder(key: &str, label: &str, height: f32) -> El {
    col()
        .key(key)
        .w_full()
        .h(height)
        .border(1.0, theme::gray(700))
        .border_dash(4.0, 4.0)
        .border_layout()
        .radius(theme::ROUNDED)
        .mb(theme::rem(0.5))
        .center()
        .child(text(label).key(format!("{key}-text")).font(type_::SM).color(theme::gray(600)))
}

/// `CheckedItem.vue`'s `!doSearch` row: the Search button, and the trade-site link.
fn search_row<S: 'static>(project: Project<S>) -> El {
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
        // `TradeLinks.vue`: upstream opens the trade site with this. Building the link needs the
        // trade request (`createTradeRequest`), so the button is drawn but inert for now.
        .child(
            ui::button("trade-link", "Trade")
                .items_center()
                .gap(theme::rem(0.25))
                .child(icon::external_link(theme::rem(0.75), theme::gray(400))),
        )
}
