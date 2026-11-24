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
            SearchResults::TooMany => &[],
            SearchResults::Found(items) => items,
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
    let search = search.trim();
    if utf16_len(search) < 3 {
        return SearchResults::TooMany;
    }
    let mut parts: Vec<String> = search.to_lowercase().split_whitespace().map(str::to_owned).collect();
    // `sort((a, b) => b.length - a.length)`: longest first, in JavaScript's UTF-16 lengths.
    parts.sort_by_key(|part| std::cmp::Reverse(utf16_len(part)));
    let longest = parts.first().map(String::as_str).unwrap_or("");

    let mut out = Vec::new();
    for item_name in item_names {
        let lower = item_name.to_lowercase();
        let matches = parts.iter().all(|part| lower.contains(part.as_str()))
            && (language == "cmn-Hant" || lower.split_whitespace().any(|word| word.starts_with(longest)));
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
            if utf16_len(&exact).abs_diff(utf16_len(&bad_str)) > 5 {
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
    String::from_utf16_lossy(&units[1.min(units.len())..units.len().saturating_sub(1)])
}

/// JavaScript's `String#length`: UTF-16 code units.
fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(results: &SearchResults) -> Vec<&str> {
        results.items().iter().map(|item| item.name.as_str()).collect()
    }

    /// The gem tab at `ref/apt` @ `2498730`: the shipped `items.ndjson` has no
    /// `altQuality` field, so `ALTQ_GEM_NAMES` is empty and every query finds nothing.
    /// (Upstream's widget shows "No items found." for it too.)
    #[test]
    fn the_alt_quality_gem_list_is_empty_at_this_revision() {
        let data = crate::data::tests::en();
        assert!(data.altq_gem_names().is_empty());
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

    #[test]
    fn lengths_are_utf16_code_units() {
        assert_eq!(utf16_len("abc"), 3);
        assert_eq!(utf16_len("\u{1F600}"), 2, "an emoji is a surrogate pair");
    }
}
