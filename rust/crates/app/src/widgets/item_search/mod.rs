//! `ref/apt/renderer/src/web/item-search/WidgetItemSearch.vue` and EE2's copy: the name
//! search that prices a base type nobody has to hover.
//!
//! Type a name and the widget lists the matches — APT's heist target's skill gems and
//! replica uniques, EE2's skill gems, uniques and base items — one button each. Picking a
//! match puts it in the strip at the top with its poe.ninja price; pressing a strip entry
//! dispatches `MAIN->CLIENT::item-text` with a `createVirtualItem` of it, which reopens the
//! price check on it without any clipboard text. The matching itself is
//! [`wraex_poe1::item_search`]/[`wraex_poe2::item_search`]; this module is the widget.
//!
//! What the port does not draw: the 4 s `UiTimeout` that makes the card invisible to the
//! overlay's clicks again (`wmFlags` is the shell's), the strip's enter animation, the row
//! hover's gradient (a flat fill stands in for it) and the magnifier glyph of the "too many"
//! message (not among the ported icons).
//!
//! The picked item's dispatch is recorded as [`crate::widgets::Action::PriceCheckItem`] and
//! applied by [`crate::App::poll`], the shell's half of upstream's `Host.selfDispatch`.

use rune::{El, TextEdit, col, row, text, text_input};
use wraex_core::GameId;
use wraex_poe1::item_search as poe1_search;
use wraex_poe1::parser::item::Item as Poe1Item;
use wraex_poe1::prices::{CurrencyValue as Poe1Price, DbQuery as Poe1Query};
use wraex_poe2::item_search as poe2_search;
use wraex_poe2::parser::item::Item as Poe2Item;
use wraex_poe2::prices::{CurrencyValue as Poe2Price, DbQuery as Poe2Query};

use crate::poe1::price_check::{self as poe1_check, prices::PriceList};
use crate::theme::{self, text as type_};
use crate::trade::Mode;
use crate::ui::{art, icon};
use crate::widgets::{Action, Overlay, Project, Widget};

/// `WidgetItemSearch.vue`'s `widget.type`.
pub const WM_TYPE: &str = "item-search";

/// `useSelectedItems`'s limit: a sixth pick replaces the strip.
pub const MAX_SELECTED: usize = 5;

/// `typeFilter`'s tabs. APT has the gem and replica ones, EE2 the gem, unique and base-item
/// ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Gem,
    Replica,
    Unique,
    BaseItem,
}

impl Tab {
    /// `typeFilter.value`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Tab::Gem => "gem",
            Tab::Replica => "replica",
            Tab::Unique => "unique",
            Tab::BaseItem => "base_item",
        }
    }

    /// The tab's label key: `t(':target_gem')` and friends, which `useI18nNs('item_search')`
    /// prefixes with `item_search.`.
    pub const fn label_key(self) -> &'static str {
        match self {
            Tab::Gem => "item_search.target_gem",
            Tab::Replica => "item_search.target_replica",
            Tab::Unique => "item_search.target_unique",
            Tab::BaseItem => "item_search.target_base_item",
        }
    }
}

/// The tabs a game's widget draws, in the template's order.
pub fn tabs(game: GameId) -> &'static [Tab] {
    match game {
        GameId::Poe2 => &[Tab::Gem, Tab::Unique, Tab::BaseItem],
        GameId::Poe1 => &[Tab::Gem, Tab::Replica],
    }
}

/// `typeFilter`'s initial value: APT's `'gem'`, EE2's `'base_item'`.
pub fn default_tab(game: GameId) -> Tab {
    match game {
        GameId::Poe2 => Tab::BaseItem,
        GameId::Poe1 => Tab::Gem,
    }
}

/// A base type a search matched. The two games' `BaseType` differ (EE2's `name` and
/// `refName` are optional), so a result carries whichever record it came from.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    Poe1(wraex_poe1::data::BaseType),
    Poe2(wraex_poe2::data::BaseType),
}

impl Record {
    /// `info.name`; empty for the nameless uniques EE2 ships (upstream renders `undefined`).
    pub fn name(&self) -> &str {
        match self {
            Record::Poe1(info) => &info.name,
            Record::Poe2(info) => info.name.as_deref().unwrap_or_default(),
        }
    }

    /// `info.refName`: what a price query searches by.
    pub fn ref_name(&self) -> &str {
        match self {
            Record::Poe1(info) => &info.ref_name,
            Record::Poe2(info) => info.ref_name.as_deref().unwrap_or_default(),
        }
    }

    /// `info.icon`: the CDN URL.
    pub fn icon(&self) -> &str {
        match self {
            Record::Poe1(info) => &info.icon,
            Record::Poe2(info) => &info.icon,
        }
    }

