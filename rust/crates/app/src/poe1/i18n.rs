//! The `en` UI strings upstream ships with.
//!
//! Read through the data layer from the game data root's `en/app_i18n.json`
//! (`ref/apt/renderer/public/data` in development, `assets/poe1/data` in a release), the
//! same file the reference loads, and looked up by a dotted key (`item.not_corrupted`,
//! `item_category.prop`) the way `vue-i18n` resolves it. A key that isn't there prints as
//! itself, as `vue-i18n` does.
//!
//! This is a stopgap for the price check's few labels: PLAN phase 5's last step replaces it
//! with upstream's Crowdin files in every language (`ru`, `ko`, `cmn-Hant`).
//!
//! The port's own features (`EXTRA`, the Inscribed Ultimatum chips) have no upstream key, so
//! their `en` text lives here until the feature is a data file of its own.

use std::sync::OnceLock;

use serde_json::Value;
use wraex_core::data::DataSource;
use wraex_poe1::data::source;

/// The `en` messages, relative to the game data root (`poe1::data::source`).
const PATH: &str = "en/app_i18n.json";

/// The port's own strings: upstream has no Inscribed Ultimatum support, so `app_i18n.json`
/// has no entry for the Ultimatum chips, the value estimate or the check.
const EXTRA: &[(&str, &str)] = &[
    ("ultimatum.name", "Ultimatum"),
    ("ultimatum.title", "Inscribed Ultimatum"),
    ("ultimatum.challenge", "Challenge: {0}"),
    ("ultimatum.area_level", "Area Level {0}"),
    ("ultimatum.reward", "Reward: {0}"),
    ("ultimatum.sacrifice", "Sacrifice: {0}"),
    ("ultimatum.sacrifice_note", "Listings mix sacrifice counts"),
    ("ultimatum.value_sacrifice", "Sacrifice worth {0}"),
    ("ultimatum.value_reward", "Reward worth {0}"),
    ("ultimatum.monsters", "Monsters: {0}"),
    ("ultimatum.monster_damage", "{0}% increased damage"),
    ("ultimatum.monster_life", "{0}% more life"),
    ("ultimatum.dont_run", "Don't run"),
    ("ultimatum.careful", "Careful"),
    ("ultimatum.settings_hint", "Each chip sets one tier for the current profile."),
    ("ultimatum.settings_empty", "Mark an Ultimatum modifier in the item check to see it here."),
];

fn strings() -> &'static Value {
    static STRINGS: OnceLock<Value> = OnceLock::new();
    STRINGS.get_or_init(|| {
        let text = source().read_string(PATH).unwrap_or_else(|e| panic!("read {PATH}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {PATH}: {e}"))
    })
}

/// `t(key)`: the message at `key`, or `key` itself.
pub fn t(key: &str) -> String {
    if let Some((_, message)) = EXTRA.iter().find(|(extra, _)| *extra == key) {
        return (*message).to_owned();
    }
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
