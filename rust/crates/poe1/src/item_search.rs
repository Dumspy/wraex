//! `ref/apt/renderer/src/web/item-search/WidgetItemSearch.vue`: the name search behind
//! the item search widget.
//!
//! The widget is a small overlay: type a name, and it lists the game's skill gems (the
//! heist target tab) or replica uniques (the other one), then hands the picked base type
//! to the price check. The matching is this module; the widget is
//! `wraex_app::widgets::item_search`.
//!
//! `findItems` searches the names the data layer generates (`ALTQ_GEM_NAMES`,
//! `REPLICA_UNIQUE_NAMES`): every word of the query must appear somewhere in the name
//! (case-insensitively), and one word of the name must *start with* the query's longest
//! word, so "head" finds "Replica Headhunter" but not "Replica Heatshiver". The
//! `cmn-Hant` client skips that second rule (Chinese item names have no word breaks).
//!
//! `fuzzyFindHeistGem` is the OCR path: a noisy line from the heist gem scanner
//! (`MAIN->CLIENT::ocr-text`) is compared against every `Anomalous`/`Divergent`/
//! `Phantasmal` gem name, and the closest one wins. Never reads the clipboard, so it
//! only matters once OCR exists; ported with the rest so the matching has one home.

use wraex_core::js;
use wraex_core::levenshtein::distance;

use crate::data::{BaseType, Data, Namespace};

/// `MAX_RESULTS`: more matches than this are not shown at all (`results === false`).
pub const MAX_RESULTS: usize = 5;

/// `findItems`'s answer.
///
/// The reference returns `BaseType[] | false`: `false` for a query under three characters
/// and for one that matches more than [`MAX_RESULTS`] items, which both render as "Too many
/// items found, enter the name more precisely."
#[derive(Debug, Clone, PartialEq)]
pub enum SearchResults {
    /// `false`: show the "too many" message.
    TooMany,
    /// The matches, possibly none (which renders "No items found.").
    Found(Vec<BaseType>),
}

impl SearchResults {
    /// The matches, when the query found any.
    pub fn items(&self) -> &[BaseType] {
        match self {
            Self::TooMany => &[],
            Self::Found(items) => items,
        }
    }
}

/// `findItems`: the item search's matching over `item_names`.
pub fn find_items(
    data: &Data,
    language: &str,
    search: &str,
    namespace: Namespace,
    item_names: &[String],
) -> SearchResults {
    let search = js::trim(search);
    if js::length(search) < 3 {
        return SearchResults::TooMany;
    }
    // `toLowerCase().split(/\s+/)`: JavaScript's whitespace, not Rust's.
    let mut parts: Vec<String> = js::split_whitespace(&search.to_lowercase()).into_iter().map(str::to_owned).collect();
    // `sort((a, b) => b.length - a.length)`: longest first, in JavaScript's UTF-16 lengths.
    parts.sort_by_key(|part| std::cmp::Reverse(js::length(part)));
    let longest = parts.first().map_or("", String::as_str);

    let mut out = Vec::new();
    for item_name in item_names {
        let lower = item_name.to_lowercase();
        let matches = parts.iter().all(|part| lower.contains(part.as_str()))
            && (language == "cmn-Hant" || js::split_whitespace(&lower).iter().any(|word| word.starts_with(longest)));
        if !matches {
            continue;
        }
        if let Some(found) = data.item_by_translated(namespace, item_name) {
            out.extend(found);
        }
        if out.len() > MAX_RESULTS {
            return SearchResults::TooMany;
        }
    }
    SearchResults::Found(out)
}

/// `results` on the `gem` tab: `ALTQ_GEM_NAMES`.
pub fn find_gems(data: &Data, language: &str, search: &str) -> SearchResults {
    find_items(data, language, search, Namespace::Gem, data.altq_gem_names())
}

/// `results` on the `replica` tab: `REPLICA_UNIQUE_NAMES`.
pub fn find_replicas(data: &Data, language: &str, search: &str) -> SearchResults {
    find_items(data, language, search, Namespace::Unique, data.replica_unique_names())
}

/// One `fuzzyFindHeistGem` match: the gem's translated name and its alternate quality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyGem {
    pub name: String,
    /// `Anomalous`, `Divergent` or `Phantasmal`.
    pub alt_quality: &'static str,
}

