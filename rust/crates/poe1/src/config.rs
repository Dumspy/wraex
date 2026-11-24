//! `ref/apt/renderer/src/web/Config.ts`: the settings, as upstream stores them.
//!
//! The config stays JSON ([`Value`]) so every field survives, including those
//! of widgets Wraex doesn't have yet. [`default_config`] is `defaultConfig()`
//! (each widget's `initInstance`/`defaultInstances`), [`load`] is
//! `initConfig()` (`upgradeConfig` included) and [`host_config`] is
//! `getConfigForHost()`. Writing `undefined` to a field removes it, as the
//! JSON round trip in `updateConfig` does.

use serde_json::{Value, json};

use wraex_core::config_upgrade::{R, obj, shortcut, text, truthy, upgrade_to_16, version, widget, widgets};

pub use wraex_core::config_upgrade::ConfigError;

/// `configVersion` of the current format.
pub const CONFIG_VERSION: u32 = 18;

/// The widgets of `defaultConfig()`, in the registry's order
/// (`overlay/widget-registry.ts`), without their ids.
fn default_widgets() -> Vec<Value> {
    vec![
        // WidgetMenu.vue
        json!({
            "wmId": 0, "wmType": "menu", "wmTitle": "", "wmWants": "show", "wmZorder": 1,
            "wmFlags": ["invisible-on-blur", "menu::skip"],
            "anchor": { "pos": "tl", "x": 5, "y": 5 },
            "alwaysShow": false
        }),
        // SettingsWindow.vue
        json!({
            "wmId": 0, "wmType": "settings", "wmTitle": "{icon=fa-cog}", "wmWants": "hide", "wmZorder": "exclusive",
            "wmFlags": ["invisible-on-blur", "ignore-ui-visibility"]
        }),
        // WidgetItemSearch.vue
        json!({
            "wmId": 0, "wmType": "item-search", "wmTitle": "{icon=fa-search}", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["invisible-on-blur"],
            "anchor": { "pos": "tl", "x": 10, "y": 20 },
            "ocrGemsKey": null
        }),
        // WidgetTimer.vue: `multi`, no default instances.
        // WidgetStashSearch.vue
        json!({
            "wmId": 0, "wmType": "stash-search", "wmTitle": "Map rolling", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["invisible-on-blur"],
            "anchor": { "pos": "tl", "x": 35, "y": 46 },
            "enableHotkeys": true,
            "entries": [
                { "id": 1, "name": "", "text": "\"Pack Size: +3\"", "hotkey": null },
                { "id": 2, "name": "", "text": "Reflect", "hotkey": null },
                { "id": 3, "name": "", "text": "\"Cannot Leech Life\"", "hotkey": null },
                { "id": 4, "name": "", "text": "\"Cannot Leech Mana\"", "hotkey": null }
            ]
        }),
        json!({
            "wmId": 0, "wmType": "stash-search", "wmTitle": "Dump sorting", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["invisible-on-blur"],
            "anchor": { "pos": "tl", "x": 34, "y": 56 },
            "enableHotkeys": true,
            "entries": [
                { "id": 1, "name": "", "text": "Currency", "hotkey": null },
                { "id": 2, "name": "", "text": "\"Divination Card\"", "hotkey": null },
                { "id": 3, "name": "", "text": "Fossil", "hotkey": null },
                { "id": 4, "name": "", "text": "\"Map Tier\"", "hotkey": null },
                { "id": 5, "name": "", "text": "\"Map Device\" \"Rarity: Normal\"", "hotkey": null },
                { "id": 6, "name": "", "text": "Tane Laboratory", "hotkey": null }
            ]
        }),
        // PriceCheckWindow.vue
        json!({
            "wmId": 0, "wmType": "price-check", "wmTitle": "", "wmWants": "hide", "wmZorder": "exclusive",
            "wmFlags": ["hide-on-blur", "menu::skip"],
            "showRateLimitState": false,
            "apiLatencySeconds": 2,
            "collapseListings": "api",
            "smartInitialSearch": true,
            "lockedInitialSearch": true,
            "activateStockFilter": false,
            "builtinBrowser": false,
            "hotkey": "D",
            "hotkeyHold": "Ctrl",
            "hotkeyLocked": "Ctrl + Alt + D",
            "showSeller": false,
            "searchStatRange": 10,
            "showCursor": true,
            "requestPricePrediction": false,
            "merchantOnly": true,
            "defaultCurrency": null
        }),
        // WidgetItemCheck.vue
        json!({
            "wmId": 0, "wmType": "item-check", "wmTitle": "", "wmWants": "hide", "wmZorder": "exclusive",
            "wmFlags": ["hide-on-blur", "menu::skip"],
            "hotkey": null,
            "wikiKey": null,
            "poedbKey": null,
            "craftOfExileKey": null,
            "stashSearchKey": null,
            "maps": {
                "profile": 1,
                "showNewStats": false,
                "selectedStats": [
                    { "matcher": "Players have #% to all maximum Resistances", "decision": "w--" },
                    { "matcher": "Monsters reflect #% of Physical Damage", "decision": "d--" },
                    { "matcher": "Monsters reflect #% of Elemental Damage", "decision": "d--" },
                    { "matcher": "Area contains two Unique Bosses", "decision": "g--" }
                ]
            }
        }),
        // WidgetImageStrip.vue
        json!({
            "wmId": 0, "wmType": "image-strip", "wmTitle": "Cheat sheets", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["invisible-on-blur"],
            "anchor": { "pos": "tc", "x": 50, "y": 10 },
            "images": [{ "id": 1, "url": "syndicate.jpg" }]
        }),
        // WidgetDelveGrid.vue
        json!({
            "wmId": 0, "wmType": "delve-grid", "wmTitle": "", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["hide-on-focus", "menu::skip"],
            "toggleKey": null
        }),
    ]
}

