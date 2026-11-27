//! `ref/apt/renderer/src/web/item-check/WidgetItemCheck.vue` and `ItemInfo.vue` in Rune.
//!
//! The item check is the widget a hotkey opens on the item under the cursor: the map check
//! ([`crate::poe1::map_check`]) for a map-like item, and the item info's name, wiki / poedb /
//! Craft of Exile buttons, stash search and the weapon DPS or dust figures otherwise.
//!
//! Its buttons do not act here: `window.open` and `Host.sendEvent` belong to the app shell, as
//! upstream's `hotkeyable-actions.ts` hands them to the main process. Each press appends the
//! [`Action`] the shell would run, so a scene or a test can check it without opening a browser
//! or typing into the game. The URLs are [`wraex_poe1::item_check`]'s, the fixture-checked port
//! of that file.

use rune::{Cx, El, col, el, row, text};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use wraex_core::js::number_to_string;
use wraex_game::ParseError;
use wraex_poe1::item_check::{self, UserAction};
use wraex_poe1::map_check::MapCheckConfig;
use wraex_poe1::parser::item::Item;
use wraex_poe1::parser::parse_clipboard;

use crate::poe1::i18n;
use crate::poe1::map_check;
use crate::theme::{self, text as type_};
use crate::ui::{self, art, icon};

/// How a view reaches this widget. Rune runs a handler with the app's state (`&mut S`), so a
/// window that isn't the whole app is rendered with the projection that finds it.
pub type Project<S> = fn(&mut S) -> &mut ItemCheck;

/// What a button asks the app to do, instead of doing it: the port of `window.open(url)` and
/// `Host.sendEvent({ name: 'CLIENT->MAIN::user-action', … })`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// `window.open(url)`: the item's wiki, poedb or Craft of Exile page.
    Open(String),
    /// `Host.sendEvent(…)`: a stash search.
    Send(UserAction),
}

/// `WidgetItemCheck.vue`'s state: the item the game's copy key produced, the map decisions, and
/// the actions its buttons asked for.
///
/// Built from the item text (`MAIN->CLIENT::item-text` with `target: 'item-check'`), so a scene
/// or a test is just [`ItemCheck::from_text`].
#[derive(Debug)]
pub struct ItemCheck {
    /// The clipboard text the widget was opened with.
    pub text: String,
    /// `parseClipboard(text)`.
    pub item: Result<Item, ParseError>,
    /// The widget is shown. A failed parse hides it, as upstream never calls `wm.show`.
    pub open: bool,
    /// `config.maps`: the item check widget's map-check decisions (`defaultConfig()`).
    pub maps: MapCheckConfig,
    /// `AppConfig().language`: poedb's path segment.
    pub language: String,
    /// The actions the buttons asked for, oldest first. The app shell drains them
    /// ([`ItemCheck::take_actions`]); a test reads them.
    pub actions: Vec<Action>,
}

/// The config fields the item check reads (`WidgetItemCheck.vue`'s `props.config.maps` and
/// `AppConfig().language`).
#[derive(Debug, Default, Deserialize)]
struct ConfigView {
    /// `AppConfig().language`: poedb's path segment.
    #[serde(default, deserialize_with = "lenient")]
    language: Option<String>,
    /// `config.widgets`: each entry is deserialised into a [`WidgetView`], so one malformed
    /// entry does not hide the `item-check` widget behind it.
    #[serde(default, deserialize_with = "lenient")]
    widgets: Vec<Value>,
}

/// The parts of a `config.widgets` entry the item check reads.
#[derive(Debug, Default, Deserialize)]
struct WidgetView {
    /// `widget.wmType`.
    #[serde(rename = "wmType", default, deserialize_with = "lenient")]
    wm_type: Option<String>,
    /// Left as a `Value`: a malformed `maps` only discards the map-check decisions, as the old
    /// `serde_json::from_value(..).ok()` did.
    #[serde(default)]
    maps: Option<Value>,
}

