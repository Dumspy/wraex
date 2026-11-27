//! `ref/apt/renderer/src/web/Config.ts`: the settings, as upstream stores them.
//!
//! The config stays JSON ([`Value`]) so every field survives, including those
//! of widgets Wraex doesn't have yet. [`default_config`] is `defaultConfig()`
//! (each widget's `initInstance`/`defaultInstances`), [`load`] is
//! `initConfig()` (`upgradeConfig` included) and [`host_config`] is
//! `getConfigForHost()`. Writing `undefined` to a field removes it, as the
//! JSON round trip in `updateConfig` does.
//!
//! The defaults and the host config are built from the typed structs below
//! (one per widget interface), so the shape upstream defines is checked by the
//! compiler; they become JSON only through [`wraex_core::js::json_value`], which
//! writes numbers as `JSON.stringify` does. The migrations over historic files
//! still walk untyped JSON, since it has every shape the interface ever had.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value, json};
use wraex_core::config_upgrade::{R, obj, text, truthy, upgrade_to_16, version, widget, widgets};
use wraex_core::js::json_value;
use wraex_core::num::usize_to_f64;
use wraex_core::widget::{Anchor, Command, Exclusive, Wants, Widget, Zorder};

use crate::map_check::{MapCheckConfig, MapCheckStat};

pub use wraex_core::config_upgrade::ConfigError;

/// `configVersion` of the current format.
pub const CONFIG_VERSION: u32 = 18;

/// The `Widget` fields of a default instance (`wmId` 0 until `defaultConfig()`
/// numbers them).
fn widget_base(wm_type: &str, wm_title: &str, wm_wants: Wants, wm_zorder: Option<Zorder>, wm_flags: &[&str]) -> Widget {
    Widget {
        wm_id: 0.0,
        wm_type: wm_type.to_owned(),
        wm_title: wm_title.to_owned(),
        wm_wants,
        wm_zorder,
        wm_flags: wm_flags.iter().map(|flag| (*flag).to_owned()).collect(),
    }
}

/// `WidgetMenu` (`overlay/WidgetMenu.vue`'s `initInstance`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetMenu {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub always_show: bool,
    /// Fields a newer config has that this struct does not know.
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// `WidgetSettings` (`settings/SettingsWindow.vue`'s `initInstance`): only the
/// base fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SettingsWidget {
    #[serde(flatten)]
    pub base: Widget,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// `ItemSearchWidget` (`item-search/WidgetItemSearch.vue`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSearchWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub ocr_gems_key: Option<String>,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// `StashSearchWidget` (`stash-search/WidgetStashSearch.vue`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashSearchWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub enable_hotkeys: bool,
    pub entries: Vec<StashSearchEntry>,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// An entry of [`StashSearchWidget::entries`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StashSearchEntry {
    pub id: f64,
    pub name: String,
    pub text: String,
    pub hotkey: Option<String>,
}

/// `showSeller`: off, or which of the seller's names to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ShowSeller {
    Off(bool),
    Name(String),
}

/// `PriceCheckWidget` (`price-check/PriceCheckWindow.vue`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(
    clippy::struct_excessive_bools,
    reason = "the widget mirrors upstream's `PriceCheckWidget` interface field for field"
)]
pub struct PriceCheckWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub show_rate_limit_state: bool,
    pub api_latency_seconds: f64,
    /// `api` or `app`.
    pub collapse_listings: String,
    pub smart_initial_search: bool,
    pub locked_initial_search: bool,
    pub activate_stock_filter: bool,
    pub builtin_browser: bool,
    pub hotkey: Option<String>,
    pub hotkey_hold: String,
    pub hotkey_locked: Option<String>,
    pub show_seller: ShowSeller,
    pub search_stat_range: f64,
    pub show_cursor: bool,
    pub request_price_prediction: bool,
    pub merchant_only: bool,
    pub default_currency: Option<String>,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// `ItemCheckWidget` (`item-check/WidgetItemCheck.vue`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemCheckWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub hotkey: Option<String>,
    pub wiki_key: Option<String>,
    pub poedb_key: Option<String>,
    pub craft_of_exile_key: Option<String>,
    pub stash_search_key: Option<String>,
    pub maps: MapCheckConfig,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// `ImageStripWidget` (`overlay/WidgetImageStrip.vue`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStripWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub images: Vec<StripImage>,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// An entry of [`ImageStripWidget::images`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StripImage {
    pub id: f64,
    pub url: String,
}

