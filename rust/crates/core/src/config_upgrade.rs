//! `upgradeConfig` steps 3–16 and the helpers they share.
//!
//! Identical in both references (EE2 forked at version 16): the config stays
//! JSON ([`Value`]) so every field survives, and writing `undefined` to a field
//! removes it, as the JSON round trip in `updateConfig` does. The only per-game
//! part is [`new_widget`]'s default widget, passed in by each game crate.

#![expect(
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "upstream's upgradeConfig migrates untyped JSON across every historic config shape; fields it doesn't know must survive"
)]

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

/// `w.wmType` of a widget, when it is a string.
fn wm_type_of(w: &Value) -> Option<&str> {
    match w {
        Value::Object(m) => match m.get("wmType") {
            Some(Value::String(s)) => Some(s),
            _ => None,
        },
        _ => None,
    }
}

/// `config.widgets.find(w => w.wmType === type)!`
pub fn widget<'a>(config: &'a mut Value, wm_type: &str) -> R<&'a mut Map<String, Value>> {
    let w = widgets(config)?
        .iter_mut()
        .find(|w| wm_type_of(w) == Some(wm_type))
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
    let wm_id = json_value(&next_wm_id(config)?);
    if let Value::Object(o) = &mut w {
        o.insert("wmId".into(), wm_id);
        for (k, v) in extra {
            o.insert((*k).to_owned(), v.clone());
        }
    }
    Ok(w)
}

/// `config.configVersion = n`
fn set_version(config: &mut Value, n: u32) -> R<()> {
    obj(config)?.insert("configVersion".into(), json!(n));
    Ok(())
}

/// `upgradeConfig` steps 3–16.
pub fn upgrade_to_16(config: &mut Value, default_widget: &dyn Fn(&str) -> Value) -> R<()> {
    if version(config) < 3.0 {
        to_3(config, default_widget)?;
    }
    if version(config) < 4.0 {
        to_4(config)?;
    }
    if version(config) < 5.0 {
        to_5(config)?;
    }
    if version(config) < 6.0 {
        to_6(config)?;
    }
    if version(config) < 7.0 {
        to_7(config)?;
    }
    if version(config) < 8.0 {
        to_8(config)?;
    }
    if version(config) < 9.0 {
        to_9(config)?;
    }
    if version(config) < 10.0 {
        to_10(config, default_widget)?;
    }
    if version(config) < 11.0 {
        to_11(config)?;
    }
    if version(config) < 12.0 {
        to_12(config, default_widget)?;
    }
    if version(config) < 13.0 {
        to_13(config)?;
    }
    if version(config) < 14.0 {
        to_14(config)?;
    }
    if version(config) < 15.0 {
        to_15(config)?;
    }
    if version(config) < 16.0 {
        to_16(config)?;
    }
    Ok(())
}

fn to_3(config: &mut Value, default_widget: &dyn Fn(&str) -> Value) -> R<()> {
    let strip = new_widget(config, default_widget, "image-strip", &[("wmZorder", Value::Null)])?;
    widgets(config)?.push(strip);
    let delve = new_widget(config, default_widget, "delve-grid", &[("wmZorder", Value::Null)])?;
    widgets(config)?.push(delve);
    widget(config, "menu")?.insert("alwaysShow".into(), json!(false));
    set_version(config, 3)
}

fn to_4(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("chaosPriceThreshold".into(), json!(0.05));
    let map_check = widget(config, "map-check")?;
    for e in map_check.get_mut("selectedStats").and_then(Value::as_array_mut).into_iter().flatten() {
        let e = obj(e)?;
        let match_ref = e.get("matchRef").cloned();
        set(e, "matcher", match_ref);
        e.remove("matchRef");
    }
    for strip in widgets(config)?.iter_mut().filter(|w| wm_type_of(w) == Some("image-strip")) {
        for (idx, e) in strip.get_mut("images").and_then(Value::as_array_mut).into_iter().flatten().enumerate() {
            obj(e)?.insert("id".into(), json!(idx));
        }
    }
    set_version(config, 4)
}

fn to_5(config: &mut Value) -> R<()> {
    for cmd in config.get_mut("commands").and_then(Value::as_array_mut).into_iter().flatten() {
        obj(cmd)?.insert("send".into(), json!(true));
    }
    set_version(config, 5)
}

fn to_6(config: &mut Value) -> R<()> {
    let debug = config.get("logLevel") == Some(&json!("debug"));
    let pc = widget(config, "price-check")?;
    pc.insert("showRateLimitState".into(), json!(debug));
    pc.insert("apiLatencySeconds".into(), json!(2));
    set_version(config, 6)
}

fn to_7(config: &mut Value) -> R<()> {
    let map_check_key = top(config, "mapCheckKey");
    let map_check = widget(config, "map-check")?;
    map_check.insert("wmType".into(), json!("item-check"));
    let selected = map_check.remove("selectedStats");
    map_check.insert("maps".into(), json!({ "selectedStats": selected }));
    if selected.is_none() {
        let maps = obj(map_check.get_mut("maps").ok_or_else(|| ConfigError("no maps".into()))?)?;
        maps.remove("selectedStats");
    }
    let item_check_key = map_check_key.filter(|v| truthy(Some(v))).unwrap_or(Value::Null);
    let o = obj(config)?;
    o.insert("itemCheckKey".into(), item_check_key);
    o.remove("mapCheckKey");
    set_version(config, 7)
}