/// Reads one field leniently: a value of the wrong shape falls back to `T`'s default, as the
/// old `.and_then(Value::as_*)` chain did, instead of discarding the rest of the view.
fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + DeserializeOwned,
{
    let value = Value::deserialize(deserializer)?;
    Ok(T::deserialize(&value).unwrap_or_default())
}

/// The item-check widget's `maps`, and the app's `language`, from a config: a scene reads
/// `defaultConfig()`, a live window the config the shell loaded.
fn widget_config_from(config: &Value) -> (MapCheckConfig, String) {
    let view = ConfigView::deserialize(config).unwrap_or_default();
    let language = view.language.unwrap_or_else(|| "en".to_owned());
    let window = view
        .widgets
        .iter()
        .filter_map(|widget| WidgetView::deserialize(widget).ok())
        .find(|widget| widget.wm_type.as_deref() == Some("item-check"));
    let maps = window
        .and_then(|widget| widget.maps)
        .and_then(|maps| MapCheckConfig::deserialize(&maps).ok())
        .unwrap_or(MapCheckConfig { profile: 1, show_new_stats: false, selected_stats: Vec::new() });
    (maps, language)
}

impl ItemCheck {
    /// The widget on `text`, with the default map-check config.
    pub fn from_text(text: &str) -> Self {
        Self::from_config(text, &wraex_poe1::config::default_config())
    }

    /// The widget on `text` over the config the app loaded (`WidgetItemCheck.vue`'s
    /// `props.config.maps` and `AppConfig().language`).
    pub fn from_config(text: &str, config: &Value) -> Self {
        let data = crate::poe1::price_check::data();
        let item = parse_clipboard(data, text).map_err(|fail| ParseError::Rejected { key: fail.0.into() });
        let (maps, language) = widget_config_from(config);
        Self { text: text.to_owned(), open: item.is_ok(), item, maps, language, actions: Vec::new() }
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
}

/// `WidgetItemCheck.vue`: the map check for a map-like item, otherwise the item's info.
pub fn view<S: 'static>(check: &ItemCheck, cx: &mut Cx<S>, project: Project<S>) -> El {
    let Some(item) = check.item() else { return el().key("item-check") };
    if !check.open {
        return el().key("item-check");
    }
    // `text-gray-200` on the card, inherited by everything inside it.
    cx.foreground(theme::gray(200));
    if wraex_poe1::map_check::is_map_like(item) {
        return map_check::view(item, &check.maps, project);
    }
    item_info(item, project)
}

/// `ItemInfo.vue`: the item's name, its action buttons, and the DPS/dust figures.
fn item_info<S: 'static>(item: &Item, project: Project<S>) -> El {
    // The window is the app's, wider than the card: align the card at the top left and keep it
    // at its own 20 rem.
    col().key("item-check-window").w_full().items_start().child(
        col()
            .key("item-check")
            .w_full()
            .max_w(theme::rem(20.0))
            .padding(theme::rem(0.25))
            .fill(theme::gray(900))
            .child(
                row()
                    .key("item-check-name")
                    .w_full()
                    .px(theme::rem(1.0))
                    .py(theme::rem(0.25))
                    .justify_center()
                    .child(text(item.info().name.clone()).key("item-check-name-text").font(type_::SM)),
            )
            .child(actions(item, project))
            .child(info_panel(item)),
    )
}