    /// `info.namespace`, as a price query's `ns`.
    pub fn namespace(&self) -> &'static str {
        match self {
            Record::Poe1(info) => info.namespace.as_str(),
            Record::Poe2(info) => info.namespace.as_str(),
        }
    }

    /// `info.unique.base`: a price query's variant.
    pub fn unique_base(&self) -> Option<&str> {
        match self {
            Record::Poe1(info) => info.unique.as_ref().map(|unique| unique.base.as_str()),
            Record::Poe2(info) => info.unique.as_ref().map(|unique| unique.base.as_str()),
        }
    }

    pub fn is_gem(&self) -> bool {
        self.namespace() == "GEM"
    }

    pub fn is_unique(&self) -> bool {
        self.namespace() == "UNIQUE"
    }

    /// `createVirtualItem`'s props for this record: the gem/unique/base-item constructor.
    fn virtual_item(&self) -> Picked {
        match self {
            Record::Poe1(info) if self.is_gem() => Picked::Poe1(Box::new(Poe1Item::virtual_gem(info.clone()))),
            Record::Poe1(info) => Picked::Poe1(Box::new(Poe1Item::virtual_unique(info.clone()))),
            Record::Poe2(info) if self.is_gem() => Picked::Poe2(Box::new(Poe2Item::virtual_gem(info.clone()))),
            Record::Poe2(info) if self.is_unique() => Picked::Poe2(Box::new(Poe2Item::virtual_unique(info.clone()))),
            Record::Poe2(info) => Picked::Poe2(Box::new(Poe2Item::virtual_normal(info.clone()))),
        }
    }
}

/// `findItems`'s answer: `false` is [`Results::TooMany`] (the "too many" message), an empty
/// list the "not found" one.
#[derive(Debug, Clone, PartialEq)]
pub enum Results {
    TooMany,
    Found(Vec<Record>),
}

impl Results {
    /// The matches, when the query found any.
    pub fn items(&self) -> &[Record] {
        match self {
            Results::TooMany => &[],
            Results::Found(items) => items,
        }
    }
}

/// A price in the game's own currency names: what a strip entry shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Price {
    pub min: f64,
    pub max: f64,
    /// `chaos`, `div`, or EE2's `exalted`.
    pub currency: &'static str,
}

/// One entry of the strip at the top: `useSelectedItems`'s `SelectedItem`.
#[derive(Debug, Clone, PartialEq)]
pub struct Selected {
    pub record: Record,
    /// `discr`: the alternate quality an OCR pick adds (`Anomalous`, ...), which the ported
    /// searches never produce.
    pub discr: Option<String>,
    /// `chaos`/`primaryValue`: the sort key, and what `autoCurrency` priced.
    pub value: Option<f64>,
    pub price: Option<Price>,
}

/// The item a strip press dispatches: the two games parse items into different types.
#[derive(Debug, Clone, PartialEq)]
pub enum Picked {
    Poe1(Box<Poe1Item>),
    Poe2(Box<Poe2Item>),
}

/// `usePoeninja` for this widget: the prices a picked base type shows.
enum Prices {
    Poe1(PriceList),
    Poe2(wraex_poe2::prices::Prices),
}

impl std::fmt::Debug for Prices {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Prices::Poe1(prices) => f.debug_tuple("Poe1").field(prices).finish(),
            Prices::Poe2(prices) => f.debug_tuple("Poe2").field(&prices.xchg_rate).finish(),
        }
    }
}

/// `WidgetItemSearch.vue`'s state.
pub struct State {
    pub game: GameId,
    /// `searchValue`, the search field's `v-model`.
    pub search: TextEdit,
    /// `searchValue` as the results read it. Rune writes a text field's model back from the
    /// element when the field loses focus, and that write can land empty, so the widget keeps
    /// the query of its own and re-seeds the field from it.
    pub query: String,
    /// `typeFilter`.
    pub tab: Tab,
    /// `results`: recomputed when the query or the tab changes, as the `computed` does.
    pub results: Results,
    /// `selected` (`useSelectedItems`): at most [`MAX_SELECTED`], most valuable first.
    pub selected: Vec<Selected>,
    /// `wm.active`: the search half of the card is shown. A scene's widget is active; the
    /// reference hides it while the overlay is shown over the game rather than focused.
    pub active: bool,
    /// A key reached the field since the last change: the edit is the user's, not a write-back.
    user_editing: bool,
    prices: Prices,
    market: Option<wraex_core::leagues::SelectedLeague>,
    mode: Mode,
}

