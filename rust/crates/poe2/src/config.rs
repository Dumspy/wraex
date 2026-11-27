//! `ref/ee2/renderer/src/web/Config.ts`: the settings, as EE2 stores them.
//!
//! The config stays JSON ([`Value`]) so every field survives, including those
//! of widgets Wraex doesn't have yet. [`default_config`] is `defaultConfig()`
//! (each widget's `initInstance`/`defaultInstances`), [`load`] is
//! `initConfig()` (`upgradeConfig` included) and [`host_config`] is
//! `getConfigForHost()`. Writing `undefined` to a field removes it, as the
//! JSON round trip in `updateConfig` does.

use indexmap::IndexMap;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Value, json};

use wraex_core::config_upgrade::{R, new_widget, obj, shortcut, text, truthy, upgrade_to_16, version, widget, widgets};
use wraex_core::js::json_value;
use wraex_core::num::{f64_to_usize, usize_to_f64};
use wraex_core::widget::{Anchor, Command, Exclusive, Wants, Widget, Zorder};

pub use wraex_core::config_upgrade::ConfigError;

/// `configVersion` of the current format.
pub const CONFIG_VERSION: u32 = 35;

/// `Config` (`web/Config.ts`): the settings file.
///
/// `widgets` stays [`Value`]: the list mixes one object per widget type, and a widget Wraex
/// doesn't know must survive a load and a save. [`Config::widget`] reads one through its typed
/// struct.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors upstream's `Config` interface (web/Config.ts), one bool per setting"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub config_version: f64,
    /// Set once the leagues load (`leagueId?`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub league_id: Option<String>,
    pub overlay_key: String,
    pub overlay_background: String,
    pub overlay_background_close: bool,
    pub overlay_always_close: bool,
    pub hide_overlay_on_blur: bool,
    pub restore_clipboard: bool,
    pub show_attach_notification: bool,
    pub commands: Vec<Command>,
    pub client_log: Option<String>,
    pub game_config: Option<String>,
    pub window_title: String,
    pub log_keys: bool,
    pub account_name: String,
    pub stash_scroll: bool,
    pub language: String,
    pub preferred_trade_site: String,
    pub realm: String,
    pub font_size: f64,
    pub widgets: Vec<Value>,
    pub enable_alphas: bool,
    pub alphas: Vec<String>,
    pub tips_frequency: f64,
    pub read_client_log: bool,
}

impl Config {
    /// The first widget of `wm_type`, read through its typed struct (`AppConfig(type)`).
    pub fn widget<T: DeserializeOwned>(&self, wm_type: &str) -> Option<T> {
        let widget = self.widgets.iter().find(|w| widget_type(w) == Some(wm_type))?;
        serde_json::from_value(widget.clone()).ok()
    }
}

/// `WidgetMenu` (`web/overlay/widgets.ts`): the widget menu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetMenu {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub always_show: bool,
}

/// `ItemSearchWidget` (`web/item-search/widget.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemSearchWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub ocr_gems_key: Option<String>,
}

/// `XpWidget` (`web/leveling/widget.ts`): the experience tracker.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XpWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub show_exp: bool,
}

/// One entry of `StashSearchWidget.entries`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StashSearchEntry {
    pub id: f64,
    pub name: String,
    pub text: String,
    pub hotkey: Option<String>,
}

/// `StashSearchWidget` (`web/stash-search/widget.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StashSearchWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub enable_hotkeys: bool,
    pub entries: Vec<StashSearchEntry>,
}

/// `PriceCheckWidget` (`web/overlay/widgets.ts`).
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors upstream's `PriceCheckWidget` interface, one bool per setting"
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceCheckWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub show_rate_limit_state: bool,
    pub api_latency_seconds: f64,
    pub collapse_listings: String,
    pub smart_initial_search: bool,
    pub locked_initial_search: bool,
    pub activate_stock_filter: bool,
    pub builtin_browser: bool,
    pub hotkey: Option<String>,
    pub hotkey_hold: String,
    pub hotkey_locked: Option<String>,
    /// `false`, `"account"` or `"ign"`, so several JSON types.
    pub show_seller: Value,
    pub search_stat_range: f64,
    pub show_cursor: bool,
    pub request_price_prediction: bool,
    pub remember_currency: bool,
    /// EE2's own settings.
    pub default_all_selected: bool,
    pub item_hover_tooltip: String,
    pub always_show_tier: bool,
    pub core_currency: String,
    pub currency_volume: String,
    pub remember_listing_type: bool,
    pub initial_delay: f64,
    pub saved_augments: IndexMap<String, Vec<Option<String>>>,
}

/// `ItemCheckWidget` (`web/item-check/widget.ts`).
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
    pub same_priced_key: Option<String>,
    pub same_priced_type: String,
    pub maps: crate::map_check::MapCheckConfig,
}

/// One entry of `ImageStripWidget.images`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageStripImage {
    pub id: f64,
    pub url: String,
}

/// `ImageStripWidget` (`web/overlay/widgets.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageStripWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub images: Vec<ImageStripImage>,
}

/// `DelveGridWidget` (`web/overlay/widgets.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DelveGridWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub toggle_key: Option<String>,
}