/// `DelveGridWidget` (`overlay/WidgetDelveGrid.vue`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelveGridWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub toggle_key: Option<String>,
    #[serde(default, flatten)]
    pub extra: Map<String, Value>,
}

/// A widget instance (`widget.Widget`), in its type's struct.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum WidgetConfig {
    Menu(WidgetMenu),
    Settings(SettingsWidget),
    ItemSearch(ItemSearchWidget),
    StashSearch(StashSearchWidget),
    PriceCheck(PriceCheckWidget),
    ItemCheck(ItemCheckWidget),
    ImageStrip(ImageStripWidget),
    DelveGrid(DelveGridWidget),
    /// A widget type from a newer config: kept exactly as it was read.
    Unknown(Value),
}

impl WidgetConfig {
    /// The instance's `Widget` fields, when this is a type Wraex knows.
    pub const fn base(&self) -> Option<&Widget> {
        match self {
            Self::Menu(w) => Some(&w.base),
            Self::Settings(w) => Some(&w.base),
            Self::ItemSearch(w) => Some(&w.base),
            Self::StashSearch(w) => Some(&w.base),
            Self::PriceCheck(w) => Some(&w.base),
            Self::ItemCheck(w) => Some(&w.base),
            Self::ImageStrip(w) => Some(&w.base),
            Self::DelveGrid(w) => Some(&w.base),
            Self::Unknown(_) => None,
        }
    }

    const fn base_mut(&mut self) -> Option<&mut Widget> {
        match self {
            Self::Menu(w) => Some(&mut w.base),
            Self::Settings(w) => Some(&mut w.base),
            Self::ItemSearch(w) => Some(&mut w.base),
            Self::StashSearch(w) => Some(&mut w.base),
            Self::PriceCheck(w) => Some(&mut w.base),
            Self::ItemCheck(w) => Some(&mut w.base),
            Self::ImageStrip(w) => Some(&mut w.base),
            Self::DelveGrid(w) => Some(&mut w.base),
            Self::Unknown(_) => None,
        }
    }

    /// `wmType`
    fn wm_type(&self) -> Option<&str> {
        self.base().map(|base| base.wm_type.as_str())
    }
}

/// The `wmType` of a widget instance, to pick its struct.
#[derive(Debug, Deserialize)]
struct WidgetTag {
    #[serde(rename = "wmType")]
    wm_type: String,
}

impl<'de> Deserialize<'de> for WidgetConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error as _;

        let value = Value::deserialize(deserializer)?;
        let tag: WidgetTag = serde_json::from_value(value.clone()).map_err(D::Error::custom)?;
        let widget = match tag.wm_type.as_str() {
            "menu" => Self::Menu(serde_json::from_value(value).map_err(D::Error::custom)?),
            "settings" => Self::Settings(serde_json::from_value(value).map_err(D::Error::custom)?),
            "item-search" => Self::ItemSearch(serde_json::from_value(value).map_err(D::Error::custom)?),
            "stash-search" => Self::StashSearch(serde_json::from_value(value).map_err(D::Error::custom)?),
            "price-check" => Self::PriceCheck(serde_json::from_value(value).map_err(D::Error::custom)?),
            "item-check" => Self::ItemCheck(serde_json::from_value(value).map_err(D::Error::custom)?),
            "image-strip" => Self::ImageStrip(serde_json::from_value(value).map_err(D::Error::custom)?),
            "delve-grid" => Self::DelveGrid(serde_json::from_value(value).map_err(D::Error::custom)?),
            _ => Self::Unknown(value),
        };
        Ok(widget)
    }
}

