//! `ref/ee2/renderer/src/web/item-check/WidgetItemCheck.vue` and `ItemInfo.vue` in Rune.
//!
//! EE2's item check differs from APT's in its card (gray-800 with a gray-900 border, the name
//! and buttons on gray-900 bars), in "find same price" (a `$` button beside the stash search,
//! which searches the price note, optionally with the item's name), and in what makes an item
//! craftable (`itemIsModifiable`, not just `!corrupted && !mirrored`). The map check half is
//! [`crate::poe2::map_check`].
//!
//! Its buttons do not act here: each press appends the [`Action`] the app shell would run, so a
//! scene or a test can check it without opening a browser or typing into the game.

use rune::{Cx, El, col, el, row, text};
use serde::Deserialize;
use wraex_core::js::number_to_string;
use wraex_game::ParseError;
use wraex_poe2::item_check::{self, SamePricedType, UserAction};
use wraex_poe2::map_check::MapCheckConfig;
use wraex_poe2::parser::item::Item;
use wraex_poe2::parser::parse_clipboard;

use crate::poe2::i18n;
use crate::poe2::map_check;
use crate::theme::{self, text as type_};
use crate::trade::Mode;
use crate::ui::{self, icon};

/// How a view reaches this widget.
pub type Project<S> = fn(&mut S) -> &mut ItemCheck;

/// What a button asks the app to do, instead of doing it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `window.open(url)`: the item's wiki, poe2db or Craft of Exile page.
    Open(String),
    /// `Host.sendEvent(…)`: a stash search.
    Send(UserAction),
}

/// `WidgetItemCheck.vue`'s state.
#[derive(Debug)]
pub struct ItemCheck {
    /// The clipboard text the widget was opened with.
    pub text: String,
    /// `parseClipboard(text)`.
    pub item: Result<Item, ParseError>,
    /// The widget is shown.
    pub open: bool,
    /// `config.maps`: the map-check decisions (`defaultConfig()`).
    pub maps: MapCheckConfig,
    /// `AppConfig().language`: poe2db's path segment.
    pub language: String,
    /// `config.samePricedType`: whether the `$` search adds the item's name to the note.
    pub same_priced_type: SamePricedType,
    /// Where the game data comes from (the trade site in a window, the recording headless).
    pub mode: Mode,
    /// The actions the buttons asked for, oldest first.
    pub actions: Vec<Action>,
}

/// `AppConfig()`'s fields the item check reads (`WidgetItemCheck.vue`'s `AppConfig().language`).
#[derive(Debug, Deserialize)]
struct AppConfigView {
    #[serde(default = "default_language")]
    language: String,
}

/// `ItemCheckWidget`'s fields the item check reads.
///
/// Each is kept as a [`serde_json::Value`] so a malformed one falls back on its own, as the
/// reference's optional chains did.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ItemCheckWidgetView {
    #[serde(default)]
    maps: serde_json::Value,
    #[serde(default)]
    same_priced_type: serde_json::Value,
}

/// `'en'` when the config has no language.
fn default_language() -> String {
    "en".to_owned()
}

/// `defaultConfig()`'s `item-check` maps, when the config has none.
const fn empty_maps() -> MapCheckConfig {
    MapCheckConfig { profile: 1, show_new_stats: false, selected_stats: Vec::new() }
}

/// The item-check widget's `maps`, the app's language and `samePricedType`, from a config: a
/// scene reads `defaultConfig()`, a live window the config the shell loaded.
fn widget_config_from(config: &serde_json::Value) -> (MapCheckConfig, String, SamePricedType) {
    let language = AppConfigView::deserialize(config).map_or_else(|_| default_language(), |view| view.language);
    let (maps, same_priced_type) = crate::settings::widget(config, "item-check")
        .and_then(|widget| ItemCheckWidgetView::deserialize(widget).ok())
        .map_or((empty_maps(), SamePricedType::Both), |view| {
            (
                MapCheckConfig::deserialize(&view.maps).unwrap_or_else(|_| empty_maps()),
                SamePricedType::deserialize(&view.same_priced_type).unwrap_or(SamePricedType::Both),
            )
        });
    (maps, language, same_priced_type)
}