/// `StopwatchWidget` (`web/overlay/widgets.ts`): the timer widget.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StopwatchWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub toggle_key: Option<String>,
    pub reset_key: Option<String>,
}

/// `NotepadWidget` (`web/overlay/widgets.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotepadWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub notepad_body: String,
    /// `0`, `1` or `2`.
    pub notepad_size: f64,
}

/// `ColumnOpts` (`web/library/widget.ts`): the columns a library profile writes.
#[expect(clippy::struct_excessive_bools, reason = "mirrors upstream's `ColumnOpts` interface, one bool per column")]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnOpts {
    pub ref_name: bool,
    pub item_level: bool,
    pub rarity: bool,
    pub sockets: bool,
    pub mods: bool,
    pub added_mods: bool,
    pub removed_mods: bool,
    pub keep: ColumnKeep,
    pub mod_opts: ColumnModOpts,
}

/// `ColumnOpts["keep"]`: which modifier types the profile keeps.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors upstream's `ColumnOpts[\"keep\"]` interface, one bool per modifier type"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnKeep {
    pub explicit: bool,
    pub implicit: bool,
    pub enchant: bool,
    pub augment: bool,
}

/// `ColumnOpts["modOpts"]`: which parts of a modifier the profile writes.
#[expect(
    clippy::struct_excessive_bools,
    reason = "mirrors upstream's `ColumnOpts[\"modOpts\"]` interface, one bool per part"
)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnModOpts {
    pub name: bool,
    pub tier: bool,
    pub roll: bool,
    #[serde(rename = "ref")]
    pub stat_ref: bool,
    #[serde(rename = "type")]
    pub kind: bool,
    pub generation: bool,
}

/// `LibraryWidget` (`web/library/widget.ts`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryWidget {
    #[serde(flatten)]
    pub base: Widget,
    pub anchor: Anchor,
    pub log_item_key: Option<String>,
    pub library_output_path: Option<String>,
    pub selected_profile: String,
    pub profiles: IndexMap<String, ColumnOpts>,
}

/// `w.wmType`, when it is a string.
fn widget_type(w: &Value) -> Option<&str> {
    match w {
        Value::Object(m) => match m.get("wmType") {
            Some(Value::String(s)) => Some(s),
            _ => None,
        },
        _ => None,
    }
}

/// `WidgetMenu.vue`'s `initInstance()`.
fn menu_instance(wm_id: f64) -> Value {
    json_value(&WidgetMenu {
        base: Widget {
            wm_id,
            wm_type: "menu".to_owned(),
            wm_title: String::new(),
            wm_wants: Wants::Show,
            wm_zorder: Some(Zorder::Order(1.0)),
            wm_flags: vec!["invisible-on-blur".to_owned(), "menu::skip".to_owned()],
        },
        anchor: Anchor { pos: "tl".to_owned(), x: 5.0, y: 5.0 },
        always_show: false,
    })
}

/// `SettingsWindow.vue`'s `initInstance()`.
fn settings_instance(wm_id: f64) -> Value {
    json_value(&Widget {
        wm_id,
        wm_type: "settings".to_owned(),
        wm_title: "{icon=fa-cog}".to_owned(),
        wm_wants: Wants::Hide,
        wm_zorder: Some(Zorder::Exclusive(Exclusive::Exclusive)),
        wm_flags: vec!["invisible-on-blur".to_owned(), "ignore-ui-visibility".to_owned()],
    })
}

/// `WidgetItemSearch.vue`'s `initInstance()`.
fn item_search_instance(wm_id: f64) -> Value {
    json_value(&ItemSearchWidget {
        base: Widget {
            wm_id,
            wm_type: "item-search".to_owned(),
            wm_title: "{icon=fa-search}".to_owned(),
            wm_wants: Wants::Hide,
            wm_zorder: None,
            wm_flags: vec!["invisible-on-blur".to_owned()],
        },
        anchor: Anchor { pos: "tl".to_owned(), x: 10.0, y: 20.0 },
        ocr_gems_key: None,
    })
}

/// `WidgetXpTracker.vue`'s `initInstance()`.
fn experience_tracker_instance(wm_id: f64) -> Value {
    json_value(&XpWidget {
        base: Widget {
            wm_id,
            wm_type: "experience-tracker".to_owned(),
            wm_title: "XP".to_owned(),
            wm_wants: Wants::Hide,
            wm_zorder: None,
            wm_flags: Vec::new(),
        },
        anchor: Anchor { pos: "bc".to_owned(), x: 68.0, y: 98.0 },
        show_exp: true,
    })
}

/// A `StashSearchWidget`, from `WidgetStashSearch.vue`'s `defaultInstances()`.
fn stash_search_instance(wm_id: f64, wm_title: &str, anchor: Anchor, entries: &[(f64, &str)]) -> Value {
    let entries = entries
        .iter()
        .map(|(id, text)| StashSearchEntry { id: *id, name: String::new(), text: (*text).to_owned(), hotkey: None })
        .collect();
    json_value(&StashSearchWidget {
        base: Widget {
            wm_id,
            wm_type: "stash-search".to_owned(),
            wm_title: wm_title.to_owned(),
            wm_wants: Wants::Hide,
            wm_zorder: None,
            wm_flags: vec!["invisible-on-blur".to_owned()],
        },
        anchor,
        enable_hotkeys: true,
        entries,
    })
}