/// `defaultConfig()`
pub fn default_config() -> Value {
    let commands = [
        ("/hideout", Some("F5")),
        ("/exit", Some("F9")),
        ("@last ty", None),
        ("/invite @last", None),
        ("/tradewith @last", None),
        ("/hideout @last", None),
    ]
    .map(|(text, hotkey)| json!({ "text": text, "hotkey": hotkey, "send": true }));
    let widgets: Vec<Value> = default_widgets()
        .into_iter()
        .enumerate()
        .map(|(i, mut w)| {
            w["wmId"] = json!(i + 1);
            w
        })
        .collect();
    json!({
        "configVersion": CONFIG_VERSION,
        "overlayKey": "Shift + Space",
        "overlayBackground": "rgba(129, 139, 149, 0.15)",
        "overlayBackgroundClose": true,
        "restoreClipboard": false,
        "showAttachNotification": true,
        "commands": commands,
        "clientLog": null,
        "gameConfig": null,
        "windowTitle": "Path of Exile",
        "logKeys": false,
        "accountName": "",
        "stashScroll": true,
        "language": "en",
        "realm": "pc-ggg",
        "useIntlSite": false,
        "fontSize": 16,
        "widgets": widgets
    })
}

/// A default widget of `wm_type` (`defaultConfig().widgets.find(...)!`).
fn default_widget(wm_type: &str) -> Value {
    default_config()["widgets"]
        .as_array()
        .and_then(|ws| ws.iter().find(|w| w["wmType"] == wm_type))
        .cloned()
        .expect("every widget type upgrades add has a default")
}

/// What `initConfig` ends with.
#[derive(Debug, Clone, PartialEq)]
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
    let c = &mut config;

    upgrade_to_16(c, &default_widget)?;

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

    if version(c) < 17.0 {
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
    }

    if version(c) < 18.0 {
        let intl = c.get("language") == Some(&json!("cmn-Hant")) && c.get("realm") == Some(&json!("pc-ggg"));
        c["useIntlSite"] = json!(intl);
        c["configVersion"] = json!(18);
    }

    Ok(config)
}

