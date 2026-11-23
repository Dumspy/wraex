//! `ref/ee2/renderer/src/parser/magic-name.ts`: the base type inside a magic
//! item's name.

use wraex_core::js;

use super::Fail;
use crate::data::{Data, Namespace, TradeItemQuery};

/// `magicBasetype`: the longest run of words in `name` that is a craftable item,
/// preferring the shipped data over the trade site's list.
pub fn magic_basetype(d: &Data, name: &str) -> Result<Option<String>, Fail> {
    // Traditional Chinese names without spaces split into characters; English
    // and the other languages split on spaces (EE2 checks the configured language).
    let separator =
        if d.lang() == "cmn-Hant" && name.chars().any(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c)) { "" } else { " " };
    let words: Vec<String> = if separator.is_empty() {
        name.chars().map(String::from).collect()
    } else {
        name.split(' ').map(str::to_owned).collect()
    };

    struct Found {
        name: String,
        trade_item: bool,
    }
    let mut found: Vec<Found> = Vec::new();
    for start in 0..words.len() {
        for end in start..words.len() {
            let candidate = words[start..=end].join(separator);
            // BUG[UPSTREAM]: https://www.pathofexile.com/forum/view-thread/3913283
            let result = d
                .item_by_ref(Namespace::Item, &candidate)
                .or_else(|| d.item_by_translated(Namespace::Item, &candidate));
            let (craftable, trade_item) = match result {
                // `result[0].craftable` throws on an empty list (a hash collision).
                Some(r) => (r.first().ok_or(Fail::THROW)?.craftable.is_some(), false),
                None => {
                    let query = TradeItemQuery { name: Some(&candidate), ..Default::default() };
                    (d.trade_item_by_ref(&query, true).is_some_and(|r| r[0].craftable.is_some()), true)
                }
            };
            if craftable {
                found.push(Found { name: candidate, trade_item });
            }
        }
    }
    // Longest first (UTF-16 length), data items far ahead of trade-site ones.
    let key = |f: &Found| js::length(&f.name) as i64 + if f.trade_item { 0 } else { 100_000 };
    found.sort_by_key(|f| std::cmp::Reverse(key(f)));
    Ok(found.into_iter().next().map(|f| f.name))
}