/// `WidgetStashSearch.vue`'s first default instance.
fn map_rolling_instance(wm_id: f64) -> Value {
    stash_search_instance(
        wm_id,
        "Map rolling",
        Anchor { pos: "tl".to_owned(), x: 35.0, y: 46.0 },
        &[
            (1.0, "\"Increased Pack Size\""),
            (2.0, "\"Additional Packs\""),
            (3.0, "\"Burning Ground\""),
            (4.0, "\"Less Recovery Rate\""),
        ],
    )
}

/// `WidgetStashSearch.vue`'s second default instance.
fn dump_sorting_instance(wm_id: f64) -> Value {
    stash_search_instance(
        wm_id,
        "Dump sorting",
        Anchor { pos: "tl".to_owned(), x: 34.0, y: 56.0 },
        &[
            (1.0, "Currency"),
            (2.0, "\"Martial Weapon\""),
            (3.0, "Jewel"),
            (4.0, "\"Waystone Tier\""),
            (5.0, "\"Rarity: Normal\""),
            (6.0, "Trials:"),
        ],
    )
}

/// `PriceCheckWindow.vue`'s `initInstance()`.
fn price_check_instance(wm_id: f64) -> Value {
    json_value(&PriceCheckWidget {
        base: Widget {
            wm_id,
            wm_type: "price-check".to_owned(),
            wm_title: String::new(),
            wm_wants: Wants::Hide,
            wm_zorder: Some(Zorder::Exclusive(Exclusive::Exclusive)),
            wm_flags: vec!["hide-on-blur".to_owned(), "menu::skip".to_owned()],
        },
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
        show_seller: Value::Bool(false),
        search_stat_range: 10.0,
        show_cursor: true,
        request_price_prediction: false,
        remember_currency: false,
        default_all_selected: false,
        item_hover_tooltip: "keybind".to_owned(),
        always_show_tier: false,
        core_currency: "exalted".to_owned(),
        currency_volume: "both".to_owned(),
        remember_listing_type: false,
        initial_delay: 48.0,
        saved_augments: IndexMap::new(),
    })
}

/// `WidgetItemCheck.vue`'s `initInstance()`.
fn item_check_instance(wm_id: f64) -> Value {
    json_value(&ItemCheckWidget {
        base: Widget {
            wm_id,
            wm_type: "item-check".to_owned(),
            wm_title: String::new(),
            wm_wants: Wants::Hide,
            wm_zorder: Some(Zorder::Exclusive(Exclusive::Exclusive)),
            wm_flags: vec!["hide-on-blur".to_owned(), "menu::skip".to_owned()],
        },
        hotkey: None,
        wiki_key: None,
        poedb_key: None,
        craft_of_exile_key: None,
        stash_search_key: None,
        same_priced_key: None,
        same_priced_type: "both".to_owned(),
        maps: crate::map_check::MapCheckConfig {
            profile: 1,
            show_new_stats: false,
            selected_stats: vec![
                crate::map_check::MapCheckStat {
                    matcher: "#% maximum Player Resistances".to_owned(),
                    decision: "w--".to_owned(),
                },
                crate::map_check::MapCheckStat {
                    matcher: "Monsters reflect #% of Physical Damage".to_owned(),
                    decision: "d--".to_owned(),
                },
                crate::map_check::MapCheckStat {
                    matcher: "Monsters reflect #% of Elemental Damage".to_owned(),
                    decision: "d--".to_owned(),
                },
                crate::map_check::MapCheckStat {
                    matcher: "Area contains two Unique Bosses".to_owned(),
                    decision: "g--".to_owned(),
                },
            ],
        },
    })
}

/// `WidgetImageStrip.vue`'s `defaultInstances()`.
fn image_strip_instance(wm_id: f64) -> Value {
    json_value(&ImageStripWidget {
        base: Widget {
            wm_id,
            wm_type: "image-strip".to_owned(),
            wm_title: "Cheat sheets".to_owned(),
            wm_wants: Wants::Hide,
            wm_zorder: None,
            wm_flags: vec!["invisible-on-blur".to_owned()],
        },
        anchor: Anchor { pos: "tc".to_owned(), x: 50.0, y: 10.0 },
        images: vec![ImageStripImage { id: 1.0, url: "syndicate.jpg".to_owned() }],
    })
}

/// `WidgetDelveGrid.vue`'s `initInstance()`.
fn delve_grid_instance(wm_id: f64) -> Value {
    json_value(&DelveGridWidget {
        base: Widget {
            wm_id,
            wm_type: "delve-grid".to_owned(),
            wm_title: String::new(),
            wm_wants: Wants::Hide,
            wm_zorder: None,
            wm_flags: vec!["hide-on-focus".to_owned(), "menu::skip".to_owned()],
        },
        toggle_key: None,
    })
}

