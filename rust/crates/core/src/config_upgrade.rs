//! `upgradeConfig` steps 3–16 and the helpers they share.
//!
//! Identical in both references (EE2 forked at version 16): the config stays
//! JSON ([`Value`]) so every field survives, and writing `undefined` to a field
//! removes it, as the JSON round trip in `updateConfig` does. The only per-game
//! part is [`new_widget`]'s default widget, passed in by each game crate.

use serde_json::{Map, Value, json};

use crate::js::{json_value, number_to_string};

/// A migration upstream can't run (a widget it expects is missing): `initConfig`
/// throws there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

pub type R<T> = Result<T, ConfigError>;

pub fn obj(v: &mut Value) -> R<&mut Map<String, Value>> {
    v.as_object_mut().ok_or_else(|| ConfigError("Cannot set properties of a non-object".into()))
}

pub fn widgets(config: &mut Value) -> R<&mut Vec<Value>> {
    config.get_mut("widgets").and_then(Value::as_array_mut).ok_or_else(|| ConfigError("no widgets".into()))
}

/// `config.widgets.find(w => w.wmType === type)!`
pub fn widget<'a>(config: &'a mut Value, wm_type: &str) -> R<&'a mut Map<String, Value>> {
    let w = widgets(config)?
        .iter_mut()
        .find(|w| w["wmType"] == wm_type)
        .ok_or_else(|| ConfigError(format!("Cannot set properties of undefined ({wm_type})")))?;
    obj(w)
}

/// `obj.key = value`, where `None` is `undefined` (gone after the JSON round trip).
pub fn set(o: &mut Map<String, Value>, key: &str, value: Option<Value>) {
    match value {
        Some(v) => {
            o.insert(key.to_owned(), v);
        }
        None => {
            o.remove(key);
        }
    }
}

/// `(config as any)[key]`
pub fn top(config: &Value, key: &str) -> Option<Value> {
    config.get(key).cloned()
}

pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `Math.max(0, ...config.widgets.map(_ => _.wmId)) + 1`
pub fn next_wm_id(config: &mut Value) -> R<f64> {
    let mut max = 0.0_f64;
    for w in widgets(config)?.iter() {
        let id = w.get("wmId").and_then(Value::as_f64).unwrap_or(f64::NAN);
        if id.is_nan() {
            return Ok(f64::NAN);
        }
        max = max.max(id);
    }
    Ok(max + 1.0)
}

pub fn version(config: &Value) -> f64 {
    config.get("configVersion").and_then(Value::as_f64).unwrap_or(f64::NAN)
}

/// `{ ...defaultConfig().widgets.find(type)!, ...extra }` with a new `wmId`;
/// `default_widget` is the game's `defaultConfig().widgets.find(type)!`.
pub fn new_widget(
    config: &mut Value,
    default_widget: &dyn Fn(&str) -> Value,
    wm_type: &str,
    extra: &[(&str, Value)],
) -> R<Value> {
    let mut w = default_widget(wm_type);
    // A double, written as JavaScript writes it (`10`, not `10.0`).
    w["wmId"] = json_value(&next_wm_id(config)?);
    for (k, v) in extra {
        w[*k] = v.clone();
    }
    Ok(w)
}