impl ItemCheck {
    /// The widget on `text`, with the default config and the game data from `mode`.
    pub fn from_text(text: &str, mode: Mode) -> Self {
        Self::from_config(text, &wraex_poe2::config::default_config(), mode)
    }

    /// The widget on `text` over the config the app loaded (`WidgetItemCheck.vue`'s
    /// `props.config` and `AppConfig().language`).
    pub fn from_config(text: &str, config: &serde_json::Value, mode: Mode) -> Self {
        let data = crate::poe2::price_check::data(mode);
        let item = parse_clipboard(data, text).map_err(|fail| ParseError::Rejected { key: fail.0 });
        let (maps, language, same_priced_type) = widget_config_from(config);
        Self {
            text: text.to_owned(),
            open: item.is_ok(),
            item,
            maps,
            language,
            same_priced_type,
            mode,
            actions: Vec::new(),
        }
    }

    /// The parsed item, when the text was one.
    pub fn item(&self) -> Option<&Item> {
        self.item.as_ref().ok()
    }

    /// The actions the buttons asked for, emptied.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// `openWiki`
    pub fn open_wiki(&mut self) {
        if let Some(url) = self.item().map(item_check::open_wiki) {
            self.actions.push(Action::Open(url));
        }
    }

    /// `openPoedb`
    pub fn open_poedb(&mut self) {
        let language = self.language.clone();
        if let Some(url) = self.item().map(|item| item_check::open_poedb(item, &language)) {
            self.actions.push(Action::Open(url));
        }
    }

    /// `openCoE`
    pub fn open_coe(&mut self) {
        if let Some(url) = self.item().map(item_check::open_coe) {
            self.actions.push(Action::Open(url));
        }
    }

    /// `findSimilarItems`
    pub fn find_similar_items(&mut self) {
        if let Some(action) = self.item().map(item_check::find_similar_items) {
            self.actions.push(Action::Send(action));
        }
    }

    /// `findSamePricedItems`: nothing without a price note.
    pub fn find_same_priced_items(&mut self) {
        let kind = self.same_priced_type;
        if let Some(action) = self.item().and_then(|item| item_check::find_same_priced_items(item, kind)) {
            self.actions.push(Action::Send(action));
        }
    }
}

/// `WidgetItemCheck.vue`: the map check for a map-like item, otherwise the item's info.
pub fn view<S: 'static>(check: &ItemCheck, cx: &mut Cx<S>, project: Project<S>) -> El {
    let Some(item) = check.item() else { return el().key("item-check") };
    if !check.open {
        return el().key("item-check");
    }
    cx.foreground(theme::gray(200));
    if wraex_poe2::map_check::is_map_like(item) {
        return map_check::view(item, &check.maps, check.mode, project);
    }
    item_info(item, project)
}

/// `ItemInfo.vue`: the item's name, its action buttons, and the weapon DPS.
fn item_info<S: 'static>(item: &Item, project: Project<S>) -> El {
    let mut card = col()
        .key("item-check")
        .w_full()
        .max_w(theme::rem(20.0))
        .fill(theme::gray(800))
        .border(theme::rem(0.25), theme::gray(900))
        .border_layout()
        .child(
            row()
                .key("item-check-name")
                .w_full()
                .fill(theme::gray(900))
                .px(theme::rem(1.0))
                .py(theme::rem(0.25))
                .justify_center()
                .child(
                    text(item.info().name.clone().unwrap_or_else(|| "undefined".to_owned()))
                        .key("item-check-name-text")
                        .font(type_::SM),
                ),
        )
        .child(actions(item, project));
    if let Some(dps) = weapon_dps(item) {
        card = card.child(dps_grid(&dps));
    }
    // The window is the app's, wider than the card: align the card at the top left.
    col().key("item-check-window").w_full().items_start().child(card)
}

