//! `ref/ee2/renderer/src/web/Config.ts`: the settings, as EE2 stores them.
//!
//! The config stays JSON ([`Value`]) so every field survives, including those
//! of widgets Wraex doesn't have yet. [`default_config`] is `defaultConfig()`
//! (each widget's `initInstance`/`defaultInstances`), [`load`] is
//! `initConfig()` (`upgradeConfig` included) and [`host_config`] is
//! `getConfigForHost()`. Writing `undefined` to a field removes it, as the
//! JSON round trip in `updateConfig` does.

use serde_json::{Value, json};

use wraex_core::config_upgrade::{R, new_widget, obj, shortcut, text, truthy, upgrade_to_16, version, widget, widgets};

pub use wraex_core::config_upgrade::ConfigError;

/// `configVersion` of the current format.
pub const CONFIG_VERSION: u32 = 35;

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
        // WidgetXpTracker.vue
        json!({
            "wmId": 0, "wmType": "experience-tracker", "wmTitle": "XP", "wmWants": "hide", "wmZorder": null,
            "wmFlags": [],
            "anchor": { "pos": "bc", "x": 68, "y": 98 },
            "showExp": true
        }),
        // WidgetTimer.vue: `multi`, no default instances.
        // WidgetStashSearch.vue
        json!({
            "wmId": 0, "wmType": "stash-search", "wmTitle": "Map rolling", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["invisible-on-blur"],
            "anchor": { "pos": "tl", "x": 35, "y": 46 },
            "enableHotkeys": true,
            "entries": [
                { "id": 1, "name": "", "text": "\"Increased Pack Size\"", "hotkey": null },
                { "id": 2, "name": "", "text": "\"Additional Packs\"", "hotkey": null },
                { "id": 3, "name": "", "text": "\"Burning Ground\"", "hotkey": null },
                { "id": 4, "name": "", "text": "\"Less Recovery Rate\"", "hotkey": null }
            ]
        }),
        json!({
            "wmId": 0, "wmType": "stash-search", "wmTitle": "Dump sorting", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["invisible-on-blur"],
            "anchor": { "pos": "tl", "x": 34, "y": 56 },
            "enableHotkeys": true,
            "entries": [
                { "id": 1, "name": "", "text": "Currency", "hotkey": null },
                { "id": 2, "name": "", "text": "\"Martial Weapon\"", "hotkey": null },
                { "id": 3, "name": "", "text": "Jewel", "hotkey": null },
                { "id": 4, "name": "", "text": "\"Waystone Tier\"", "hotkey": null },
                { "id": 5, "name": "", "text": "\"Rarity: Normal\"", "hotkey": null },
                { "id": 6, "name": "", "text": "Trials:", "hotkey": null }
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
            "rememberCurrency": false,
            // EE2's own settings.
            "defaultAllSelected": false,
            "itemHoverTooltip": "keybind",
            "alwaysShowTier": false,
            "coreCurrency": "exalted",
            "currencyVolume": "both",
            "rememberListingType": false,
            "initialDelay": 48,
            "savedAugments": {}
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
            "samePricedKey": null,
            "samePricedType": "both",
            "maps": {
                "profile": 1,
                "showNewStats": false,
                "selectedStats": [
                    { "matcher": "#% maximum Player Resistances", "decision": "w--" },
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
        // WidgetNotepad.vue: `multi`, no default instances.
        // WidgetLibrary.vue
        json!({
            "wmId": 0, "wmType": "library", "wmTitle": "{icon=fa-book}", "wmWants": "hide", "wmZorder": null,
            "wmFlags": ["invisible-on-blur", "menu::skip"],
            "anchor": { "pos": "tl", "x": 20, "y": 20 },
            "logItemKey": null,
            "libraryOutputPath": null,
            "selectedProfile": "chaos",
            "profiles": {
                "chaos": {
                    "refName": true,
                    "itemLevel": true,
                    "rarity": true,
                    "sockets": true,
                    "mods": true,
                    "addedMods": true,
                    "removedMods": true,
                    "keep": { "explicit": true, "implicit": false, "enchant": false, "augment": false },
                    "modOpts": { "name": true, "tier": true, "roll": true, "ref": true, "type": true, "generation": true }
                }
            }
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
        "overlayAlwaysClose": false,
        "hideOverlayOnBlur": false,
        "restoreClipboard": false,
        "showAttachNotification": true,
        "commands": commands,
        "clientLog": null,
        "gameConfig": null,
        "windowTitle": "Path of Exile 2",
        "logKeys": false,
        "accountName": "",
        "stashScroll": true,
        "language": "en",
        "preferredTradeSite": "default",
        "realm": "pc-ggg",
        "fontSize": 16,
        "widgets": widgets,
        "enableAlphas": false,
        "alphas": [],
        "tipsFrequency": 3,
        "readClientLog": false
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

/// `config.widgets.find(w => w.wmType === type)!.wmId`
fn widget_id(config: &mut Value, wm_type: &str) -> R<f64> {
    Ok(widget(config, wm_type)?.get("wmId").and_then(Value::as_f64).unwrap_or(f64::NAN))
}

/// `config.widgets.splice(index, 0, widget)`: `index` is resolved like JS
/// (`NaN` is 0, a negative counts from the end, past the end is the length).
fn splice_widget(config: &mut Value, index: f64, widget: Value) -> R<()> {
    let ws = widgets(config)?;
    let len = ws.len() as f64;
    let index = if index.is_nan() { 0.0 } else { index.trunc() };
    let at = if index < 0.0 { (len + index).max(0.0) } else { index.min(len) } as usize;
    ws.insert(at, widget);
    Ok(())
}

/// `upgradeConfig`: brings an older config up to [`CONFIG_VERSION`].
pub fn upgrade_config(mut config: Value) -> R<Value> {
    let c = &mut config;

    upgrade_to_16(c, &default_widget)?;

    if version(c) < 17.0 {
        // `config.preferredTradeSite = config.language` is commented out upstream.
        c["configVersion"] = json!(17);
    }

    if version(c) < 18.0 {
        c["enableAlphas"] = json!(false);
        c["alphas"] = json!([]);
        c["preferredTradeSite"] = json!("default");
        c["configVersion"] = json!(18);
    }

    if version(c) < 19.0 {
        widget(c, "price-check")?.insert("defaultAllSelected".into(), json!(false));
        c["configVersion"] = json!(19);
    }

    if version(c) < 21.0 {
        widget(c, "price-check")?.insert("itemHoverTooltip".into(), json!("keybind"));
        c["alphas"] = json!([]);
        c["configVersion"] = json!(21);
    }

    {
        let pc = widget(c, "price-check")?;
        // `=== undefined`: a `null` stays.
        if !pc.contains_key("rememberCurrency") {
            pc.insert("rememberCurrency".into(), json!(false));
        }
    }

    for w in widgets(c)?.iter_mut().filter(|w| w["wmType"] == "stash-search") {
        // `??=`: also replaces `null`.
        if w.get("enableHotkeys").is_none_or(Value::is_null) {
            w["enableHotkeys"] = json!(true);
        }
    }

    let o = obj(c)?;
    if !o.contains_key("logKeys") {
        o.insert("logKeys".into(), json!(false));
    }

    if version(c) < 22.0 {
        widget(c, "price-check")?.insert("autoFillEmptyRuneSockets".into(), json!(false));
        c["tipsFrequency"] = json!(3);
        c["configVersion"] = json!(22);
    }

    if version(c) < 23.0 {
        for w in widgets(c)?.iter_mut() {
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
        widget(c, "item-search")?.insert("wmTitle".into(), json!("{icon=fa-search}"));
        widget(c, "settings")?.insert("wmTitle".into(), json!("{icon=fa-cog}"));
        // The settings icon first in the widget menu. Stable, like `Array.prototype.sort`.
        widgets(c)?.sort_by_key(|w| w["wmType"] != "settings");
        c["configVersion"] = json!(23);
    }

    if version(c) < 24.0 {
        widget(c, "price-check")?.insert("alwaysShowTier".into(), json!(false));
        c["configVersion"] = json!(24);
    }

    if version(c) < 25.0 {
        widget(c, "price-check")?.insert("openItemEditorAbove".into(), json!(false));
        c["configVersion"] = json!(25);
    }

    if version(c) < 26.0 {
        widgets(c)?.retain(|w| w["wmType"] != "filter-generator");
        c["configVersion"] = json!(26);
    }

    if version(c) < 27.0 {
        widget(c, "item-check")?.insert("samePricedKey".into(), Value::Null);
        c["configVersion"] = json!(27);
    }

    if version(c) < 28.0 {
        widget(c, "item-check")?.insert("samePricedType".into(), json!("both"));
        c["configVersion"] = json!(28);
    }

    if version(c) < 29.0 {
        // The item-search widget's `wmId` doubles as the splice index.
        let item_search_id = widget_id(c, "item-search")?;
        let tracker = new_widget(c, &default_widget, "experience-tracker", &[])?;
        splice_widget(c, item_search_id, tracker)?;
        c["tipsFrequency"] = json!(6);
        let pc = widget(c, "price-check")?;
        pc.insert("coreCurrency".into(), json!("exalted"));
        pc.insert("currencyVolume".into(), json!("both"));
        c["readClientLog"] = json!(false);
        c["configVersion"] = json!(29);
    }

    if version(c) < 30.0 {
        let item_search_id = widget_id(c, "item-search")?;
        let library = new_widget(c, &default_widget, "library", &[])?;
        splice_widget(c, item_search_id, library)?;
        c["configVersion"] = json!(30);
    }

    if version(c) < 31.0 {
        widget(c, "price-check")?.insert("rememberListingType".into(), json!(false));
        c["configVersion"] = json!(31);
    }

    if version(c) < 32.0 {
        widget(c, "price-check")?.insert("initialDelay".into(), json!(48));
        c["configVersion"] = json!(32);
    }

    if version(c) < 33.0 {
        {
            let library = widget(c, "library")?;
            library.insert("selectedProfile".into(), json!("chaos"));
            let profiles = obj(library.get_mut("profiles").ok_or_else(|| ConfigError("no profiles".into()))?)?;
            let chaos = obj(profiles.get_mut("chaos").ok_or_else(|| ConfigError("no chaos profile".into()))?)?;
            let mod_opts = obj(chaos.get_mut("modOpts").ok_or_else(|| ConfigError("no modOpts".into()))?)?;
            mod_opts.insert("generation".into(), json!(true));
        }
        // Upstream deliberately jumps past 33.
        c["configVersion"] = json!(34);
    }

    if version(c) < 35.0 {
        widget(c, "price-check")?.insert("savedAugments".into(), json!({}));
        c["hideOverlayOnBlur"] = json!(false);
        c["configVersion"] = json!(35);
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
    // Unlike PoE1, the overlay toggle does not keep the modifier keys.
    actions.push(shortcut(text(config.get("overlayKey")), json!({ "type": "toggle-overlay" }), false));

    let ic = find("item-check")?;
    for (key, target) in [
        ("wikiKey", "open-wiki"),
        ("craftOfExileKey", "open-craft-of-exile"),
        ("poedbKey", "open-poedb"),
        ("stashSearchKey", "search-similar"),
        ("samePricedKey", "search-same-priced"),
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

    let library = find("library")?;
    if truthy(library.get("logItemKey")) {
        actions.push(shortcut(
            text(library.get("logItemKey")),
            json!({ "type": "copy-item", "target": "log-item" }),
            true,
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
        "readClientLog",
        "hideOverlayOnBlur",
    ] {
        if let Some(v) = config.get(key) {
            host[key] = v.clone();
        }
    }
    // `config.enableAlphas && config.alphas.includes("library")`: a falsy
    // `enableAlphas` is returned as is.
    if let Some(enable) = config.get("enableAlphas") {
        let alpha = if truthy(Some(enable)) {
            let alphas = config
                .get("alphas")
                .and_then(Value::as_array)
                .ok_or_else(|| ConfigError("Cannot read properties of undefined (reading 'includes')".into()))?;
            Value::Bool(alphas.iter().any(|a| a == "library"))
        } else {
            enable.clone()
        };
        host["libraryAlpha"] = alpha;
    }
    let pc = find("price-check")?;
    for (key, value) in
        [("initialDelay", pc.get("initialDelay")), ("libraryOutputPath", library.get("libraryOutputPath"))]
    {
        if let Some(v) = value {
            host[key] = v.clone();
        }
    }
    Ok(host)
}