/// `upgradeConfig` steps 3–16.
pub fn upgrade_to_16(config: &mut Value, default_widget: &dyn Fn(&str) -> Value) -> R<()> {
    let c = &mut *config;

    if version(c) < 3.0 {
        let strip = new_widget(c, default_widget, "image-strip", &[("wmZorder", Value::Null)])?;
        widgets(c)?.push(strip);
        let delve = new_widget(c, default_widget, "delve-grid", &[("wmZorder", Value::Null)])?;
        widgets(c)?.push(delve);
        widget(c, "menu")?.insert("alwaysShow".into(), json!(false));
        c["configVersion"] = json!(3);
    }

    if version(c) < 4.0 {
        widget(c, "price-check")?.insert("chaosPriceThreshold".into(), json!(0.05));
        let map_check = widget(c, "map-check")?;
        for e in map_check.get_mut("selectedStats").and_then(Value::as_array_mut).into_iter().flatten() {
            let e = obj(e)?;
            let match_ref = e.get("matchRef").cloned();
            set(e, "matcher", match_ref);
            e.remove("matchRef");
        }
        for strip in widgets(c)?.iter_mut().filter(|w| w["wmType"] == "image-strip") {
            for (idx, e) in strip.get_mut("images").and_then(Value::as_array_mut).into_iter().flatten().enumerate() {
                obj(e)?.insert("id".into(), json!(idx));
            }
        }
        c["configVersion"] = json!(4);
    }

    if version(c) < 5.0 {
        for cmd in c.get_mut("commands").and_then(Value::as_array_mut).into_iter().flatten() {
            obj(cmd)?.insert("send".into(), json!(true));
        }
        c["configVersion"] = json!(5);
    }

    if version(c) < 6.0 {
        let debug = c.get("logLevel") == Some(&json!("debug"));
        let pc = widget(c, "price-check")?;
        pc.insert("showRateLimitState".into(), json!(debug));
        pc.insert("apiLatencySeconds".into(), json!(2));
        c["configVersion"] = json!(6);
    }

    if version(c) < 7.0 {
        let map_check_key = top(c, "mapCheckKey");
        let map_check = widget(c, "map-check")?;
        map_check.insert("wmType".into(), json!("item-check"));
        let selected = map_check.remove("selectedStats");
        map_check.insert("maps".into(), json!({ "selectedStats": selected }));
        if selected.is_none() {
            map_check["maps"].as_object_mut().expect("just made").remove("selectedStats");
        }
        let o = obj(c)?;
        o.insert(
            "itemCheckKey".into(),
            if truthy(map_check_key.as_ref()) { map_check_key.expect("truthy") } else { Value::Null },
        );
        o.remove("mapCheckKey");
        c["configVersion"] = json!(7);
    }

    if version(c) < 8.0 {
        let item_check = widget(c, "item-check")?;
        let maps = item_check.get_mut("maps").ok_or_else(|| ConfigError("no maps".into()))?;
        obj(maps)?.insert("showNewStats".into(), json!(false));
        let stats = maps
            .get_mut("selectedStats")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| ConfigError("no stats".into()))?;
        for entry in stats.iter_mut() {
            let decision = if truthy(entry.get("valueDanger")) {
                "danger"
            } else if truthy(entry.get("valueWarning")) {
                "warning"
            } else if truthy(entry.get("valueDesirable")) {
                "desirable"
            } else {
                "seen"
            };
            let mut new = json!({ "decision": decision });
            if let Some(m) = entry.get("matcher") {
                new["matcher"] = m.clone();
            }
            *entry = new;
        }
        c["configVersion"] = json!(8);
    }

    if version(c) < 9.0 {
        let pc = widget(c, "price-check")?;
        pc.insert("collapseListings".into(), json!("api"));
        pc.insert("smartInitialSearch".into(), json!(true));
        pc.insert("lockedInitialSearch".into(), json!(true));
        pc.insert("activateStockFilter".into(), json!(false));
        c["configVersion"] = json!(9);
    }

    if version(c) < 10.0 {
        let settings = new_widget(c, default_widget, "settings", &[])?;
        widgets(c)?.push(settings);
        let moved: Vec<(&str, Option<Value>)> = [
            ("hotkey", "priceCheckKey"),
            ("hotkeyHold", "priceCheckKeyHold"),
            ("hotkeyLocked", "priceCheckLocked"),
            ("showSeller", "showSeller"),
            ("searchStatRange", "searchStatRange"),
            ("showCursor", "priceCheckShowCursor"),
        ]
        .into_iter()
        .map(|(to, from)| (to, top(c, from)))
        .collect();
        let pc = widget(c, "price-check")?;
        for (to, value) in moved {
            set(pc, to, value);
        }
        if pc.get("chaosPriceThreshold") == Some(&json!(0.05)) {
            pc.insert("chaosPriceThreshold".into(), json!(0));
        }
        c["configVersion"] = json!(10);
    }

    if version(c) < 11.0 {
        widget(c, "price-check")?.insert("requestPricePrediction".into(), json!(false));
        c["configVersion"] = json!(11);
    }

    if version(c) < 12.0 {
        let after_settings = widgets(c)?.iter().position(|w| w["wmType"] == "settings");
        let item_search = new_widget(c, default_widget, "item-search", &[("wmWants", json!("show"))])?;
        // `findIndex` is -1 without one: `splice(0, ...)`.
        let at = after_settings.map_or(0, |i| i + 1);
        widgets(c)?.insert(at, item_search);
        c["realm"] = json!("pc-ggg");
        if c.get("language") == Some(&json!("zh_TW")) {
            c["language"] = json!("cmn-Hant");
        }
        c["configVersion"] = json!(12);
    }

    if version(c) < 13.0 {
        c["showAttachNotification"] = json!(true);
        c["configVersion"] = json!(13);
    }

    if version(c) < 14.0 {
        for strip in widgets(c)?.iter_mut().filter(|w| w["wmType"] == "image-strip") {
            for e in strip.get_mut("images").and_then(Value::as_array_mut).into_iter().flatten() {
                if let Some(url) = e.get("url").and_then(Value::as_str)
                    && let Some(rest) = url.strip_prefix("app-file://")
                {
                    e["url"] = json!(rest);
                }
            }
        }
        let (wiki, coe) = (top(c, "wikiKey"), top(c, "craftOfExileKey"));
        let ic = widget(c, "item-check")?;
        set(ic, "wikiKey", wiki);
        ic.insert("poedbKey".into(), Value::Null);
        set(ic, "craftOfExileKey", coe);
        ic.insert("stashSearchKey".into(), Value::Null);
        c["configVersion"] = json!(14);
    }

    if version(c) < 15.0 {
        widget(c, "price-check")?.insert("builtinBrowser".into(), json!(false));
        widget(c, "item-search")?.insert("ocrGemsKey".into(), Value::Null);
        let ic = widget(c, "item-check")?;
        let maps = obj(ic.get_mut("maps").ok_or_else(|| ConfigError("no maps".into()))?)?;
        maps.insert("profile".into(), json!(1));
        for stat in maps.get_mut("selectedStats").and_then(Value::as_array_mut).into_iter().flatten() {
            let p1 = match stat.get("decision").and_then(Value::as_str) {
                Some("danger") => "d",
                Some("warning") => "w",
                Some("desirable") => "g",
                _ => "s",
            };
            stat["decision"] = json!(format!("{p1}--"));
        }
        c["configVersion"] = json!(15);
    }

    if version(c) < 16.0 {
        let delve_key = top(c, "delveGridKey");
        set(widget(c, "delve-grid")?, "toggleKey", delve_key);
        let item_check_key = top(c, "itemCheckKey");
        let ic = widget(c, "item-check")?;
        set(ic, "hotkey", item_check_key);
        let maps = obj(ic.get_mut("maps").ok_or_else(|| ConfigError("no maps".into()))?)?;
        if !maps.contains_key("profile") {
            maps.insert("profile".into(), json!(1));
            maps.insert("selectedStats".into(), json!([]));
        }
        c["configVersion"] = json!(16);
    }

    Ok(())
}

/// `${value}`
pub fn text(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.as_f64().map_or_else(|| n.to_string(), number_to_string),
        Some(other) => other.to_string(),
    }
}

pub fn shortcut(shortcut: String, action: Value, keep_mod_keys: bool) -> Value {
    let mut s = json!({ "shortcut": shortcut, "action": action });
    if keep_mod_keys {
        s["keepModKeys"] = json!(true);
    }
    s
}
