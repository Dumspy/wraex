//! The `en` UI strings Exiled Exchange 2 ships with.
//!
//! Read through the data layer from the game data root's `en/app_i18n.json`
//! (`ref/ee2/renderer/public/data` in development, `assets/poe2/data` in a release), the
//! file `web/i18n.ts` loads, and looked up by a dotted key the way `vue-i18n` resolves it.
//! A key that isn't there prints as itself — which is what EE2 does too: it only ships the
//! strings it translates, so `t('Instant')`, `t('min')` or `t('Not recognized modifier')`
//! come out as the English they were written as.
//!
//! EE2's components call `t(':matched')` through `useI18nNs('trade_result')`, which prefixes
//! the namespace; the port passes the full key (`trade_result.matched`) instead.

use std::sync::OnceLock;

use serde_json::Value;
use wraex_core::data::DataSource;
use wraex_poe2::data::source;

/// The `en` messages, relative to the game data root (`poe2::data::source`).
const PATH: &str = "en/app_i18n.json";

fn strings() -> &'static Value {
    static STRINGS: OnceLock<Value> = OnceLock::new();
    STRINGS.get_or_init(|| {
        let text = source().read_string(PATH).unwrap_or_else(|e| panic!("read {PATH}: {e}"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ee2s_strings_load_and_missing_keys_print_themselves() {
        assert_eq!(t("filters.preset_base_item"), "Base item");
        assert_eq!(fmt("filters.selected_some", &["2", "5"]), "2 of 5, stats");
        // EE2 ships no translation for these; vue-i18n prints the key.
        assert_eq!(t("Not recognized modifier"), "Not recognized modifier");
    }
}