/// `WidgetLibrary.vue`'s `initInstance()`.
fn library_instance(wm_id: f64) -> Value {
    let chaos = ColumnOpts {
        ref_name: true,
        item_level: true,
        rarity: true,
        sockets: true,
        mods: true,
        added_mods: true,
        removed_mods: true,
        keep: ColumnKeep { explicit: true, implicit: false, enchant: false, augment: false },
        mod_opts: ColumnModOpts { name: true, tier: true, roll: true, stat_ref: true, kind: true, generation: true },
    };
    json_value(&LibraryWidget {
        base: Widget {
            wm_id,
            wm_type: "library".to_owned(),
            wm_title: "{icon=fa-book}".to_owned(),
            wm_wants: Wants::Hide,
            wm_zorder: None,
            wm_flags: vec!["invisible-on-blur".to_owned(), "menu::skip".to_owned()],
        },
        anchor: Anchor { pos: "tl".to_owned(), x: 20.0, y: 20.0 },
        log_item_key: None,
        library_output_path: None,
        selected_profile: "chaos".to_owned(),
        profiles: IndexMap::from_iter([("chaos".to_owned(), chaos)]),
    })
}

/// The widgets of `defaultConfig()`, in the registry's order
/// (`overlay/widget-registry.ts`), with the ids `defaultConfig()` gives them
/// (`wmId = widgets.length + 1`).
fn default_widgets() -> Vec<Value> {
    let instances: [fn(f64) -> Value; 11] = [
        menu_instance,
        settings_instance,
        item_search_instance,
        experience_tracker_instance,
        map_rolling_instance,
        dump_sorting_instance,
        price_check_instance,
        item_check_instance,
        image_strip_instance,
        delve_grid_instance,
        library_instance,
    ];
    instances.iter().enumerate().map(|(index, instance)| instance(1.0 + usize_to_f64(index))).collect()
}

/// `defaultConfig().commands`.
fn default_commands() -> Vec<Command> {
    [
        ("/hideout", Some("F5")),
        ("/exit", Some("F9")),
        ("@last ty", None),
        ("/invite @last", None),
        ("/tradewith @last", None),
        ("/hideout @last", None),
    ]
    .into_iter()
    .map(|(text, hotkey)| Command { text: text.to_owned(), hotkey: hotkey.map(str::to_owned), send: true })
    .collect()
}

/// `defaultConfig()`
pub fn default_config() -> Value {
    json_value(&Config {
        config_version: f64::from(CONFIG_VERSION),
        league_id: None,
        overlay_key: "Shift + Space".to_owned(),
        overlay_background: "rgba(129, 139, 149, 0.15)".to_owned(),
        overlay_background_close: true,
        overlay_always_close: false,
        hide_overlay_on_blur: false,
        restore_clipboard: false,
        show_attach_notification: true,
        commands: default_commands(),
        client_log: None,
        game_config: None,
        window_title: "Path of Exile 2".to_owned(),
        log_keys: false,
        account_name: String::new(),
        stash_scroll: true,
        language: "en".to_owned(),
        preferred_trade_site: "default".to_owned(),
        realm: "pc-ggg".to_owned(),
        font_size: 16.0,
        widgets: default_widgets(),
        enable_alphas: false,
        alphas: Vec::new(),
        tips_frequency: 3.0,
        read_client_log: false,
    })
}

/// A default widget of `wm_type` (`defaultConfig().widgets.find(...)!`).
///
/// Every `wmType` the upgrade steps add has a default instance, so the `null` fallback is
/// unreachable; upstream's `!` would throw there, this cannot panic inside the UI.
fn default_widget(wm_type: &str) -> Value {
    default_widgets().into_iter().find(|w| widget_type(w) == Some(wm_type)).unwrap_or(Value::Null)
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

/// `config.configVersion = n`
fn set_version(config: &mut Value, n: u32) -> R<()> {
    obj(config)?.insert("configVersion".to_owned(), Value::from(n));
    Ok(())
}

/// `config.widgets.find(w => w.wmType === type)!.wmId`
#[expect(
    clippy::disallowed_methods,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn widget_id(config: &mut Value, wm_type: &str) -> R<f64> {
    Ok(widget(config, wm_type)?.get("wmId").and_then(Value::as_f64).unwrap_or(f64::NAN))
}

/// `config.widgets.splice(index, 0, widget)`: `index` is resolved like JS
/// (`NaN` is 0, a negative counts from the end, past the end is the length).
fn splice_widget(config: &mut Value, index: f64, widget: Value) -> R<()> {
    let ws = widgets(config)?;
    let len = usize_to_f64(ws.len());
    let index = if index.is_nan() { 0.0 } else { index.trunc() };
    let at = if index < 0.0 { (len + index).max(0.0) } else { index.min(len) };
    ws.insert(f64_to_usize(at), widget);
    Ok(())
}

/// `upgradeConfig`: brings an older config up to [`CONFIG_VERSION`].
pub fn upgrade_config(mut config: Value) -> R<Value> {
    let c = &mut config;

    upgrade_to_16(c, &default_widget)?;

    if version(c) < 17.0 {
        to_17(c)?;
    }
    if version(c) < 18.0 {
        to_18(c)?;
    }
    if version(c) < 19.0 {
        to_19(c)?;
    }
    if version(c) < 21.0 {
        to_21(c)?;
    }
    // Upstream runs these three every time, between its `< 21` and `< 22` blocks.
    default_remember_currency(c)?;
    default_stash_hotkeys(c)?;
    default_log_keys(c)?;
    if version(c) < 22.0 {
        to_22(c)?;
    }
    if version(c) < 23.0 {
        to_23(c)?;
    }
    if version(c) < 24.0 {
        to_24(c)?;
    }
    if version(c) < 25.0 {
        to_25(c)?;
    }
    if version(c) < 26.0 {
        to_26(c)?;
    }
    if version(c) < 27.0 {
        to_27(c)?;
    }
    if version(c) < 28.0 {
        to_28(c)?;
    }
    if version(c) < 29.0 {
        to_29(c)?;
    }
    if version(c) < 30.0 {
        to_30(c)?;
    }
    if version(c) < 31.0 {
        to_31(c)?;
    }
    if version(c) < 32.0 {
        to_32(c)?;
    }
    if version(c) < 33.0 {
        to_33(c)?;
    }
    if version(c) < 35.0 {
        to_35(c)?;
    }

    Ok(config)
}

/// `if (config.configVersion < 17)`: `config.preferredTradeSite = config.language` is
/// commented out upstream.
fn to_17(config: &mut Value) -> R<()> {
    set_version(config, 17)
}

/// `if (config.configVersion < 18)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_18(config: &mut Value) -> R<()> {
    config["enableAlphas"] = json!(false);
    config["alphas"] = json!([]);
    config["preferredTradeSite"] = json!("default");
    set_version(config, 18)
}