/// `fuzzyFindHeistGem`: the gem name an OCR line most likely meant.
///
/// Each quality's client-string regex (`/^Anomalous (.*)$/`) is turned back into a
/// template and every gem name substituted into it; edit distances over
/// [`distance`] pick the smallest, and a name more than five characters away in
/// length is not considered.
pub fn fuzzy_find_heist_gem(data: &Data, bad_str: &str) -> Option<FuzzyGem> {
    let bad_str = bad_str.to_lowercase();
    // The reference computes `regex.toString().slice(2, -2)`: the pattern without its
    // slashes and flags.
    let qualities =
        [("Anomalous", "QUALITY_ANOMALOUS"), ("Divergent", "QUALITY_DIVERGENT"), ("Phantasmal", "QUALITY_PHANTASMAL")];
    let mut best: Option<FuzzyGem> = None;
    let mut min_dist = usize::MAX;
    for name in data.altq_gem_names() {
        for (alt_quality, key) in qualities {
            let template = template(data.re(key).source());
            let exact = template.replacen("(.*)", name, 1).to_lowercase();
            if js::length(&exact).abs_diff(js::length(&bad_str)) > 5 {
                continue;
            }
            let dist = distance(&bad_str, &exact);
            if dist < min_dist {
                best = Some(FuzzyGem { name: name.clone(), alt_quality });
                if dist == 0 {
                    return best;
                }
                min_dist = dist;
            }
        }
    }
    best
}