fn to_8(config: &mut Value) -> R<()> {
    let item_check = widget(config, "item-check")?;
    let maps = item_check.get_mut("maps").ok_or_else(|| ConfigError("no maps".into()))?;
    obj(maps)?.insert("showNewStats".into(), json!(false));
    let stats =
        maps.get_mut("selectedStats").and_then(Value::as_array_mut).ok_or_else(|| ConfigError("no stats".into()))?;
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
        let mut new = Map::new();
        new.insert("decision".into(), json!(decision));
        if let Some(m) = entry.get("matcher") {
            new.insert("matcher".into(), m.clone());
        }
        *entry = Value::Object(new);
    }
    set_version(config, 8)
}

fn to_9(config: &mut Value) -> R<()> {
    let pc = widget(config, "price-check")?;
    pc.insert("collapseListings".into(), json!("api"));
    pc.insert("smartInitialSearch".into(), json!(true));
    pc.insert("lockedInitialSearch".into(), json!(true));
    pc.insert("activateStockFilter".into(), json!(false));
    set_version(config, 9)
}

fn to_10(config: &mut Value, default_widget: &dyn Fn(&str) -> Value) -> R<()> {
    let settings = new_widget(config, default_widget, "settings", &[])?;
    widgets(config)?.push(settings);
    let moved: Vec<(&str, Option<Value>)> = [
        ("hotkey", "priceCheckKey"),
        ("hotkeyHold", "priceCheckKeyHold"),
        ("hotkeyLocked", "priceCheckLocked"),
        ("showSeller", "showSeller"),
        ("searchStatRange", "searchStatRange"),
        ("showCursor", "priceCheckShowCursor"),
    ]
    .into_iter()
    .map(|(to, from)| (to, top(config, from)))
    .collect();
    let pc = widget(config, "price-check")?;
    for (to, value) in moved {
        set(pc, to, value);
    }
    if pc.get("chaosPriceThreshold") == Some(&json!(0.05)) {
        pc.insert("chaosPriceThreshold".into(), json!(0));
    }
    set_version(config, 10)
}

fn to_11(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("requestPricePrediction".into(), json!(false));
    set_version(config, 11)
}

fn to_12(config: &mut Value, default_widget: &dyn Fn(&str) -> Value) -> R<()> {
    let after_settings = widgets(config)?.iter().position(|w| wm_type_of(w) == Some("settings"));
    let item_search = new_widget(config, default_widget, "item-search", &[("wmWants", json!("show"))])?;
    // `findIndex` is -1 without one: `splice(0, ...)`.
    let at = after_settings.map_or(0, |i| i.saturating_add(1));
    widgets(config)?.insert(at, item_search);
    let o = obj(config)?;
    o.insert("realm".into(), json!("pc-ggg"));
    if o.get("language") == Some(&json!("zh_TW")) {
        o.insert("language".into(), json!("cmn-Hant"));
    }
    set_version(config, 12)
}

fn to_13(config: &mut Value) -> R<()> {
    obj(config)?.insert("showAttachNotification".into(), json!(true));
    set_version(config, 13)
}

fn to_14(config: &mut Value) -> R<()> {
    for strip in widgets(config)?.iter_mut().filter(|w| wm_type_of(w) == Some("image-strip")) {
        for e in strip.get_mut("images").and_then(Value::as_array_mut).into_iter().flatten() {
            let Value::Object(entry) = e else { continue };
            let Some(Value::String(url)) = entry.get("url") else { continue };
            if let Some(rest) = url.strip_prefix("app-file://") {
                let rest = rest.to_owned();
                entry.insert("url".into(), json!(rest));
            }
        }
    }
    let (wiki, coe) = (top(config, "wikiKey"), top(config, "craftOfExileKey"));
    let ic = widget(config, "item-check")?;
    set(ic, "wikiKey", wiki);
    ic.insert("poedbKey".into(), Value::Null);
    set(ic, "craftOfExileKey", coe);
    ic.insert("stashSearchKey".into(), Value::Null);
    set_version(config, 14)
}

fn to_15(config: &mut Value) -> R<()> {
    widget(config, "price-check")?.insert("builtinBrowser".into(), json!(false));
    widget(config, "item-search")?.insert("ocrGemsKey".into(), Value::Null);
    let ic = widget(config, "item-check")?;
    let maps = obj(ic.get_mut("maps").ok_or_else(|| ConfigError("no maps".into()))?)?;
    maps.insert("profile".into(), json!(1));
    for stat in maps.get_mut("selectedStats").and_then(Value::as_array_mut).into_iter().flatten() {
        let p1 = match stat.get("decision").and_then(Value::as_str) {
            Some("danger") => "d",
            Some("warning") => "w",
            Some("desirable") => "g",
            _ => "s",
        };
        obj(stat)?.insert("decision".into(), json!(format!("{p1}--")));
    }
    set_version(config, 15)
}

fn to_16(config: &mut Value) -> R<()> {
    let delve_key = top(config, "delveGridKey");
    set(widget(config, "delve-grid")?, "toggleKey", delve_key);
    let item_check_key = top(config, "itemCheckKey");
    let ic = widget(config, "item-check")?;
    set(ic, "hotkey", item_check_key);
    let maps = obj(ic.get_mut("maps").ok_or_else(|| ConfigError("no maps".into()))?)?;
    if !maps.contains_key("profile") {
        maps.insert("profile".into(), json!(1));
        maps.insert("selectedStats".into(), json!([]));
    }
    set_version(config, 16)
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
    let mut s = Map::new();
    s.insert("shortcut".to_owned(), Value::String(shortcut));
    s.insert("action".to_owned(), action);
    if keep_mod_keys {
        s.insert("keepModKeys".to_owned(), Value::Bool(true));
    }
    Value::Object(s)
}