impl std::fmt::Debug for State {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("State")
            .field("game", &self.game)
            .field("tab", &self.tab)
            .field("results", &self.results)
            .field("selected", &self.selected)
            .field("active", &self.active)
            .finish()
    }
}

impl State {
    /// The widget on `game`, reading the prices `mode` answers.
    pub fn new(game: GameId, mode: Mode) -> Self {
        let prices = match game {
            GameId::Poe2 => Prices::Poe2(crate::poe2::price_check::prices::load(mode)),
            GameId::Poe1 => Prices::Poe1(PriceList::new(mode)),
        };
        Self {
            game,
            search: TextEdit::default(),
            query: String::new(),
            tab: default_tab(game),
            results: Results::Found(Vec::new()),
            selected: Vec::new(),
            active: true,
            user_editing: false,
            prices,
            market: None,
            mode,
        }
    }

    pub(crate) fn poll_market(&mut self, league: Option<wraex_core::leagues::SelectedLeague>) -> bool {
        let Prices::Poe1(prices) = &mut self.prices else { return false };
        if self.market != league {
            *prices = PriceList::new(self.mode);
            self.market = league;
            prices.queue_selected(self.market.as_ref(), false);
        }
        prices.poll()
    }

    /// `v-model="searchValue"`: the query, then the `computed` results.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_owned();
        self.search.replace_text(query);
        self.recompute();
    }

    /// `v-model="searchValue"`, after an edit: the field's text is the query.
    ///
    /// Only an edit to a *focused* field counts. Rune writes a blurred field's model back
    /// through `on_input_change`, and that write can land empty (the focus went to a button, not
    /// another field); the widget keeps the query and [`Self::sync_field`] puts it back.
    pub fn edited(&mut self) {
        // Only an edit a key caused counts; a write-back has no key (`edited_on_key`).
        if !self.user_editing {
            return;
        }
        self.user_editing = false;
        self.query = self.search.text.clone();
        self.recompute();
    }

    /// `on_input_key`: a key reached the field, so the change that follows is the user's.
    pub fn edited_on_key(&mut self) {
        self.user_editing = true;
    }

    /// Keep the field showing the query: a blurred field's model can come back empty, and the
    /// view rebuilds the element from it.
    pub fn sync_field(&mut self) -> bool {
        if self.search.focused || self.search.text == self.query {
            return false;
        }
        self.search.replace_text(&self.query);
        true
    }

    /// `typeFilter = ...`: another tab, then the results again.
    pub fn set_tab(&mut self, tab: Tab) {
        if tab == self.tab {
            return;
        }
        self.tab = tab;
        self.recompute();
    }

    /// `results`: the game's `findItems` over the tab's name list.
    pub fn recompute(&mut self) {
        let query = self.query.clone();
        self.results = match self.game {
            GameId::Poe1 => {
                let data = crate::poe1::price_check::data();
                match self.tab {
                    Tab::Gem => poe1_results(poe1_search::find_gems(data, "en", &query)),
                    _ => poe1_results(poe1_search::find_replicas(data, "en", &query)),
                }
            }
            GameId::Poe2 => {
                let data = crate::poe2::price_check::data(self.mode);
                let which = match self.tab {
                    Tab::Unique => poe2_search::TypeFilter::Unique,
                    Tab::BaseItem => poe2_search::TypeFilter::BaseItem,
                    _ => poe2_search::TypeFilter::Gem,
                };
                poe2_results(poe2_search::find(data, "en", &query, which))
            }
        };
    }

    /// `selectItem(item, …)`: price it, put it in the strip and clear the query.
    pub fn select(&mut self, index: usize) {
        let Some(record) = self.results.items().get(index).cloned() else { return };
        let (value, price) = self.price_for(&record);
        self.add(Selected { record, discr: None, value, price });
        self.query.clear();
        self.search.clear();
        self.recompute();
    }

    /// `useSelectedItems`'s `addItem`: a duplicate is not added; a full strip starts over with
    /// the new item; otherwise the strip stays sorted by value, descending.
    pub fn add(&mut self, new: Selected) {
        if self.selected.iter().any(|item| item.record.name() == new.record.name() && item.discr == new.discr) {
            return;
        }
        if self.selected.len() < MAX_SELECTED {
            self.selected.push(new);
            // `(b.chaos ?? 0) - (a.chaos ?? 0)`, with `total_cmp` so a NaN cannot panic.
            self.selected.sort_by(|a, b| b.value.unwrap_or(0.0).total_cmp(&a.value.unwrap_or(0.0)));
        } else {
            self.selected = vec![new];
        }
    }

    /// `clearSelectedItems`: the Reset button.
    pub fn clear_selected(&mut self) {
        self.selected.clear();
    }

    /// `findPriceByQuery` + `autoCurrency` for a picked record.
    fn price_for(&mut self, record: &Record) -> (Option<f64>, Option<Price>) {
        let variant = record.unique_base();
        match (&mut self.prices, record) {
            (Prices::Poe1(prices), Record::Poe1(_)) => {
                // `queuePricesFetch()`: the blob the lookup reads.
                if self.mode == Mode::Replay {
                    prices.queue(poe1_check::DEFAULT_LEAGUE, false);
                } else {
                    prices.queue_selected(self.market.as_ref(), false);
                }
                let query = Poe1Query { ns: record.namespace(), name: record.ref_name(), variant };
                let Some(found) = prices.find_price(&query) else { return (None, None) };
                let value = found.info.chaos;
                let price: Poe1Price = prices.prices().auto_currency(value);
                (Some(value), Some(Price { min: price.min, max: price.max, currency: price.currency }))
            }
            (Prices::Poe2(prices), Record::Poe2(_)) => {
                let query = Poe2Query {
                    ns: record.namespace().to_owned(),
                    name: record.ref_name().to_owned(),
                    variant: variant.map(str::to_owned),
                };
                let Some(found) = prices.find_price(&query) else { return (None, None) };
                let value = found.info.primary_value;
                let price: Poe2Price = prices.auto_currency(value, false);
                (Some(value), Some(Price { min: price.min, max: price.max, currency: price.currency }))
            }
            // The state's prices and its records always come from the same game.
            _ => (None, None),
        }
    }

    /// `selectedItemClick`: the virtual item a strip entry dispatches to the price check.
    pub fn picked(&self, index: usize) -> Option<Picked> {
        self.selected.get(index).map(|item| item.record.virtual_item())
    }
}