/// The weapon's physical and elemental DPS (`Math.round(as * damage)`).
struct WeaponDps {
    physical: f64,
    elemental: f64,
    total: f64,
}

fn weapon_dps(item: &Item) -> Option<WeaponDps> {
    let as_ = item.weapon_as?;
    let physical = (as_ * item.weapon_physical.unwrap_or(0.0)).round();
    let elemental = (as_ * item.weapon_elemental.unwrap_or(0.0)).round();
    Some(WeaponDps { physical, elemental, total: physical + elemental })
}

/// The buttons row: wiki, poedb, CoE (a craftable, modifiable item), the separator, the stash
/// search, and `$` for the price note.
fn actions<S: 'static>(item: &Item, project: Project<S>) -> El {
    let mut buttons = row()
        .key("item-check-actions")
        .w_full()
        .fill(theme::gray(900))
        .py(theme::rem(0.25))
        .gap(theme::rem(0.25))
        .items_center()
        .child(
            ui::button("item-check-wiki", "wiki")
                .grow(1.0)
                .justify_center()
                .on_press(move |s: &mut S| project(s).open_wiki()),
        )
        .child(
            ui::button("item-check-poedb", "poedb")
                .grow(1.0)
                .justify_center()
                .on_press(move |s: &mut S| project(s).open_poedb()),
        );
    // `showCoE`: `item.info.craftable && itemIsModifiable(item)`
    if item.info().craftable.is_some() && item.is_modifiable() {
        buttons = buttons.child(
            ui::button("item-check-coe", "CoE")
                .grow(1.0)
                .justify_center()
                .on_press(move |s: &mut S| project(s).open_coe()),
        );
    }
    buttons = buttons
        .child(icon::ellipsis_vertical(theme::text::SM.size, theme::gray(600)).key("item-check-separator"))
        .child(
            ui::button("item-check-stash", &i18n::t("item.find_in_stash"))
                .grow(1.0)
                .justify_center()
                .on_press(move |s: &mut S| project(s).find_similar_items()),
        );
    // `showPrice`: `props.item.note`
    if item.note.as_deref().is_some_and(|note| !note.is_empty()) {
        buttons = buttons.child(
            row()
                .key("item-check-price")
                .px(theme::rem(0.5))
                .py(theme::rem(0.25))
                .radius(theme::ROUNDED)
                .fill(theme::gray(700))
                .center_y()
                .justify_center()
                .focusable(true)
                .role("button")
                .label("find same price")
                .on_press(move |s: &mut S| project(s).find_same_priced_items())
                // `<i class="fas fa-dollar text-xs">`
                .child(icon::dollar(theme::rem(0.75), theme::gray(400)).key("item-check-price-icon")),
        );
    }
    buttons
}

/// The label column's width, `auto` in the reference's grid; wide enough for `Elemental DPS:`.
const DPS_LABEL: f32 = 5.75 * theme::REM;
/// The value column's width, so the numbers line up at their right edge.
const DPS_VALUE: f32 = 1.75 * theme::REM;

/// `grid mx-auto gap-x-4 my-2`, two columns with the values right-aligned.
fn dps_grid(dps: &WeaponDps) -> El {
    let mut grid = col().key("item-check-dps").self_center().my(theme::rem(0.5));
    for (index, (label, value)) in [
        (i18n::fmt("item.physical_dps", &[""]), dps.physical),
        (i18n::fmt("item.elemental_dps", &[""]), dps.elemental),
        (i18n::fmt("item.total_dps", &[""]), dps.total),
    ]
    .into_iter()
    .enumerate()
    {
        grid = grid.child(
            row()
                .key(format!("item-check-dps-{index}"))
                .gap(theme::rem(1.0))
                .items_center()
                .child(text(label).key(format!("item-check-dps-{index}-label")).font(type_::SM).w(DPS_LABEL))
                .child(
                    text(number_to_string(value))
                        .key(format!("item-check-dps-{index}-value"))
                        .font(type_::SM)
                        .w(DPS_VALUE)
                        .align(rune::Align::End),
                ),
        );
    }
    grid
}