/// `Config` (`web/Config.ts`), as `defaultConfig()` writes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[expect(clippy::struct_excessive_bools, reason = "the settings mirror upstream's `Config` interface field for field")]
pub struct Config {
    pub config_version: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub league_id: Option<String>,
    pub overlay_key: String,
    pub overlay_background: String,
    pub overlay_background_close: bool,
    pub restore_clipboard: bool,
    pub show_attach_notification: bool,
    pub commands: Vec<Command>,
    #[serde(default)]
    pub client_log: Option<String>,
    #[serde(default)]
    pub game_config: Option<String>,
    pub window_title: String,
    pub log_keys: bool,
    pub account_name: String,
    pub stash_scroll: bool,
    pub language: String,
    pub realm: String,
    pub use_intl_site: bool,
    pub font_size: f64,
    pub widgets: Vec<WidgetConfig>,
}

/// The widgets of `defaultConfig()`, in the registry's order
/// (`overlay/widget-registry.ts`), without their ids.
fn default_widgets() -> Vec<WidgetConfig> {
    let mut widgets = vec![default_menu(), default_settings(), default_item_search()];
    // WidgetTimer.vue: `multi`, no default instances.
    widgets.extend(default_stash_searches());
    widgets.push(default_price_check());
    widgets.push(default_item_check());
    widgets.push(default_image_strip());
    widgets.push(default_delve_grid());
    widgets
}

/// `WidgetMenu.vue`'s `initInstance`.
fn default_menu() -> WidgetConfig {
    WidgetConfig::Menu(WidgetMenu {
        base: widget_base("menu", "", Wants::Show, Some(Zorder::Order(1.0)), &["invisible-on-blur", "menu::skip"]),
        anchor: Anchor { pos: "tl".to_owned(), x: 5.0, y: 5.0 },
        always_show: false,
        extra: Map::new(),
    })
}

/// `SettingsWindow.vue`'s `initInstance`.
fn default_settings() -> WidgetConfig {
    WidgetConfig::Settings(SettingsWidget {
        base: widget_base(
            "settings",
            "{icon=fa-cog}",
            Wants::Hide,
            Some(Zorder::Exclusive(Exclusive::Exclusive)),
            &["invisible-on-blur", "ignore-ui-visibility"],
        ),
        extra: Map::new(),
    })
}

/// `WidgetItemSearch.vue`'s `initInstance`.
fn default_item_search() -> WidgetConfig {
    WidgetConfig::ItemSearch(ItemSearchWidget {
        base: widget_base("item-search", "{icon=fa-search}", Wants::Hide, None, &["invisible-on-blur"]),
        anchor: Anchor { pos: "tl".to_owned(), x: 10.0, y: 20.0 },
        ocr_gems_key: None,
        extra: Map::new(),
    })
}

/// `WidgetStashSearch.vue`'s `defaultInstances`.
fn default_stash_searches() -> [WidgetConfig; 2] {
    let entry = |id: f64, text: &str| StashSearchEntry { id, name: String::new(), text: text.to_owned(), hotkey: None };
    [
        WidgetConfig::StashSearch(StashSearchWidget {
            base: widget_base("stash-search", "Map rolling", Wants::Hide, None, &["invisible-on-blur"]),
            anchor: Anchor { pos: "tl".to_owned(), x: 35.0, y: 46.0 },
            enable_hotkeys: true,
            entries: vec![
                entry(1.0, "\"Pack Size: +3\""),
                entry(2.0, "Reflect"),
                entry(3.0, "\"Cannot Leech Life\""),
                entry(4.0, "\"Cannot Leech Mana\""),
            ],
            extra: Map::new(),
        }),
        WidgetConfig::StashSearch(StashSearchWidget {
            base: widget_base("stash-search", "Dump sorting", Wants::Hide, None, &["invisible-on-blur"]),
            anchor: Anchor { pos: "tl".to_owned(), x: 34.0, y: 56.0 },
            enable_hotkeys: true,
            entries: vec![
                entry(1.0, "Currency"),
                entry(2.0, "\"Divination Card\""),
                entry(3.0, "Fossil"),
                entry(4.0, "\"Map Tier\""),
                entry(5.0, "\"Map Device\" \"Rarity: Normal\""),
                entry(6.0, "Tane Laboratory"),
            ],
            extra: Map::new(),
        }),
    ]
}