/// PoE1's `findItems` answer as this widget's.
fn poe1_results(results: poe1_search::SearchResults) -> Results {
    match results {
        poe1_search::SearchResults::TooMany => Results::TooMany,
        poe1_search::SearchResults::Found(items) => Results::Found(items.into_iter().map(Record::Poe1).collect()),
    }
}

/// EE2's `findItems` answer as this widget's.
fn poe2_results(results: poe2_search::SearchResults) -> Results {
    match results {
        poe2_search::SearchResults::TooMany => Results::TooMany,
        poe2_search::SearchResults::Found(items) => Results::Found(items.into_iter().map(Record::Poe2).collect()),
    }
}

/// The shell's half of a strip press: takes the [`Action::PriceCheckItem`] the widget recorded,
/// so [`crate::App::poll`] can reopen the price check on it.
pub fn take_picked(overlay: &mut Overlay) -> Option<Picked> {
    let index = overlay.actions.iter().position(|action| matches!(action, Action::PriceCheckItem(_)))?;
    match overlay.actions.remove(index) {
        Action::PriceCheckItem(picked) => Some(picked),
        _ => None,
    }
}

/// `WidgetItemSearch.vue`: the strip, the search field, the tabs and the results.
pub fn view<S: 'static>(overlay: &Overlay, _widget: &Widget, project: Project<S>) -> El {
    let Some(state) = overlay.item_search.as_ref() else { return rune::el().key("item-search") };
    // The window is the card: upstream's `.widget` is content-sized and the screenshot harness
    // crops it, so the scene's window is the reference card's size and the card fills it.
    let mut card = col()
        .key("item-search")
        .w_full()
        .min_w(theme::rem(24.0))
        .padding(theme::rem(0.25))
        .gap(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        .items_start();

    if !state.selected.is_empty() {
        card = card.child(selected_strip(state, project));
    }
    // `v-else` on `wm.active`: the search half is only there while the overlay has the pointer
    // (a scene's widget does; the reference's 4 s `UiTimeout` then hides the card's own clicks).
    if state.active {
        card = card.child(search_half(state, project));
    }
    card
}

/// `transition-group.selectedItems`: the strip of picked entries on gray-800.
fn selected_strip<S: 'static>(state: &State, project: Project<S>) -> El {
    let mut strip =
        row().key("item-search-selected").gap(theme::rem(0.25)).radius(theme::ROUNDED).fill(theme::gray(800));
    for (index, item) in state.selected.iter().enumerate() {
        let mut entry = col()
            .key(format!("item-search-selected-{index}"))
            .px(theme::rem(0.25))
            .pt(theme::rem(0.125))
            .pb(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .hover(theme::gray(700))
            .focusable(true)
            .role("button")
            .label(item.record.name().to_owned())
            .on_press(move |s: &mut S| open_selected(project(s), index))
            // `ItemQuickPrice :item-img :price currency-text`
            .child(price_line(state.game, &item.record, item.price))
            // `<div :class="$style.name">{{ item.info.name }}</div>`: `max-w-7rem`, truncated.
            .child(
                text(item.record.name().to_owned())
                    .key(format!("item-search-selected-{index}-name"))
                    .font(type_::SM)
                    .max_w(theme::rem(7.0))
                    .truncate(),
            );
        if let Some(discr) = &item.discr {
            entry = entry.child(
                text(crate::settings::t(state.game, discr))
                    .key(format!("item-search-selected-{index}-discr"))
                    .font(type_::SM)
                    .max_w(theme::rem(7.0))
                    .truncate(),
            );
        }
        strip = strip.child(entry);
    }
    strip
}