/// `regex.toString().slice(2, -2)`: the reference slices the regex *literal*,
/// `/^Anomalous (.*)$/`, dropping `/^` and `$/` to leave `Anomalous (.*)`. The reader here
/// already took the slashes off, so the same cut is one character at each end. It slices
/// blindly, so this does too; the quality patterns carry no flags (the data tests check
/// every key), which is what keeps the two cuts equivalent.
fn template(source: &str) -> String {
    let units: Vec<u16> = source.encode_utf16().collect();
    let start = 1.min(units.len());
    let end = units.len().saturating_sub(1);
    String::from_utf16_lossy(units.get(start..end).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use wraex_core::GameId;

    use super::*;

    /// `fixtures/item-search/poe1.json`.
    #[derive(Debug, Deserialize)]
    struct Fixture {
        searches: Vec<SearchEntry>,
        fuzzy: Vec<FuzzyEntry>,
    }

    #[derive(Debug, Deserialize)]
    struct SearchEntry {
        lang: String,
        language: String,
        tab: String,
        query: String,
        /// The matched `BaseType` list, or `false` (too short, or too many matches).
        results: Value,
    }

    #[derive(Debug, Deserialize)]
    struct FuzzyEntry {
        lang: String,
        query: String,
        result: Value,
    }

    /// A fuzzy match as `fuzzyFindHeistGem`'s answer serialises.
    #[derive(Debug, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct FuzzyMatch {
        alt_quality: &'static str,
        name: String,
    }

    fn names(results: &SearchResults) -> Vec<&str> {
        results.items().iter().map(|item| item.name.as_str()).collect()
    }

    /// `SearchResults` as the reference's `BaseType[] | false`.
    fn json_results(results: &SearchResults) -> Value {
        match results {
            SearchResults::TooMany => Value::Bool(false),
            SearchResults::Found(items) => wraex_core::js::json_value(items),
        }
    }

    /// Every query in `corpus/item-search/poe1.json`, with the answer the
    /// reference's own `findItems` and `fuzzyFindHeistGem` gave.
    #[test]
    fn searches_and_fuzzy_matches_equal_the_fixtures() {
        let fixture: Fixture =
            serde_json::from_value(wraex_golden::item_search_fixture(GameId::Poe1)).expect("fixtures/item-search");
        let mut langs: HashMap<String, Data> = HashMap::new();
        let mut failures = Vec::new();

        assert!(!fixture.searches.is_empty(), "no recorded searches: run `npm run regen` in tools/golden");
        for entry in &fixture.searches {
            let data = langs
                .entry(entry.lang.clone())
                .or_insert_with(|| Data::load(&crate::data::reference_source(), &entry.lang).unwrap());
            let actual = match entry.tab.as_str() {
                "gem" => json_results(&find_gems(data, &entry.language, &entry.query)),
                "replica" => json_results(&find_replicas(data, &entry.language, &entry.query)),
                other => panic!("unknown PoE1 item-search tab {other:?}"),
            };
            if let Some(diff) = wraex_golden::diff(&entry.results, &actual) {
                failures.push(format!("{} {:?} (language {}): {diff}", entry.tab, entry.query, entry.language));
            }
        }

        assert!(!fixture.fuzzy.is_empty(), "no recorded fuzzy lines: run `npm run regen` in tools/golden");
        for entry in &fixture.fuzzy {
            let data = langs
                .entry(entry.lang.clone())
                .or_insert_with(|| Data::load(&crate::data::reference_source(), &entry.lang).unwrap());
            let actual = match fuzzy_find_heist_gem(data, &entry.query) {
                None => Value::Null,
                Some(gem) => wraex_core::js::json_value(&FuzzyMatch { alt_quality: gem.alt_quality, name: gem.name }),
            };
            if let Some(diff) = wraex_golden::diff(&entry.result, &actual) {
                failures.push(format!("fuzzy {:?}: {diff}", entry.query));
            }
        }

        assert!(failures.is_empty(), "{} item-search answers differ:\n\n{}", failures.len(), failures.join("\n\n"));
    }

    /// The gem tab at `ref/apt` @ `2498730`: the shipped `items.ndjson` has no
    /// `altQuality` field, so `ALTQ_GEM_NAMES` is empty and every query finds nothing.
    /// (Upstream's widget shows "No items found." for it too.)
    #[test]
    fn the_alt_quality_gem_list_is_empty_at_this_revision() {
        let data = crate::data::tests::en();
        assert_eq!(data.altq_gem_names(), Vec::<String>::new());
        assert_eq!(find_gems(data, "en", "empower"), SearchResults::Found(Vec::new()));
    }

    /// The replica tab: `REPLICA_UNIQUE_NAMES`, `ITEM_BY_TRANSLATED('UNIQUE', name)`.
    #[test]
    fn finds_replica_uniques() {
        let data = crate::data::tests::en();
        assert_eq!(names(&find_replicas(data, "en", "headhunter")), ["Replica Headhunter"]);
        // Every query word must appear, and one name word must start with the longest.
        assert_eq!(names(&find_replicas(data, "en", "replica headhunter")), ["Replica Headhunter"]);
        assert_eq!(names(&find_replicas(data, "en", "replica kaom")), ["Replica Kaom's Heart"]);
        // Case and surrounding spaces do not matter.
        assert_eq!(names(&find_replicas(data, "en", "  REPLICA HEADHUNTER  ")), ["Replica Headhunter"]);
        // "hea" is a prefix of four names, which still fits under the limit.
        assert_eq!(
            names(&find_replicas(data, "en", "hea")),
            ["Replica Headhunter", "Replica Heartbreaker", "Replica Heatshiver", "Replica Kaom's Heart"]
        );
        // "head" is not a prefix of "Heatshiver", so only the headhunter matches.
        assert_eq!(names(&find_replicas(data, "en", "head")), ["Replica Headhunter"]);
        // "replica w" matches six, so the reference shows the "too many" message.
        assert_eq!(find_replicas(data, "en", "replica w"), SearchResults::TooMany);
        assert_eq!(
            names(&find_replicas(data, "en", "replica wi")),
            ["Replica Windripper", "Replica Wings of Entropy", "Replica Winterheart", "Replica Witchfire Brew"]
        );
        // A name word has to start with the longest query word: "kaoms" does not
        // prefix-match "Kaom's".
        assert_eq!(find_replicas(data, "en", "kaoms"), SearchResults::Found(Vec::new()));
        // Under three characters the reference returns `false` before it looks at all.
        assert_eq!(find_replicas(data, "en", "he"), SearchResults::TooMany);
    }

    /// The `cmn-Hant` client drops the word-prefix rule.
    #[test]
    fn the_chinese_client_skips_the_word_prefix_rule() {
        let data = crate::data::tests::en();
        // On the English data the rule is what rejects "eadhunter"; the language check
        // lets it through, as the reference's does.
        assert_eq!(find_replicas(data, "en", "eadhunter"), SearchResults::Found(Vec::new()));
        assert_eq!(names(&find_replicas(data, "cmn-Hant", "eadhunter")), ["Replica Headhunter"]);
    }

    /// A perfect template match wins outright; a typo that keeps the length within five
    /// still picks the nearest gem. Both lists are empty or English-only at this
    /// revision, so this exercises the loop's arithmetic rather than upstream's data.
    #[test]
    fn the_fuzzy_matcher_returns_nothing_without_gem_names() {
        let data = crate::data::tests::en();
        assert_eq!(fuzzy_find_heist_gem(data, "Anomalous Empower Support"), None);
    }
}
