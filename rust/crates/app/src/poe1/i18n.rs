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

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::Deserialize;
use wraex_core::data::DataSource;
use wraex_poe1::data::source;

/// The `en` messages, relative to the game data root (`poe1::data::source`).
const PATH: &str = "en/app_i18n.json";

/// One node of `app_i18n.json`: a message, or the group of messages a dotted key's segment
/// names (`vue-i18n`'s nested messages).
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Message {
    /// `"Not recognized modifier"`.
    Text(String),
    /// `{ "item": { "not_corrupted": "…" } }`.
    Group(BTreeMap<String, Self>),
}

/// The message table, read once.
#[expect(
    clippy::panic,
    reason = "the `en` messages ship with the assets: a missing or unparsable file is a broken install and must fail loudly at first use instead of printing raw keys"
)]
fn strings() -> &'static BTreeMap<String, Message> {
    static STRINGS: OnceLock<BTreeMap<String, Message>> = OnceLock::new();
    STRINGS.get_or_init(|| {
        let text = source().read_string(PATH).unwrap_or_else(|e| panic!("read {PATH}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {PATH}: {e}"))
    })
}

/// `t(key)`: the message at `key`, or `key` itself.
pub fn t(key: &str) -> String {
    match lookup(key) {
        Some(Message::Text(message)) => message.clone(),
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

fn lookup(key: &str) -> Option<&'static Message> {
    let mut segments = key.split('.');
    let mut message = strings().get(segments.next()?)?;
    for segment in segments {
        message = match message {
            Message::Group(group) => group.get(segment)?,
            // A leaf has no children: the key is not there, as `value[segment]` was
            // `undefined` for a string in `vue-i18n`.
            Message::Text(_) => return None,
        };
    }
    Some(message)
}