/// `PriceCheckWindow.vue`'s `initInstance`.
fn default_price_check() -> WidgetConfig {
    WidgetConfig::PriceCheck(PriceCheckWidget {
        base: widget_base(
            "price-check",
            "",
            Wants::Hide,
            Some(Zorder::Exclusive(Exclusive::Exclusive)),
            &["hide-on-blur", "menu::skip"],
        ),
        show_rate_limit_state: false,
        api_latency_seconds: 2.0,
        collapse_listings: "api".to_owned(),
        smart_initial_search: true,
        locked_initial_search: true,
        activate_stock_filter: false,
        builtin_browser: false,
        hotkey: Some("D".to_owned()),
        hotkey_hold: "Ctrl".to_owned(),
        hotkey_locked: Some("Ctrl + Alt + D".to_owned()),
        show_seller: ShowSeller::Off(false),
        search_stat_range: 10.0,
        show_cursor: true,
        request_price_prediction: false,
        merchant_only: true,
        default_currency: None,
        extra: Map::new(),
    })
}

/// `WidgetItemCheck.vue`'s `initInstance`.
fn default_item_check() -> WidgetConfig {
    WidgetConfig::ItemCheck(ItemCheckWidget {
        base: widget_base(
            "item-check",
            "",
            Wants::Hide,
            Some(Zorder::Exclusive(Exclusive::Exclusive)),
            &["hide-on-blur", "menu::skip"],
        ),
        hotkey: None,
        wiki_key: None,
        poedb_key: None,
        craft_of_exile_key: None,
        stash_search_key: None,
        maps: MapCheckConfig {
            profile: 1,
            show_new_stats: false,
            selected_stats: vec![
                MapCheckStat {
                    matcher: "Players have #% to all maximum Resistances".to_owned(),
                    decision: "w--".to_owned(),
                },
                MapCheckStat {
                    matcher: "Monsters reflect #% of Physical Damage".to_owned(),
                    decision: "d--".to_owned(),
                },
                MapCheckStat {
                    matcher: "Monsters reflect #% of Elemental Damage".to_owned(),
                    decision: "d--".to_owned(),
                },
                MapCheckStat { matcher: "Area contains two Unique Bosses".to_owned(), decision: "g--".to_owned() },
            ],
        },
        extra: Map::new(),
    })
}

/// `WidgetImageStrip.vue`'s `defaultInstances`.
fn default_image_strip() -> WidgetConfig {
    WidgetConfig::ImageStrip(ImageStripWidget {
        base: widget_base("image-strip", "Cheat sheets", Wants::Hide, None, &["invisible-on-blur"]),
        anchor: Anchor { pos: "tc".to_owned(), x: 50.0, y: 10.0 },
        images: vec![StripImage { id: 1.0, url: "syndicate.jpg".to_owned() }],
        extra: Map::new(),
    })
}

/// `WidgetDelveGrid.vue`'s `initInstance`.
fn default_delve_grid() -> WidgetConfig {
    WidgetConfig::DelveGrid(DelveGridWidget {
        base: widget_base("delve-grid", "", Wants::Hide, None, &["hide-on-focus", "menu::skip"]),
        toggle_key: None,
        extra: Map::new(),
    })
}