/// `.actionBtns`: wiki, poedb, CoE (a craftable, uncorrupted, unmirrored item), the separator
/// and the stash search.
fn actions<S: 'static>(item: &Item, project: Project<S>) -> El {
    let mut buttons = row().key("item-check-actions").w_full().py(theme::rem(0.25)).gap(theme::rem(0.25));
    buttons = buttons
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
    // `showCoE`: `item.info.craftable && !item.isCorrupted && !item.isMirrored`
    let show_coe = item.info().craftable.is_some() && item.is_corrupted.is_none() && item.is_mirrored != Some(true);
    if show_coe {
        buttons = buttons.child(
            ui::button("item-check-coe", "CoE")
                .grow(1.0)
                .justify_center()
                .on_press(move |s: &mut S| project(s).open_coe()),
        );
    }
    buttons.child(icon::ellipsis_vertical(theme::text::SM.size, theme::gray(600)).key("item-check-separator")).child(
        ui::button("item-check-stash", &i18n::t("item.find_in_stash"))
            .grow(1.0)
            .justify_center()
            .on_press(move |s: &mut S| project(s).find_similar_items()),
    )
}

/// `.itemInfo`: the centred two-column grid of the weapon DPS and the dust value, on gray-800.
fn info_panel(item: &Item) -> El {
    let mut rows = col().key("item-check-info-rows").items_center();
    if let Some(as_) = item.weapon_as {
        let physical = (as_ * item.weapon_physical.unwrap_or(0.0)).round();
        let elemental = (as_ * item.weapon_elemental.unwrap_or(0.0)).round();
        for (index, (label, value)) in [
            (i18n::fmt("item.physical_dps", &[""]), physical),
            (i18n::fmt("item.elemental_dps", &[""]), elemental),
            (i18n::fmt("item.total_dps", &[""]), physical + elemental),
        ]
        .into_iter()
        .enumerate()
        {
            rows = rows.child(info_row(index, &label, &number_to_string(value)));
        }
    }
    if let Some(dust) = item.dust_equivalent {
        rows = rows.child(
            row()
                .key("item-check-dust")
                .gap(theme::rem(1.0))
                .items_center()
                .child(info_label("item-check-dust-label", &i18n::t("item.disenchanting")))
                .child(
                    row()
                        .key("item-check-dust-value")
                        .min_w(DPS_VALUE)
                        .justify_end()
                        .gap(theme::rem(0.25))
                        .items_center()
                        .child(text(grouped(dust)).key("item-check-dust-amount").font(type_::SM))
                        // `<img src="/images/dust.png" class="w-5">`
                        .child(
                            art::local(wraex_core::GameId::Poe1, "dust.png", theme::rem(1.25))
                                .key("item-check-dust-icon"),
                        ),
                ),
        );
    }
    col().key("item-check-info").w_full().py(theme::rem(0.5)).fill(theme::gray(800)).items_center().child(rows)
}

/// The label column's width, `auto` in the reference's grid; wide enough for `Elemental DPS: `.
const DPS_LABEL: f32 = 5.75 * theme::REM;
/// The value column's width, so the numbers line up at their right edge.
const DPS_VALUE: f32 = 1.75 * theme::REM;

fn info_row(index: usize, label: &str, value: &str) -> El {
    row()
        .key(format!("item-check-dps-{index}"))
        .gap(theme::rem(1.0))
        .items_center()
        .child(info_label(&format!("item-check-dps-{index}-label"), label))
        .child(
            text(value.to_owned())
                .key(format!("item-check-dps-{index}-value"))
                .font(type_::SM)
                .w(DPS_VALUE)
                .align(rune::Align::End),
        )
}

fn info_label(key: &str, label: &str) -> El {
    text(label.to_owned()).key(key).font(type_::SM).w(DPS_LABEL)
}

/// `Number#toLocaleString()`'s grouping for a whole number ("36,225").
fn grouped(value: f64) -> String {
    let negative = value < 0.0;
    let text = number_to_string(value.abs().trunc());
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut out = String::with_capacity(whole.len().saturating_add(whole.len() / 3));
    for (index, digit) in whole.chars().enumerate() {
        if index > 0 && whole.len().saturating_sub(index).is_multiple_of(3) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_grouped_like_the_browser() {
        assert_eq!(grouped(36225.0), "36,225");
        assert_eq!(grouped(0.0), "0");
        assert_eq!(grouped(1000.0), "1,000");
        assert_eq!(grouped(-1234.0), "-1,234");
    }
}