/// `getConfigForHost`: the global shortcuts and what the host process needs.
pub fn host_config(config: &Value) -> R<Value> {
    let find = |t: &str| -> R<&Value> {
        config["widgets"]
            .as_array()
            .and_then(|ws| ws.iter().find(|w| w["wmType"] == t))
            .ok_or_else(|| ConfigError(format!("Cannot read properties of undefined ({t})")))
    };
    let mut actions = Vec::new();

    let pc = find("price-check")?;
    if truthy(pc.get("hotkey")) {
        actions.push(shortcut(
            format!("{} + {}", text(pc.get("hotkeyHold")), text(pc.get("hotkey"))),
            json!({ "type": "copy-item", "target": "price-check", "focusOverlay": false }),
            true,
        ));
    }
    if truthy(pc.get("hotkeyLocked")) {
        actions.push(shortcut(
            text(pc.get("hotkeyLocked")),
            json!({ "type": "copy-item", "target": "price-check", "focusOverlay": true }),
            false,
        ));
    }
    actions.push(shortcut(text(config.get("overlayKey")), json!({ "type": "toggle-overlay" }), true));

    let ic = find("item-check")?;
    for (key, target) in [
        ("wikiKey", "open-wiki"),
        ("craftOfExileKey", "open-craft-of-exile"),
        ("poedbKey", "open-poedb"),
        ("stashSearchKey", "search-similar"),
    ] {
        if truthy(ic.get(key)) {
            actions.push(shortcut(text(ic.get(key)), json!({ "type": "copy-item", "target": target }), false));
        }
    }
    if truthy(ic.get("hotkey")) {
        actions.push(shortcut(
            text(ic.get("hotkey")),
            json!({ "type": "copy-item", "target": "item-check", "focusOverlay": true }),
            false,
        ));
    }

    let delve = find("delve-grid")?;
    if truthy(delve.get("toggleKey")) {
        actions.push(shortcut(
            text(delve.get("toggleKey")),
            json!({ "type": "trigger-event", "target": "delve-grid" }),
            true,
        ));
    }

    for command in config["commands"].as_array().into_iter().flatten() {
        if truthy(command.get("hotkey")) {
            actions.push(shortcut(
                text(command.get("hotkey")),
                json!({ "type": "paste-in-chat", "text": command.get("text"), "send": command.get("send") }),
                false,
            ));
        }
    }

    for w in config["widgets"].as_array().into_iter().flatten() {
        match w["wmType"].as_str() {
            Some("stash-search") => {
                if !truthy(w.get("enableHotkeys")) {
                    continue;
                }
                for entry in w["entries"].as_array().into_iter().flatten() {
                    if truthy(entry.get("hotkey")) {
                        actions.push(shortcut(
                            text(entry.get("hotkey")),
                            json!({ "type": "stash-search", "text": entry.get("text") }),
                            false,
                        ));
                    }
                }
            }
            Some("timer") => {
                let id = text(w.get("wmId"));
                for (key, event) in [("toggleKey", "stopwatch-start-stop"), ("resetKey", "stopwatch-reset")] {
                    if truthy(w.get(key)) {
                        actions.push(shortcut(
                            text(w.get(key)),
                            json!({ "type": "trigger-event", "target": format!("{event}:{id}") }),
                            true,
                        ));
                    }
                }
            }
            Some("item-search") if truthy(w.get("ocrGemsKey")) => {
                actions.push(shortcut(
                    text(w.get("ocrGemsKey")),
                    json!({ "type": "ocr-text", "target": "heist-gems" }),
                    true,
                ));
            }
            _ => {}
        }
    }

    let mut host = json!({ "shortcuts": actions });
    for key in [
        "restoreClipboard",
        "clientLog",
        "gameConfig",
        "stashScroll",
        "overlayKey",
        "logKeys",
        "windowTitle",
        "language",
    ] {
        if let Some(v) = config.get(key) {
            host[key] = v.clone();
        }
    }
    Ok(host)
}