/// `ItemQuickPrice` with `currency-text`: the item's art, an arrow, and `price ¤currency`.
fn price_line(game: GameId, record: &Record, price: Option<Price>) -> El {
    let size = (theme::rem(2.0), theme::rem(2.0));
    // `UiItemImg.vue` draws `404.png` for an item the data has no icon for (EE2's only).
    let picture = if game == GameId::Poe2 && (record.icon().is_empty() || record.icon() == "%NOT_FOUND%") {
        art::local_sized(game, "404.png", size)
    } else {
        art::cdn(game, record.icon(), size)
    };
    let (min, max) = match (price, game) {
        (Some(price), GameId::Poe2) => (
            wraex_poe2::prices::display_rounding(price.min, false, false),
            wraex_poe2::prices::display_rounding(price.max, false, false),
        ),
        (Some(price), _) => (
            wraex_poe1::prices::display_rounding(price.min, false),
            wraex_poe1::prices::display_rounding(price.max, false),
        ),
        (None, _) => ("?".to_owned(), "?".to_owned()),
    };
    let mut line = row()
        .key("item-search-price")
        .items_center()
        .gap(theme::rem(0.25))
        .child(picture.key("item-search-price-icon"))
        // `<i class="fas fa-arrow-right text-gray-600 px-1 text-sm">`
        .child(icon::arrow_right(theme::rem(0.875), theme::rem(0.875), theme::gray(600)).mx(theme::rem(0.25)))
        .child(text(min.clone()).key("item-search-price-min").font(type_::BASE));
    if min != max {
        line = line.child(text(max).key("item-search-price-max").font(type_::BASE));
    }
    // `&nbsp;{{ price.currency }}`, nothing when there is no price.
    let currency = price.map(|price| format!("\u{a0}{}", price.currency)).unwrap_or_default();
    line.child(text(currency).key("item-search-price-currency").font(type_::BASE))
}