/// `defaultConfig()`
pub fn default_config() -> Value {
    let commands: Vec<Command> = [
        ("/hideout", Some("F5")),
        ("/exit", Some("F9")),
        ("@last ty", None),
        ("/invite @last", None),
        ("/tradewith @last", None),
        ("/hideout @last", None),
    ]
    .into_iter()
    .map(|(text, hotkey)| Command { text: text.to_owned(), hotkey: hotkey.map(str::to_owned), send: true })
    .collect();
    let mut widgets = default_widgets();
    for (i, widget) in widgets.iter_mut().enumerate() {
        if let Some(base) = widget.base_mut() {
            base.wm_id = usize_to_f64(i) + 1.0;
        }
    }
    json_value(&Config {
        config_version: f64::from(CONFIG_VERSION),
        league_id: None,
        overlay_key: "Shift + Space".to_owned(),
        overlay_background: "rgba(129, 139, 149, 0.15)".to_owned(),
        overlay_background_close: true,
        restore_clipboard: false,
        show_attach_notification: true,
        commands,
        client_log: None,
        game_config: None,
        window_title: "Path of Exile".to_owned(),
        log_keys: false,
        account_name: String::new(),
        stash_scroll: true,
        language: "en".to_owned(),
        realm: "pc-ggg".to_owned(),
        use_intl_site: false,
        font_size: 16.0,
        widgets,
    })
}

/// A default widget of `wm_type` (`defaultConfig().widgets.find(...)!`).
fn default_widget(wm_type: &str) -> Value {
    default_widgets()
        .into_iter()
        .find(|widget| widget.wm_type() == Some(wm_type))
        .map_or(Value::Null, |widget| json_value(&widget))
}

/// What `initConfig` ends with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub config: Value,
    /// An unreadable file is replaced by the defaults, saved as a temporary
    /// file (`saveConfig({ isTemporary: true })`).
    pub save_temporary: bool,
}

/// `initConfig`: the defaults without a file, else the file upgraded.
pub fn load(contents: Option<&str>) -> R<Loaded> {
    let Some(contents) = contents.filter(|c| !c.is_empty()) else {
        return Ok(Loaded { config: default_config(), save_temporary: false });
    };
    match serde_json::from_str::<Value>(contents) {
        Ok(config) => Ok(Loaded { config: upgrade_config(config)?, save_temporary: false }),
        Err(_) => Ok(Loaded { config: default_config(), save_temporary: true }),
    }
}

/// `upgradeConfig`: brings an older config up to [`CONFIG_VERSION`].
pub fn upgrade_config(mut config: Value) -> R<Value> {
    upgrade_to_16(&mut config, &default_widget)?;
    fill_defaults(&mut config)?;
    if version(&config) < 17.0 {
        to_17(&mut config)?;
    }
    if version(&config) < 18.0 {
        to_18(&mut config);
    }
    Ok(config)
}

/// The defaults `upgradeConfig` adds after step 16, before its last two steps.
#[expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn fill_defaults(c: &mut Value) -> R<()> {
    let o = obj(c)?;
    if !o.contains_key("logKeys") {
        o.insert("logKeys".into(), json!(false));
    }

    let pc = widget(c, "price-check")?;
    if !pc.contains_key("merchantOnly") {
        pc.insert("merchantOnly".into(), json!(true));
    }
    if !pc.contains_key("defaultCurrency") {
        pc.insert("defaultCurrency".into(), Value::Null);
    }

    for w in widgets(c)?.iter_mut().filter(|w| w["wmType"] == "stash-search") {
        // `??=`: also replaces `null`.
        if w.get("enableHotkeys").is_none_or(Value::is_null) {
            w["enableHotkeys"] = json!(true);
        }
    }
    Ok(())
}

