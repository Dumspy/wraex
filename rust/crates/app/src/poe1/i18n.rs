//! The `en` UI strings upstream ships with.
//!
//! Read through the data layer from `ref/apt/renderer/public/data/en/app_i18n.json`, the same
//! file the reference loads, and looked up by a dotted key (`item.not_corrupted`,
//! `item_category.prop`) the way `vue-i18n` resolves it. A key that isn't there prints as
//! itself, as `vue-i18n` does.
//!
//! This is a stopgap for the price check's few labels: PLAN phase 5's last step replaces it
//! with upstream's Crowdin files in every language (`ru`, `ko`, `cmn-Hant`).

use std::sync::OnceLock;

use serde_json::Value;
use wraex_core::data::DataSource;
use wraex_poe1::data::reference_source;

/// The `en` messages, relative to the reference's `public/data` (`poe1::data::reference_source`).
const PATH: &str = "en/app_i18n.json";

fn strings() -> &'static Value {
    static STRINGS: OnceLock<Value> = OnceLock::new();
    STRINGS.get_or_init(|| {
        let text = reference_source().read_string(PATH).unwrap_or_else(|e| panic!("read {PATH}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {PATH}: {e}"))
    })
}

/// `t(key)`: the message at `key`, or `key` itself.
pub fn t(key: &str) -> String {
    match lookup(key) {
        Some(Value::String(message)) => message.clone(),
        _ => key.to_owned(),
    }
}

/// `t(key, [a, b])`: the message with `{0}`, `{1}`, ... replaced.
pub fn fmt(key: &str, args: &[&str]) -> String {
    let mut message = t(key);
    for (i, arg) in args.iter().enumerate() {
        message = message.replace(&format!("{{{i}}}"), arg);
    }
    message
}

fn lookup(key: &str) -> Option<&'static Value> {
    let mut value = strings();
    for segment in key.split('.') {
        value = value.get(segment)?;
    }
    Some(value)
}