/// `if (config.configVersion < 19)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_19(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("defaultAllSelected".into(), json!(false));
    set_version(config, 19)
}

/// `if (config.configVersion < 21)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_21(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("itemHoverTooltip".into(), json!("keybind"));
    config["alphas"] = json!([]);
    set_version(config, 21)
}

/// `priceCheck.rememberCurrency = false` if it is `undefined`: a `null` stays.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn default_remember_currency(config: &mut Value) -> R<()> {
    let pc = widget(config, "price-check")?;
    if !pc.contains_key("rememberCurrency") {
        pc.insert("rememberCurrency".into(), json!(false));
    }
    Ok(())
}

/// `stashSearch.enableHotkeys ??= true`: also replaces `null`.
#[expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn default_stash_hotkeys(config: &mut Value) -> R<()> {
    for w in widgets(config)?.iter_mut().filter(|w| w["wmType"] == "stash-search") {
        if w.get("enableHotkeys").is_none_or(Value::is_null) {
            w["enableHotkeys"] = json!(true);
        }
    }
    Ok(())
}

/// `config.logKeys = false` if it is `undefined`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn default_log_keys(config: &mut Value) -> R<()> {
    let o = obj(config)?;
    if !o.contains_key("logKeys") {
        o.insert("logKeys".into(), json!(false));
    }
    Ok(())
}

/// `if (config.configVersion < 22)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_22(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("autoFillEmptyRuneSockets".into(), json!(false));
    config["tipsFrequency"] = json!(3);
    set_version(config, 22)
}

/// `if (config.configVersion < 23)`.
#[expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_23(config: &mut Value) -> R<()> {
    for w in widgets(config)?.iter_mut() {
        for flag in w.get_mut("wmFlags").and_then(Value::as_array_mut).into_iter().flatten() {
            if flag == "skip-menu" {
                *flag = json!("menu::skip");
            }
        }
        if w["wmType"] == "settings"
            && let Some(flags) = w.get_mut("wmFlags").and_then(Value::as_array_mut)
        {
            flags.retain(|flag| !flag.as_str().is_some_and(|f| f.starts_with("settings::")));
        }
    }
    widget(config, "item-search")?.insert("wmTitle".into(), json!("{icon=fa-search}"));
    widget(config, "settings")?.insert("wmTitle".into(), json!("{icon=fa-cog}"));
    // The settings icon first in the widget menu. Stable, like `Array.prototype.sort`.
    widgets(config)?.sort_by_key(|w| w["wmType"] != "settings");
    set_version(config, 23)
}

/// `if (config.configVersion < 24)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_24(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("alwaysShowTier".into(), json!(false));
    set_version(config, 24)
}

/// `if (config.configVersion < 25)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_25(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("openItemEditorAbove".into(), json!(false));
    set_version(config, 25)
}

/// `if (config.configVersion < 26)`: drops the item filter generator.
fn to_26(config: &mut Value) -> R<()> {
    widgets(config)?.retain(|w| w["wmType"] != "filter-generator");
    set_version(config, 26)
}

/// `if (config.configVersion < 27)`.
fn to_27(config: &mut Value) -> R<()> {
    widget(config, "item-check")?.insert("samePricedKey".into(), Value::Null);
    set_version(config, 27)
}

/// `if (config.configVersion < 28)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_28(config: &mut Value) -> R<()> {
    widget(config, "item-check")?.insert("samePricedType".into(), json!("both"));
    set_version(config, 28)
}

/// `if (config.configVersion < 29)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_29(config: &mut Value) -> R<()> {
    // The item-search widget's `wmId` doubles as the splice index.
    let item_search_id = widget_id(config, "item-search")?;
    let tracker = new_widget(config, &default_widget, "experience-tracker", &[])?;
    splice_widget(config, item_search_id, tracker)?;
    config["tipsFrequency"] = json!(6);
    let pc = widget(config, "price-check")?;
    pc.insert("coreCurrency".into(), json!("exalted"));
    pc.insert("currencyVolume".into(), json!("both"));
    config["readClientLog"] = json!(false);
    set_version(config, 29)
}