/// `upgradeConfig` step 17: the widget menu's flags and the settings icons.
#[expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_17(c: &mut Value) -> R<()> {
    for w in widgets(c)?.iter_mut() {
        for flag in w.get_mut("wmFlags").and_then(Value::as_array_mut).into_iter().flatten() {
            if flag == "skip-menu" {
                *flag = json!("menu::skip");
            }
        }
    }
    widget(c, "item-search")?.insert("wmTitle".into(), json!("{icon=fa-search}"));
    widget(c, "settings")?.insert("wmTitle".into(), json!("{icon=fa-cog}"));
    // The settings icon first in the widget menu. Stable, like `Array.prototype.sort`.
    widgets(c)?.sort_by_key(|w| w["wmType"] != "settings");
    c["configVersion"] = json!(17);
    Ok(())
}

/// `upgradeConfig` step 18: `useIntlSite`.
#[expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_18(c: &mut Value) {
    let intl = c.get("language") == Some(&json!("cmn-Hant")) && c.get("realm") == Some(&json!("pc-ggg"));
    c["useIntlSite"] = json!(intl);
    c["configVersion"] = json!(18);
}

/// A config field copied to the host config as-is: [`Copied::Missing`] when the
/// config has no such key (upstream's `undefined`), otherwise its JSON value
/// (`null` included).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Copied {
    #[default]
    Missing,
    Value(Value),
}

impl Copied {
    const fn value(&self) -> Option<&Value> {
        match self {
            Self::Missing => None,
            Self::Value(value) => Some(value),
        }
    }

    fn as_str(&self) -> Option<&str> {
        match self.value()? {
            Value::String(text) => Some(text),
            _ => None,
        }
    }

    /// `${value}`, as `config_upgrade::text` reads it.
    fn as_text(&self) -> String {
        text(self.value())
    }

    /// `truthy(value)`
    fn is_truthy(&self) -> bool {
        truthy(self.value())
    }

    const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

impl Serialize for Copied {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Missing => serializer.serialize_none(),
            Self::Value(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for Copied {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Value::deserialize(deserializer).map(Self::Value)
    }
}

/// `ShortcutAction` (`@ipc/types`): what a hotkey does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ShortcutAction {
    CopyItem {
        target: &'static str,
        #[serde(rename = "focusOverlay", skip_serializing_if = "Option::is_none")]
        focus_overlay: Option<bool>,
    },
    ToggleOverlay,
    PasteInChat {
        text: Copied,
        send: Copied,
    },
    TriggerEvent {
        target: String,
    },
    StashSearch {
        text: Copied,
    },
    OcrText {
        target: &'static str,
    },
}

/// A shortcut (`Shortcut` in `@ipc/types`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Shortcut {
    pub shortcut: String,
    pub action: ShortcutAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep_mod_keys: Option<bool>,
}

/// `HostConfig` (`@ipc/types`): the global shortcuts and what the host process
/// needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostConfig {
    pub shortcuts: Vec<Shortcut>,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub restore_clipboard: Copied,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub client_log: Copied,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub game_config: Copied,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub stash_scroll: Copied,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub overlay_key: Copied,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub log_keys: Copied,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub window_title: Copied,
    #[serde(skip_serializing_if = "Copied::is_missing")]
    pub language: Copied,
}

/// The config `getConfigForHost` reads, leniently: any field may be missing
/// (upstream's `undefined`) or hold anything a hand-edited file has.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostConfigView {
    #[serde(default)]
    restore_clipboard: Copied,
    #[serde(default)]
    client_log: Copied,
    #[serde(default)]
    game_config: Copied,
    #[serde(default)]
    stash_scroll: Copied,
    #[serde(default)]
    overlay_key: Copied,
    #[serde(default)]
    log_keys: Copied,
    #[serde(default)]
    window_title: Copied,
    #[serde(default)]
    language: Copied,
    /// Decoded through [`decode_entries`] in [`host_config`].
    #[serde(default, deserialize_with = "lenient_list")]
    commands: Vec<Value>,
    /// Decoded through [`decode_entries`] in [`host_config`].
    #[serde(default, deserialize_with = "lenient_list")]
    widgets: Vec<Value>,
}