/// `searchWrapper`: the input row, the tab row and the results, on gray-800.
fn search_half<S: 'static>(state: &State, project: Project<S>) -> El {
    let game = state.game;
    let placeholder = crate::settings::t(game, "item_search.input");
    let input = text_input(&state.search, type_::SM)
        .key("item-search-input")
        .h(type_::SM.line_px())
        .single_line()
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        .caret(theme::gray(300), theme::gray(400).alpha(0.35))
        .color(theme::gray(300))
        .label(placeholder.clone())
        .placeholder(&placeholder, theme::gray(600))
        .placeholder_font(type_::SM)
        .grow(1.0)
        .min_w(0.0)
        .on_edit(move |s: &mut S| &mut project(s).item_search.as_mut().expect("an item-search scene").search)
        .on_input_key(move |s: &mut S, _event: &rune::KeyEvent, _tx: &mut rune::Tx| {
            if let Some(state) = project(s).item_search.as_mut() {
                state.edited_on_key();
            }
            false
        })
        .on_input_change(move |s: &mut S, _tx| {
            if let Some(state) = project(s).item_search.as_mut() {
                state.edited();
            }
        });

    // `<button @click="clearSelectedItems" class="btn"><i class="fas fa-times" /> Reset items</button>`
    let reset = row()
        .key("item-search-reset")
        .px(theme::rem(0.5))
        .py(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(700))
        .hover(theme::gray(600))
        .focusable(true)
        .role("button")
        .label(crate::settings::t(game, "item_search.reset"))
        .on_press(move |s: &mut S| {
            if let Some(state) = project(s).item_search.as_mut() {
                state.clear_selected();
            }
        })
        // `<i class="fas fa-times" />`
        .child(icon::times(theme::rem(0.875), theme::gray(400)).key("item-search-reset-icon"))
        .child(
            text(crate::settings::t(game, "item_search.reset"))
                .key("item-search-reset-label")
                .font(theme::leading_none(type_::SM))
                .color(theme::gray(400))
                .ml(theme::rem(0.25)),
        );

    // `<span>{{ t(':heist_target') }}</span>` and one button per tab.
    let mut tabs = row().key("item-search-tabs").gap(theme::rem(0.5)).px(theme::rem(0.5)).py(theme::rem(0.25)).child(
        text(crate::settings::t(game, "item_search.heist_target")).key("item-search-tabs-label").font(type_::SM),
    );
    let mut buttons = row().key("item-search-tab-buttons").gap(theme::rem(0.25));
    for tab in tabs_for(game) {
        let active = state.tab == *tab;
        let key = format!("item-search-tab-{}", tab.as_str());
        buttons = buttons.child(
            row()
                .key(key.clone())
                .px(theme::rem(0.5))
                .radius(theme::ROUNDED)
                .fill(theme::gray(900))
                // `:class="{ border: typeFilter === '…' }"`
                .border(if active { 1.0 } else { 0.0 }, theme::gray(200))
                .border_layout()
                .focusable(true)
                .role("button")
                .a11y_value(if active { "selected" } else { "not selected" })
                .label(tab_text(game, *tab))
                .on_press(move |s: &mut S| {
                    if let Some(state) = project(s).item_search.as_mut() {
                        state.set_tab(*tab);
                    }
                })
                .child(tab_label(game, *tab, &format!("{key}-label"))),
        );
    }
    tabs = tabs.child(buttons);

    let mut results = col().key("item-search-results").w_full();
    match &state.results {
        Results::TooMany => {
            // APT spells the key `:too_many`, EE2 `:too_many_`.
            let key = if game == GameId::Poe2 { "item_search.too_many_" } else { "item_search.too_many" };
            results = results.child(search_message("item-search-too-many", key, game, message_glyph()));
        }
        Results::Found(items) if items.is_empty() => {
            results =
                results.child(search_message("item-search-not-found", "item_search.not_found", game, message_glyph()));
        }
        Results::Found(items) => {
            for (index, record) in items.iter().enumerate() {
                results = results.child(result_row(game, record, index, project));
            }
        }
    }

    col()
        .key("item-search-search")
        .w_full()
        .radius(theme::ROUNDED)
        .fill(theme::gray(800))
        .child(
            row()
                .key("item-search-input-row")
                .w_full()
                .padding(theme::rem(0.25))
                .gap(theme::rem(0.25))
                .child(input)
                .child(reset),
        )
        .child(tabs)
        .child(results)
}

/// The message rows' glyph: `fa-exclamation-triangle` (`fa-search` is not among the ported
/// icons, so the "too many" row takes the triangle too).
fn message_glyph() -> El {
    // `<i class="fas fa-exclamation-triangle" />` in both games, in the inherited gray-400.
    icon::warning(theme::rem(0.875), theme::gray(400))
}

fn tabs_for(game: GameId) -> &'static [Tab] {
    tabs(game)
}

/// A tab's label. APT's replica tab names what it no longer searches: "Replicas, Base
/// items", the second half struck through and gray (`<span class="line-through
/// text-gray-600">Base items</span>`, a literal in the template).
fn tab_text(game: GameId, tab: Tab) -> String {
    let label = crate::settings::t(game, tab.label_key());
    if game == GameId::Poe1 && tab == Tab::Replica { format!("{label}, Base items") } else { label }
}

/// A tab's label element, with APT's struck-through tail.
fn tab_label(game: GameId, tab: Tab, key: &str) -> El {
    let text_value = tab_text(game, tab);
    if game == GameId::Poe1 && tab == Tab::Replica {
        let start = text_value.len() - "Base items".len();
        return rune::rich(text_value.clone(), vec![rune::Span::new(start, text_value.len()).color(theme::gray(600))])
            .key(key)
            .font(type_::SM)
            .strike(start, text_value.len(), None);
    }
    text(text_value).key(key).font(type_::SM)
}

/// `.searchMessage`: the centred note, its glyph and its text.
fn search_message(key: &str, label_key: &str, game: GameId, glyph: El) -> El {
    col().key(key).padding(theme::rem(2.0)).max_w(theme::rem(20.0)).child(
        row()
            .key(format!("{key}-line"))
            .gap(theme::rem(0.25))
            .items_center()
            .child(glyph)
            .child(text(crate::settings::t(game, label_key)).key(format!("{key}-label")).font(type_::SM)),
    )
}