/// `if (config.configVersion < 30)`.
fn to_30(config: &mut Value) -> R<()> {
    let item_search_id = widget_id(config, "item-search")?;
    let library = new_widget(config, &default_widget, "library", &[])?;
    splice_widget(config, item_search_id, library)?;
    set_version(config, 30)
}

/// `if (config.configVersion < 31)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_31(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("rememberListingType".into(), json!(false));
    set_version(config, 31)
}

/// `if (config.configVersion < 32)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_32(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("initialDelay".into(), json!(48));
    set_version(config, 32)
}

/// `if (config.configVersion < 33)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_33(config: &mut Value) -> R<()> {
    {
        let library = widget(config, "library")?;
        library.insert("selectedProfile".into(), json!("chaos"));
        let profiles = obj(library.get_mut("profiles").ok_or_else(|| ConfigError("no profiles".into()))?)?;
        let chaos = obj(profiles.get_mut("chaos").ok_or_else(|| ConfigError("no chaos profile".into()))?)?;
        let mod_opts = obj(chaos.get_mut("modOpts").ok_or_else(|| ConfigError("no modOpts".into()))?)?;
        mod_opts.insert("generation".into(), json!(true));
    }
    // Upstream deliberately jumps past 33.
    set_version(config, 34)
}

/// `if (config.configVersion < 35)`.
#[expect(
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]
fn to_35(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("savedAugments".into(), json!({}));
    config["hideOverlayOnBlur"] = json!(false);
    set_version(config, 35)
}

/// `getConfigForHost`: the global shortcuts and what the host process needs.
pub fn host_config(config: &Value) -> R<Value> {
    let view: HostView = serde_json::from_value(config.clone()).map_err(|e| ConfigError(e.to_string()))?;
    let mut actions = host_widget_shortcuts(&view)?;
    actions.extend(command_shortcuts(&view));
    actions.extend(configured_widget_shortcuts(&view));

    Ok(json_value(&HostConfig {
        shortcuts: actions,
        restore_clipboard: view.restore_clipboard.clone(),
        client_log: view.client_log.clone(),
        game_config: view.game_config.clone(),
        stash_scroll: view.stash_scroll.clone(),
        overlay_key: view.overlay_key.clone(),
        log_keys: view.log_keys.clone(),
        window_title: view.window_title.clone(),
        language: view.language.clone(),
        read_client_log: view.read_client_log.clone(),
        library_alpha: library_alpha(&view)?,
        library_output_path: view.widget("library", HostWidget::library)?.library_output_path.clone(),
        initial_delay: view.widget("price-check", HostWidget::price_check)?.initial_delay.clone(),
        hide_overlay_on_blur: view.hide_overlay_on_blur.clone(),
    }))
}

/// The price-check, overlay, item-check, library and delve-grid hotkeys, in upstream's order.
fn host_widget_shortcuts(view: &HostView) -> R<Vec<Value>> {
    let mut actions = Vec::new();

    let pc = view.widget("price-check", HostWidget::price_check)?;
    if truthy(pc.hotkey.as_ref()) {
        let hotkey = format!("{} + {}", text(pc.hotkey_hold.as_ref()), text(pc.hotkey.as_ref()));
        actions.push(copy_item(hotkey, "price-check", Some(false), true));
    }
    if truthy(pc.hotkey_locked.as_ref()) {
        actions.push(copy_item(text(pc.hotkey_locked.as_ref()), "price-check", Some(true), false));
    }
    // Unlike PoE1, the overlay toggle does not keep the modifier keys.
    actions.push(shortcut(text(view.overlay_key.as_ref()), json_value(&Action::ToggleOverlay), false));

    let ic = view.widget("item-check", HostWidget::item_check)?;
    for (key, target) in [
        (&ic.wiki_key, "open-wiki"),
        (&ic.craft_of_exile_key, "open-craft-of-exile"),
        (&ic.poedb_key, "open-poedb"),
        (&ic.stash_search_key, "search-similar"),
        (&ic.same_priced_key, "search-same-priced"),
    ] {
        if truthy(key.as_ref()) {
            actions.push(copy_item(text(key.as_ref()), target, None, false));
        }
    }
    if truthy(ic.hotkey.as_ref()) {
        actions.push(copy_item(text(ic.hotkey.as_ref()), "item-check", Some(true), false));
    }

    let library = view.widget("library", HostWidget::library)?;
    if truthy(library.log_item_key.as_ref()) {
        actions.push(copy_item(text(library.log_item_key.as_ref()), "log-item", None, true));
    }

    let delve = view.widget("delve-grid", HostWidget::delve_grid)?;
    if truthy(delve.toggle_key.as_ref()) {
        actions.push(shortcut(
            text(delve.toggle_key.as_ref()),
            json_value(&Action::TriggerEvent { target: "delve-grid".to_owned() }),
            true,
        ));
    }

    Ok(actions)
}