/// Anything but an array reads as an empty list, as upstream's `.as_array()`
/// (`undefined`) does.
fn lenient_list<'de, D>(deserializer: D) -> Result<Vec<Value>, D::Error>
where
    D: Deserializer<'de>,
{
    match Value::deserialize(deserializer)? {
        Value::Array(items) => Ok(items),
        _ => Ok(Vec::new()),
    }
}

/// The entries of a config list that read as their typed shape. Anything else
/// (something a hand-edited file put there) is skipped, as upstream's property
/// reads on it give `undefined`.
fn decode_entries<T: DeserializeOwned>(entries: &[Value]) -> Vec<T> {
    entries.iter().filter_map(|entry| serde_json::from_value(entry.clone()).ok()).collect()
}

/// An entry of `config.commands`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CommandView {
    #[serde(default)]
    hotkey: Copied,
    #[serde(default)]
    text: Copied,
    #[serde(default)]
    send: Copied,
}

/// A widget instance, for the fields `getConfigForHost` reads from any type.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct WidgetView {
    #[serde(default)]
    wm_type: Copied,
    #[serde(default)]
    wm_id: Copied,
    #[serde(default)]
    hotkey: Copied,
    #[serde(default)]
    hotkey_hold: Copied,
    #[serde(default)]
    hotkey_locked: Copied,
    #[serde(default)]
    wiki_key: Copied,
    #[serde(default)]
    poedb_key: Copied,
    #[serde(default)]
    craft_of_exile_key: Copied,
    #[serde(default)]
    stash_search_key: Copied,
    #[serde(default)]
    toggle_key: Copied,
    #[serde(default)]
    reset_key: Copied,
    #[serde(default)]
    ocr_gems_key: Copied,
    #[serde(default)]
    enable_hotkeys: Copied,
    #[serde(default)]
    entries: Vec<EntryView>,
}

/// An entry of a stash search widget's `entries`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntryView {
    #[serde(default)]
    hotkey: Copied,
    #[serde(default)]
    text: Copied,
}

/// `getConfigForHost`: the global shortcuts and what the host process needs.
pub fn host_config(config: &Value) -> R<Value> {
    let view: HostConfigView =
        serde_json::from_value(config.clone()).map_err(|error| ConfigError(format!("Cannot read config: {error}")))?;
    let widgets: Vec<WidgetView> = decode_entries(&view.widgets);
    let commands: Vec<CommandView> = decode_entries(&view.commands);
    let actions = host_actions(&view, &widgets, &commands)?;
    Ok(json_value(&HostConfig {
        shortcuts: actions,
        restore_clipboard: view.restore_clipboard,
        client_log: view.client_log,
        game_config: view.game_config,
        stash_scroll: view.stash_scroll,
        overlay_key: view.overlay_key,
        log_keys: view.log_keys,
        window_title: view.window_title,
        language: view.language,
    }))
}

/// `config.widgets.find(w => w.wmType === type)!`
fn find_widget<'a>(widgets: &'a [WidgetView], wm_type: &str) -> R<&'a WidgetView> {
    widgets
        .iter()
        .find(|widget| widget.wm_type.as_str() == Some(wm_type))
        .ok_or_else(|| ConfigError(format!("Cannot read properties of undefined ({wm_type})")))
}