/// `.searchItem`: the icon, the name and the Select button.
fn result_row<S: 'static>(game: GameId, record: &Record, index: usize, project: Project<S>) -> El {
    // `$style.icon`: `3rem` square on APT, `w-8 h-8` on EE2.
    let icon_size = if game == GameId::Poe2 { theme::rem(2.0) } else { theme::rem(3.0) };
    let picture = if game == GameId::Poe2 && (record.icon().is_empty() || record.icon() == "%NOT_FOUND%") {
        art::local_sized(game, "404.png", (icon_size, icon_size))
    } else {
        art::cdn(game, record.icon(), (icon_size, icon_size))
    };
    let name = match (game, record.unique_base()) {
        // EE2 shows the unique's base beside its name: `item.name + ' ' + item.unique.base`.
        (GameId::Poe2, Some(base)) => format!("{} {}", record.name(), base),
        _ => record.name().to_owned(),
    };
    let mut details = col().key(format!("item-search-result-{index}-details")).items_start().child(
        // `<div class="h-8 flex items-center px-1">`
        row()
            .key(format!("item-search-result-{index}-name"))
            .h(theme::rem(2.0))
            .px(theme::rem(0.25))
            .items_center()
            .child(text(name).key(format!("item-search-result-{index}-name-text")).font(type_::SM)),
    );
    // APT draws no button for a gem (`v-for="altQuality in []"`), EE2 one for every record.
    if game == GameId::Poe2 || record.is_unique() {
        // `.selectButtons > button`: `text-gray-600 px-1 rounded`, `text-gray-400 bg-gray-700`
        // while its row is hovered.
        details = details.child(
            row().key(format!("item-search-result-{index}-select-row")).child(
                row()
                    .key(format!("item-search-result-{index}-select"))
                    .px(theme::rem(0.25))
                    .radius(theme::ROUNDED)
                    .hover(theme::gray(700))
                    .focusable(true)
                    .role("button")
                    .label(crate::settings::t(game, "Select"))
                    .on_press(move |s: &mut S| {
                        if let Some(state) = project(s).item_search.as_mut() {
                            state.select(index);
                        }
                    })
                    .child(
                        text(crate::settings::t(game, "Select"))
                            .key(format!("item-search-result-{index}-select-label"))
                            .font(type_::SM)
                            .color(theme::gray(600)),
                    ),
            ),
        );
    }
    row()
        .key(format!("item-search-result-{index}"))
        .w_full()
        .min_h(theme::rem(3.0))
        .pl(theme::rem(0.25))
        .pt(theme::rem(0.25))
        .items_start()
        .hover(theme::gray(900))
        .child(picture)
        .child(details)
}