/// The chat commands' hotkeys.
fn command_shortcuts(view: &HostView) -> Vec<Value> {
    view.commands
        .iter()
        .filter(|command| truthy(command.hotkey.as_ref()))
        .map(|command| {
            shortcut(
                text(command.hotkey.as_ref()),
                json_value(&Action::PasteInChat { text: command.text.clone(), send: command.send.clone() }),
                false,
            )
        })
        .collect()
}

/// The stash-search, timer and item-search widgets' own hotkeys.
fn configured_widget_shortcuts(view: &HostView) -> Vec<Value> {
    let mut actions = Vec::new();
    for widget in &view.widgets {
        match widget {
            HostWidget::StashSearch(stash) => {
                if !truthy(stash.enable_hotkeys.as_ref()) {
                    continue;
                }
                for entry in &stash.entries {
                    if truthy(entry.hotkey.as_ref()) {
                        actions.push(shortcut(
                            text(entry.hotkey.as_ref()),
                            json_value(&Action::StashSearch { text: entry.text.clone() }),
                            false,
                        ));
                    }
                }
            }
            HostWidget::Timer(timer) => {
                let id = text(timer.wm_id.as_ref());
                for (key, event) in [(&timer.toggle_key, "stopwatch-start-stop"), (&timer.reset_key, "stopwatch-reset")]
                {
                    if truthy(key.as_ref()) {
                        actions.push(shortcut(
                            text(key.as_ref()),
                            json_value(&Action::TriggerEvent { target: format!("{event}:{id}") }),
                            true,
                        ));
                    }
                }
            }
            HostWidget::ItemSearch(search) if truthy(search.ocr_gems_key.as_ref()) => {
                actions.push(shortcut(
                    text(search.ocr_gems_key.as_ref()),
                    json_value(&Action::OcrText { target: "heist-gems".to_owned() }),
                    true,
                ));
            }
            _ => {}
        }
    }
    actions
}

/// `config.enableAlphas && config.alphas.includes("library")`: a falsy `enableAlphas` is
/// returned as is, and a config without one has no `libraryAlpha` at all.
fn library_alpha(view: &HostView) -> R<Option<Value>> {
    let Some(enable) = view.enable_alphas.clone() else { return Ok(None) };
    Ok(Some(if truthy(Some(&enable)) {
        let Value::Array(alphas) = view.alphas.clone().unwrap_or(Value::Null) else {
            return Err(ConfigError("Cannot read properties of undefined (reading 'includes')".to_owned()));
        };
        Value::Bool(alphas.iter().any(|a| a == "library"))
    } else {
        enable
    }))
}

/// `{ type: "copy-item", target, focusOverlay }`.
fn copy_item(shortcut_key: String, target: &str, focus_overlay: Option<bool>, keep_mod_keys: bool) -> Value {
    shortcut(shortcut_key, json_value(&Action::CopyItem { target: target.to_owned(), focus_overlay }), keep_mod_keys)
}

/// `Cannot read properties of undefined ({wm_type})`: upstream's error when the config has no
/// widget of `wm_type`.
fn missing_widget(wm_type: &str) -> ConfigError {
    ConfigError(format!("Cannot read properties of undefined ({wm_type})"))
}

/// Reads a field that may be `null`: `None` only when the key is absent, so a `null` value
/// survives the round trip (upstream reads `undefined` and `null` differently here).
fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(d).map(Some)
}

/// A config list: anything that is not an array reads as empty and an element of the wrong
/// shape is skipped, as the old `Value::as_array().into_iter().flatten()` reads did.
fn list<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error> {
    match Value::deserialize(d)? {
        Value::Array(items) => Ok(items.into_iter().filter_map(|item| serde_json::from_value(item).ok()).collect()),
        _ => Ok(Vec::new()),
    }
}

/// What `getConfigForHost()` reads off the config file: `Config`'s fields it needs, kept
/// leniently (a hand-edited value of the wrong type must not drop the rest).
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostView {
    #[serde(deserialize_with = "present")]
    overlay_key: Option<Value>,
    #[serde(deserialize_with = "present")]
    restore_clipboard: Option<Value>,
    #[serde(deserialize_with = "present")]
    client_log: Option<Value>,
    #[serde(deserialize_with = "present")]
    game_config: Option<Value>,
    #[serde(deserialize_with = "present")]
    stash_scroll: Option<Value>,
    #[serde(deserialize_with = "present")]
    log_keys: Option<Value>,
    #[serde(deserialize_with = "present")]
    window_title: Option<Value>,
    #[serde(deserialize_with = "present")]
    language: Option<Value>,
    #[serde(deserialize_with = "present")]
    read_client_log: Option<Value>,
    #[serde(deserialize_with = "present")]
    enable_alphas: Option<Value>,
    #[serde(deserialize_with = "present")]
    alphas: Option<Value>,
    #[serde(deserialize_with = "present")]
    hide_overlay_on_blur: Option<Value>,
    #[serde(deserialize_with = "list")]
    commands: Vec<HostCommand>,
    #[serde(deserialize_with = "list")]
    widgets: Vec<HostWidget>,
}

impl HostView {
    /// `AppConfig(type)`'s fields, or upstream's error when the config has no such widget.
    fn widget<'a, T>(&'a self, wm_type: &str, pick: impl Fn(&'a HostWidget) -> Option<&'a T>) -> R<&'a T> {
        self.widgets.iter().find_map(pick).ok_or_else(|| missing_widget(wm_type))
    }
}