/// The shortcuts `getConfigForHost` builds, in the order upstream pushes them.
fn host_actions(view: &HostConfigView, widgets: &[WidgetView], commands: &[CommandView]) -> R<Vec<Shortcut>> {
    let mut actions = Vec::new();

    let pc = find_widget(widgets, "price-check")?;
    if pc.hotkey.is_truthy() {
        actions.push(Shortcut {
            shortcut: format!("{} + {}", pc.hotkey_hold.as_text(), pc.hotkey.as_text()),
            action: ShortcutAction::CopyItem { target: "price-check", focus_overlay: Some(false) },
            keep_mod_keys: Some(true),
        });
    }
    if pc.hotkey_locked.is_truthy() {
        actions.push(Shortcut {
            shortcut: pc.hotkey_locked.as_text(),
            action: ShortcutAction::CopyItem { target: "price-check", focus_overlay: Some(true) },
            keep_mod_keys: None,
        });
    }
    actions.push(Shortcut {
        shortcut: view.overlay_key.as_text(),
        action: ShortcutAction::ToggleOverlay,
        keep_mod_keys: Some(true),
    });

    let ic = find_widget(widgets, "item-check")?;
    for (key, target) in [
        (&ic.wiki_key, "open-wiki"),
        (&ic.craft_of_exile_key, "open-craft-of-exile"),
        (&ic.poedb_key, "open-poedb"),
        (&ic.stash_search_key, "search-similar"),
    ] {
        if key.is_truthy() {
            actions.push(Shortcut {
                shortcut: key.as_text(),
                action: ShortcutAction::CopyItem { target, focus_overlay: None },
                keep_mod_keys: None,
            });
        }
    }
    if ic.hotkey.is_truthy() {
        actions.push(Shortcut {
            shortcut: ic.hotkey.as_text(),
            action: ShortcutAction::CopyItem { target: "item-check", focus_overlay: Some(true) },
            keep_mod_keys: None,
        });
    }

    let delve = find_widget(widgets, "delve-grid")?;
    if delve.toggle_key.is_truthy() {
        actions.push(Shortcut {
            shortcut: delve.toggle_key.as_text(),
            action: ShortcutAction::TriggerEvent { target: "delve-grid".to_owned() },
            keep_mod_keys: Some(true),
        });
    }

    command_actions(commands, &mut actions);
    widget_actions(widgets, &mut actions);
    Ok(actions)
}

/// The shortcuts `config.commands` add.
fn command_actions(commands: &[CommandView], actions: &mut Vec<Shortcut>) {
    for command in commands {
        if command.hotkey.is_truthy() {
            actions.push(Shortcut {
                shortcut: command.hotkey.as_text(),
                action: ShortcutAction::PasteInChat { text: command.text.clone(), send: command.send.clone() },
                keep_mod_keys: None,
            });
        }
    }
}

/// The shortcuts the stash search, timer and item search widgets add.
fn widget_actions(widgets: &[WidgetView], actions: &mut Vec<Shortcut>) {
    for widget in widgets {
        match widget.wm_type.as_str() {
            Some("stash-search") => {
                if !widget.enable_hotkeys.is_truthy() {
                    continue;
                }
                for entry in &widget.entries {
                    if entry.hotkey.is_truthy() {
                        actions.push(Shortcut {
                            shortcut: entry.hotkey.as_text(),
                            action: ShortcutAction::StashSearch { text: entry.text.clone() },
                            keep_mod_keys: None,
                        });
                    }
                }
            }
            Some("timer") => {
                let id = widget.wm_id.as_text();
                for (key, event) in
                    [(&widget.toggle_key, "stopwatch-start-stop"), (&widget.reset_key, "stopwatch-reset")]
                {
                    if key.is_truthy() {
                        actions.push(Shortcut {
                            shortcut: key.as_text(),
                            action: ShortcutAction::TriggerEvent { target: format!("{event}:{id}") },
                            keep_mod_keys: Some(true),
                        });
                    }
                }
            }
            Some("item-search") if widget.ocr_gems_key.is_truthy() => {
                actions.push(Shortcut {
                    shortcut: widget.ocr_gems_key.as_text(),
                    action: ShortcutAction::OcrText { target: "heist-gems" },
                    keep_mod_keys: Some(true),
                });
            }
            _ => {}
        }
    }
}
