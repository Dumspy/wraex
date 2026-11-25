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

#[cfg(test)]
mod tests {
    use std::sync::OnceLock;

    use super::*;
    use crate::data::{Data, reference_source, reference_trade_data};

    fn data(lang: &'static str) -> &'static Data {
        static EN: OnceLock<Data> = OnceLock::new();
        static HANT: OnceLock<Data> = OnceLock::new();
        match lang {
            "en" => EN.get_or_init(|| Data::load(&reference_source(), "en", Some(reference_trade_data())).unwrap()),
            "cmn-Hant" => {
                HANT.get_or_init(|| Data::load(&reference_source(), "cmn-Hant", Some(reference_trade_data())).unwrap())
            }
            other => panic!("no cached data for {other}"),
        }
    }

    fn base(d: &Data, name: &str) -> Option<String> {
        magic_basetype(d, name).unwrap()
    }

    /// `specs/Parser/magic-name.test.ts` (`Check Magic Name (en)`).
    #[test]
    fn finds_the_english_base_type_inside_a_magic_name() {
        let d = data("en");
        assert_eq!(base(d, "Rattling Sceptre").as_deref(), Some("Rattling Sceptre"));
        assert_eq!(base(d, "Cultist Greathammer of Nourishment").as_deref(), Some("Cultist Greathammer"));
        assert_eq!(base(d, "Pulsing Antler Focus").as_deref(), Some("Antler Focus"));
        assert_eq!(base(d, "Reaver's Temple Maul of Stunning").as_deref(), Some("Temple Maul"));
    }

    /// `specs/Parser/magic-name.test.ts` (`Check Magic Name (cmn-Hant)`): names
    /// without spaces split into characters.
    #[test]
    fn finds_the_traditional_chinese_base_type_inside_a_magic_name() {
        let d = data("cmn-Hant");
        assert_eq!(base(d, "雜響權杖").as_deref(), Some("雜響權杖"));
        assert_eq!(base(d, "營養之教徒巨錘").as_deref(), Some("教徒巨錘"));
        assert_eq!(base(d, "脈衝的靈鹿法器").as_deref(), Some("靈鹿法器"));
        assert_eq!(base(d, "掠奪者的擊暈之神殿重錘").as_deref(), Some("神殿重錘"));
    }

    /// `specs/Parser/magic-name.test.ts` (`Check Magic Name (cmn-Hant) when UI
    /// returns english name`): the ref names are English in every language.
    #[test]
    fn finds_the_english_base_type_with_traditional_chinese_data() {
        let d = data("cmn-Hant");
        assert_eq!(base(d, "Rattling Sceptre").as_deref(), Some("Rattling Sceptre"));
        assert_eq!(base(d, "Cultist Greathammer of Nourishment").as_deref(), Some("Cultist Greathammer"));
        assert_eq!(base(d, "Pulsing Antler Focus").as_deref(), Some("Antler Focus"));
        assert_eq!(base(d, "Reaver's Temple Maul of Stunning").as_deref(), Some("Temple Maul"));
    }
}