/// An entry of `Config.commands`, as `getConfigForHost()` reads it: `text` and `send` go into
/// the action unchanged, so a hand-edited value of any type survives.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostCommand {
    text: Value,
    #[serde(deserialize_with = "present")]
    hotkey: Option<Value>,
    send: Value,
}

/// `AppConfig(type)` for `getConfigForHost`: the fields each branch reads. A widget whose
/// `wmType` is missing, unknown or not a string is skipped, as the old
/// `w["wmType"].as_str()` match skipped it.
#[derive(Debug, Deserialize)]
#[serde(tag = "wmType", rename_all = "kebab-case")]
enum HostWidget {
    PriceCheck(HostPriceCheck),
    ItemCheck(HostItemCheck),
    Library(HostLibrary),
    DelveGrid(HostDelveGrid),
    StashSearch(HostStashSearch),
    Timer(HostTimer),
    ItemSearch(HostItemSearch),
}

impl HostWidget {
    const fn price_check(&self) -> Option<&HostPriceCheck> {
        match self {
            Self::PriceCheck(widget) => Some(widget),
            _ => None,
        }
    }

    const fn item_check(&self) -> Option<&HostItemCheck> {
        match self {
            Self::ItemCheck(widget) => Some(widget),
            _ => None,
        }
    }

    const fn library(&self) -> Option<&HostLibrary> {
        match self {
            Self::Library(widget) => Some(widget),
            _ => None,
        }
    }

    const fn delve_grid(&self) -> Option<&HostDelveGrid> {
        match self {
            Self::DelveGrid(widget) => Some(widget),
            _ => None,
        }
    }
}

/// `PriceCheckWidget`'s hotkeys and initial delay.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostPriceCheck {
    #[serde(deserialize_with = "present")]
    hotkey: Option<Value>,
    #[serde(deserialize_with = "present")]
    hotkey_hold: Option<Value>,
    #[serde(deserialize_with = "present")]
    hotkey_locked: Option<Value>,
    #[serde(deserialize_with = "present")]
    initial_delay: Option<Value>,
}

/// `ItemCheckWidget`'s hotkeys.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostItemCheck {
    #[serde(deserialize_with = "present")]
    hotkey: Option<Value>,
    #[serde(deserialize_with = "present")]
    wiki_key: Option<Value>,
    #[serde(deserialize_with = "present")]
    poedb_key: Option<Value>,
    #[serde(deserialize_with = "present")]
    craft_of_exile_key: Option<Value>,
    #[serde(deserialize_with = "present")]
    stash_search_key: Option<Value>,
    #[serde(deserialize_with = "present")]
    same_priced_key: Option<Value>,
}

/// `LibraryWidget`'s hotkey and output path.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostLibrary {
    #[serde(deserialize_with = "present")]
    log_item_key: Option<Value>,
    #[serde(deserialize_with = "present")]
    library_output_path: Option<Value>,
}

/// `DelveGridWidget`'s toggle key.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostDelveGrid {
    #[serde(deserialize_with = "present")]
    toggle_key: Option<Value>,
}

/// `StashSearchWidget`'s enabled hotkeys and their entries.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostStashSearch {
    #[serde(deserialize_with = "present")]
    enable_hotkeys: Option<Value>,
    #[serde(deserialize_with = "list")]
    entries: Vec<HostStashEntry>,
}

/// One `StashSearchWidget.entries` entry as `getConfigForHost()` reads it.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostStashEntry {
    #[serde(deserialize_with = "present")]
    hotkey: Option<Value>,
    text: Value,
}

/// `StopwatchWidget`'s keys and id.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostTimer {
    #[serde(deserialize_with = "present")]
    wm_id: Option<Value>,
    #[serde(deserialize_with = "present")]
    toggle_key: Option<Value>,
    #[serde(deserialize_with = "present")]
    reset_key: Option<Value>,
}

/// `ItemSearchWidget`'s OCR key.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct HostItemSearch {
    #[serde(deserialize_with = "present")]
    ocr_gems_key: Option<Value>,
}

/// `HostConfig` (`@ipc/types.ts`): what `getConfigForHost()` sends the host process. Only the
/// fields the config has are sent.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct HostConfig {
    shortcuts: Vec<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    restore_clipboard: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    client_log: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    game_config: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stash_scroll: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    overlay_key: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_keys: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_title: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    language: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    read_client_log: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    library_alpha: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    library_output_path: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_delay: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hide_overlay_on_blur: Option<Value>,
}

/// `ShortcutAction.action` (`@ipc/types.ts`), serialised with its `type` tag in front.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
enum Action {
    CopyItem {
        target: String,
        /// Whether the widget takes the focus once the item text is there.
        #[serde(rename = "focusOverlay", skip_serializing_if = "Option::is_none")]
        focus_overlay: Option<bool>,
    },
    /// The command's fields, passed on as the config holds them.
    PasteInChat {
        text: Value,
        send: Value,
    },
    StashSearch {
        text: Value,
    },
    TriggerEvent {
        target: String,
    },
    OcrText {
        target: String,
    },
    ToggleOverlay,
}