/// `selectedItemClick`: record the dispatch the shell turns into a price check.
fn open_selected(overlay: &mut Overlay, index: usize) {
    let Some(picked) = overlay.item_search.as_ref().and_then(|state| state.picked(index)) else { return };
    overlay.actions.push(Action::PriceCheckItem(picked));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widgets::Kind;

    fn state(game: GameId) -> State {
        State::new(game, Mode::Replay)
    }

    fn names(results: &Results) -> Vec<&str> {
        results.items().iter().map(Record::name).collect()
    }

    /// APT opens on the gem tab, which its pinned data has no entries for (it ships no
    /// alt-quality gems), so a query there finds nothing; the Replicas tab searches replicas.
    #[test]
    fn apt_searches_replica_uniques() {
        let mut apt = state(GameId::Poe1);
        assert_eq!(apt.tab, Tab::Gem);
        apt.set_query("headhunter");
        assert_eq!(names(&apt.results), Vec::<&str>::new());
        apt.set_tab(Tab::Replica);
        assert_eq!(names(&apt.results), ["Replica Headhunter"]);
        apt.set_tab(Tab::Gem);
        assert_eq!(apt.results, Results::Found(Vec::new()), "the tab recomputes the results");
        assert_eq!(tabs(GameId::Poe1), [Tab::Gem, Tab::Replica]);
    }

    /// EE2 opens on base items, and its three tabs search gems, uniques and base types.
    #[test]
    fn ee2_searches_three_tabs() {
        let mut poe2 = state(GameId::Poe2);
        assert_eq!(poe2.tab, Tab::BaseItem);
        poe2.set_query("divine");
        assert_eq!(names(&poe2.results), ["Divine Crown", "Divine Orb", "Runeforged Divine Crown"]);
        poe2.set_tab(Tab::Unique);
        poe2.set_query("Mageblood");
        assert_eq!(names(&poe2.results), ["Mageblood"]);
        poe2.set_tab(Tab::Gem);
        poe2.set_query("empower");
        assert_eq!(names(&poe2.results), ["Empowered Sparks I", "Empowered Sparks II"]);
        assert_eq!(tabs(GameId::Poe2), [Tab::Gem, Tab::Unique, Tab::BaseItem]);
    }

    /// Picking a match clears the query, prices it and puts it in the strip.
    #[test]
    fn a_pick_fills_the_strip() {
        let mut poe2 = state(GameId::Poe2);
        poe2.set_query("divine");
        poe2.select(1);
        assert_eq!(poe2.search.text, "", "`searchValue = ''`");
        assert_eq!(poe2.selected.len(), 1);
        let orb = &poe2.selected[0];
        assert_eq!(orb.record.name(), "Divine Orb");
        assert_eq!(orb.discr, None);
        // The recorded EE2 price database prices currency; `autoCurrency` names the core one.
        assert_eq!(orb.value, Some(1.0), "a divine is the primary currency");
        assert!(orb.price.is_some());

        // The recording has no uniques, so a unique shows the reference's `?`.
        let mut uniques = state(GameId::Poe2);
        uniques.set_tab(Tab::Unique);
        uniques.set_query("Mageblood");
        uniques.select(0);
        assert_eq!(uniques.selected[0].record.name(), "Mageblood");
        assert_eq!(uniques.selected[0].value, None);
        assert_eq!(uniques.selected[0].price, None);
    }

    /// `useSelectedItems`: no duplicates, at most five, sorted by value, and a full strip starts
    /// over.
    #[test]
    fn the_strip_holds_five_sorted_items() {
        fn record(index: usize) -> Record {
            Record::Poe2(wraex_poe2::data::BaseType {
                name: Some(format!("Item {index}")),
                ref_name: Some(format!("Item {index}")),
                namespace: wraex_poe2::data::Namespace::Item,
                icon: String::new(),
                w: None,
                h: None,
                trade_tag: None,
                trade_disc: None,
                disc: None,
                craftable: None,
                unique: None,
                map: None,
                gem: None,
                armour: None,
                augment: None,
                tags: Vec::new(),
            })
        }
        let mut widget = state(GameId::Poe2);
        for index in 0..7 {
            widget.add(Selected { record: record(index), discr: None, value: Some(index as f64), price: None });
        }
        // Seven picks: the sixth is the fifth-plus-one, so it replaces the strip; the seventh
        // then sits beside it, most valuable first.
        assert_eq!(widget.selected.len(), 2);
        assert_eq!(widget.selected[0].record.name(), "Item 6");
        assert_eq!(widget.selected[1].record.name(), "Item 5");
        // A duplicate is ignored.
        let duplicate = widget.selected[0].clone();
        widget.add(duplicate);
        assert_eq!(widget.selected.len(), 2);
        widget.clear_selected();
        assert!(widget.selected.is_empty());
    }

    /// A strip press dispatches the virtual item `createVirtualItem` builds for the record.
    #[test]
    fn a_strip_press_dispatches_the_virtual_item() {
        let mut overlay = Overlay::scene(GameId::Poe2, Kind::ItemSearch);
        {
            let state = overlay.item_search.as_mut().expect("the item search");
            state.set_tab(Tab::Unique);
            state.set_query("Mageblood");
            state.select(0);
        }
        let state = overlay.item_search.as_ref().unwrap();
        let Picked::Poe2(item) = state.picked(0).expect("the first strip entry") else { panic!("a PoE2 item") };
        let item = *item;
        assert_eq!(item.raw_text, "VIRTUAL_ITEM");
        assert_eq!(item.rarity, Some(wraex_core::Rarity::Unique));
        assert_eq!(item.info().name.as_deref(), Some("Mageblood"));
        assert!(item.name.is_empty(), "`createVirtualItem` leaves `name` undefined");

        open_selected(&mut overlay, 0);
        assert!(matches!(overlay.actions.as_slice(), [Action::PriceCheckItem(_)]));
        assert!(take_picked(&mut overlay).is_some());
        assert!(overlay.actions.is_empty(), "the shell took it");
    }

    /// A gem picks through `createVirtualItem({ category: Gem, gemLevel: 1 })`.
    #[test]
    fn a_gem_pick_is_level_one() {
        let mut poe2 = state(GameId::Poe2);
        poe2.set_tab(Tab::Gem);
        poe2.set_query("empower");
        poe2.select(1);
        let Picked::Poe2(item) = poe2.picked(0).expect("the gem") else { panic!("a PoE2 item") };
        let item = *item;
        assert_eq!(item.category, Some(wraex_poe2::parser::meta::ItemCategory::Gem));
        assert_eq!(item.gem_level, Some(1.0));
        assert_eq!(item.rarity, None);
    }

    #[test]
    fn tab_values_match_the_reference() {
        let values: Vec<&str> =
            [Tab::Gem, Tab::Replica, Tab::Unique, Tab::BaseItem].iter().map(|tab| tab.as_str()).collect();
        assert_eq!(values, ["gem", "replica", "unique", "base_item"]);
        assert_eq!(default_tab(GameId::Poe1), Tab::Gem);
        assert_eq!(default_tab(GameId::Poe2), Tab::BaseItem);
    }
}
