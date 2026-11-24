//! `ref/ee2/renderer/src/web/item-search/WidgetItemSearch.vue`: the name search behind
//! EE2's item search widget.
//!
//! EE2 kept upstream's matcher and changed the tabs: instead of the heist target's skill
//! gems and replicas, it searches skill gems, uniques and base items, with the base items
//! as the tab the widget opens on. The names come from `GEM_NS_NAMES`,
//! `UNIQUE_NS_NAMES` and `ITEM_NS_NAMES`; the matching is `findItems` as in
//! [`wraex_poe1::item_search`] — every query word must appear in the name, and one name
//! word must start with the query's longest word.
//!
//! `fuzzyFindHeistGem` is EE2's OCR path (`MAIN->CLIENT::ocr-text`, target
//! `heist-gems`). EE2's handler only inserts the gem, without the alternate-quality
//! price upstream's does, but its matcher still walks `GEM_NS_NAMES`.

use wraex_core::js;
use wraex_core::levenshtein::distance;

use crate::data::{BaseType, Data, Namespace};

/// `MAX_RESULTS`: more matches than this are not shown at all (`results === false`).
pub const MAX_RESULTS: usize = 5;

/// `typeFilter`'s three tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeFilter {
    Gem,
    Unique,
    BaseItem,
}

impl TypeFilter {
    /// The values `typeFilter.value` takes, in the template's order.
    pub const ALL: [TypeFilter; 3] = [TypeFilter::Gem, TypeFilter::Unique, TypeFilter::BaseItem];

    /// `ref/ee2`'s `shallowRef<'gem' | 'unique' | 'base_item'>('base_item')`.
    pub const DEFAULT: TypeFilter = TypeFilter::BaseItem;

    pub const fn as_str(self) -> &'static str {
        match self {
            TypeFilter::Gem => "gem",
            TypeFilter::Unique => "unique",
            TypeFilter::BaseItem => "base_item",
        }
    }

    /// The `item_search.target_*` string key the tab is labelled with.
    pub const fn label_key(self) -> &'static str {
        match self {
            TypeFilter::Gem => "item_search.target_gem",
            TypeFilter::Unique => "item_search.target_unique",
            TypeFilter::BaseItem => "item_search.target_base_item",
        }
    }
}

/// `findItems`'s answer: `false` (too short a query, or more than [`MAX_RESULTS`] items) or
/// the matches.
#[derive(Debug, Clone, PartialEq)]
pub enum SearchResults {
    /// `false`: show the "too many" message (`:too_many_`).
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
    let search = js::trim(search);
    if js::length(search) < 3 {
        return SearchResults::TooMany;
    }
    // `toLowerCase().split(/\s+/)`: JavaScript's whitespace, not Rust's.
    let mut parts: Vec<String> = js::split_whitespace(&search.to_lowercase()).into_iter().map(str::to_owned).collect();
    // `sort((a, b) => b.length - a.length)`: longest first, in JavaScript's UTF-16 lengths.
    parts.sort_by_key(|part| std::cmp::Reverse(js::length(part)));
    let longest = parts.first().map(String::as_str).unwrap_or("");

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

/// `results` for one tab.
pub fn find(data: &Data, language: &str, search: &str, which: TypeFilter) -> SearchResults {
    match which {
        TypeFilter::Gem => find_items(data, language, search, Namespace::Gem, data.gem_ns_names()),
        TypeFilter::Unique => find_items(data, language, search, Namespace::Unique, data.unique_ns_names()),
        TypeFilter::BaseItem => find_items(data, language, search, Namespace::Item, data.item_ns_names()),
    }
}

/// One `fuzzyFindHeistGem` match: the gem's translated name and its alternate quality.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyGem {
    pub name: String,
    /// `Anomalous`, `Divergent` or `Phantasmal`.
    pub alt_quality: &'static str,
}

/// `fuzzyFindHeistGem`: the gem name an OCR line most likely meant.
pub fn fuzzy_find_heist_gem(data: &Data, bad_str: &str) -> Option<FuzzyGem> {
    let bad_str = bad_str.to_lowercase();
    // `regex.toString().slice(2, -2)`: the pattern without its slashes and flags.
    let qualities =
        [("Anomalous", "QUALITY_ANOMALOUS"), ("Divergent", "QUALITY_DIVERGENT"), ("Phantasmal", "QUALITY_PHANTASMAL")];
    let mut best: Option<FuzzyGem> = None;
    let mut min_dist = usize::MAX;
    for name in data.gem_ns_names() {
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
    String::from_utf16_lossy(&units[1.min(units.len())..units.len().saturating_sub(1)])
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use serde_json::{Value, json};
    use wraex_core::GameId;

    use super::*;

    fn names(results: &SearchResults) -> Vec<Option<&str>> {
        results.items().iter().map(|item| item.name.as_deref()).collect()
    }

    /// `SearchResults` as the reference's `BaseType[] | false`.
    fn json_results(results: &SearchResults) -> Value {
        match results {
            SearchResults::TooMany => Value::Bool(false),
            SearchResults::Found(items) => wraex_core::js::json_value(items),
        }
    }

    /// Every query in `corpus/item-search/poe2.json`, per tab, with the answer
    /// the reference's own `findItems` and `fuzzyFindHeistGem` gave.
    #[test]
    fn searches_and_fuzzy_matches_equal_the_fixtures() {
        let fixture = wraex_golden::item_search_fixture(GameId::Poe2);
        let mut langs: HashMap<String, Data> = HashMap::new();
        let mut failures = Vec::new();

        let searches = fixture["searches"].as_array().expect("searches");
        assert!(!searches.is_empty(), "no recorded searches: run `npm run regen` in tools/golden");
        for entry in searches {
            let lang = entry["lang"].as_str().unwrap();
            let language = entry["language"].as_str().unwrap();
            let tab = entry["tab"].as_str().unwrap();
            let query = entry["query"].as_str().unwrap();
            let data = langs
                .entry(lang.to_owned())
                .or_insert_with(|| Data::load(&crate::data::reference_source(), lang, None).unwrap());
            let which = match tab {
                "gem" => TypeFilter::Gem,
                "unique" => TypeFilter::Unique,
                "base_item" => TypeFilter::BaseItem,
                other => panic!("unknown PoE2 item-search tab {other:?}"),
            };
            let actual = json_results(&find(data, language, query, which));
            if let Some(diff) = wraex_golden::diff(&entry["results"], &actual) {
                failures.push(format!("{tab} {query:?} (language {language}): {diff}"));
            }
        }

        let fuzzy = fixture["fuzzy"].as_array().expect("fuzzy");
        assert!(!fuzzy.is_empty(), "no recorded fuzzy lines: run `npm run regen` in tools/golden");
        for entry in fuzzy {
            let lang = entry["lang"].as_str().unwrap();
            let query = entry["query"].as_str().unwrap();
            let data = langs
                .entry(lang.to_owned())
                .or_insert_with(|| Data::load(&crate::data::reference_source(), lang, None).unwrap());
            let actual = match fuzzy_find_heist_gem(data, query) {
                None => Value::Null,
                Some(gem) => json!({ "altQuality": gem.alt_quality, "name": gem.name }),
            };
            if let Some(diff) = wraex_golden::diff(&entry["result"], &actual) {
                failures.push(format!("fuzzy {query:?}: {diff}"));
            }
        }

        assert!(failures.is_empty(), "{} item-search answers differ:\n\n{}", failures.len(), failures.join("\n\n"));
    }

    /// The base-item tab, which the widget opens on: `ITEM_NS_NAMES` and
    /// `ITEM_BY_TRANSLATED('ITEM', name)`.
    #[test]
    fn finds_base_items() {
        let data = crate::data::tests::en();
        assert_eq!(names(&find(data, "en", "gold ring", TypeFilter::BaseItem)), [Some("Gold Ring")]);
        assert_eq!(names(&find(data, "en", "divine orb", TypeFilter::BaseItem)), [Some("Divine Orb")]);
        // Uniques are another tab's business.
        assert_eq!(find(data, "en", "mageblood", TypeFilter::BaseItem), SearchResults::Found(Vec::new()));
        // "ring" matches more than five base types.
        assert_eq!(find(data, "en", "ring", TypeFilter::BaseItem), SearchResults::TooMany);
        // Extra whitespace between words is one separator.
        assert_eq!(names(&find(data, "en", "Gold  Ring", TypeFilter::BaseItem)), [Some("Gold Ring")]);
        assert_eq!(
            names(&find(data, "en", "grand regalia", TypeFilter::BaseItem)),
            [Some("Grand Regalia"), Some("Runemastered Grand Regalia")]
        );
        assert_eq!(find(data, "en", "go", TypeFilter::BaseItem), SearchResults::TooMany, "under three characters");
    }

    /// The unique tab: a unique with several variants is listed once per record, as
    /// `ITEM_BY_TRANSLATED` returns every line `name` matches (`commonFind` stops at the
    /// first record without `disc`/`unique`).
    #[test]
    fn finds_uniques() {
        let data = crate::data::tests::en();
        assert_eq!(names(&find(data, "en", "mageblood", TypeFilter::Unique)), [Some("Mageblood")]);
        assert_eq!(
            names(&find(data, "en", "goldrim", TypeFilter::Unique)),
            [Some("Goldrim"), Some("Goldrim"), Some("Goldrim"), Some("Goldrim")]
        );
        assert_eq!(
            names(&find(data, "en", "the eternal spark", TypeFilter::Unique)),
            [
                Some("The Eternal Spark"),
                Some("The Eternal Spark"),
                Some("The Eternal Spark"),
                Some("The Eternal Spark")
            ]
        );
    }

    /// The gem tab: an uncut gem is an `ITEM`, not a `GEM`, so it is not here.
    #[test]
    fn finds_gems() {
        let data = crate::data::tests::en();
        assert_eq!(
            names(&find(data, "en", "empower", TypeFilter::Gem)),
            [Some("Empowered Sparks I"), Some("Empowered Sparks II")]
        );
        assert_eq!(find(data, "en", "uncut", TypeFilter::Gem), SearchResults::Found(Vec::new()));
    }

    /// `fuzzyFindHeistGem` over `GEM_NS_NAMES`: an exact template match wins at once, and a
    /// one-character typo still lands on the same gem.
    #[test]
    fn the_fuzzy_matcher_picks_the_nearest_gem() {
        let data = crate::data::tests::en();
        assert_eq!(
            fuzzy_find_heist_gem(data, "Anomalous Empowered Sparks I"),
            Some(FuzzyGem { name: "Empowered Sparks I".to_owned(), alt_quality: "Anomalous" })
        );
        assert_eq!(
            fuzzy_find_heist_gem(data, "anomalus empowered sparks i"),
            Some(FuzzyGem { name: "Empowered Sparks I".to_owned(), alt_quality: "Anomalous" })
        );
        // Case does not matter: the comparison lowercases both sides.
        assert_eq!(
            fuzzy_find_heist_gem(data, "PHANTASMAL EMPOWERED SPARKS II"),
            Some(FuzzyGem { name: "Empowered Sparks II".to_owned(), alt_quality: "Phantasmal" })
        );
        // A gem whose name is not a variant of another one: the nearest wins outright.
        assert_eq!(
            fuzzy_find_heist_gem(data, "Divergent Spark"),
            Some(FuzzyGem { name: "Spark".to_owned(), alt_quality: "Divergent" })
        );
        assert_eq!(
            fuzzy_find_heist_gem(data, "Anomalous Firebal"),
            Some(FuzzyGem { name: "Fireball".to_owned(), alt_quality: "Anomalous" })
        );
        assert_eq!(
            fuzzy_find_heist_gem(data, "Phantasmal Lightning Arrow"),
            Some(FuzzyGem { name: "Lightning Arrow".to_owned(), alt_quality: "Phantasmal" })
        );
    }

    /// `slice(2, -2)` takes the `/^…$/` wrapper off the client-string regex, so an exact
    /// name is distance 0 rather than counting the anchors.
    #[test]
    fn the_quality_templates_lose_their_anchors() {
        assert_eq!(template("^Anomalous (.*)$"), "Anomalous (.*)");
        assert_eq!(template("Phantasmal (.*)"), "hantasmal (.*", "a blind slice, as in the reference");
    }

    #[test]
    fn type_filter_values_match_the_reference() {
        let values: Vec<&str> = TypeFilter::ALL.iter().map(|f| f.as_str()).collect();
        assert_eq!(values, ["gem", "unique", "base_item"]);
        assert_eq!(TypeFilter::DEFAULT, TypeFilter::BaseItem);
    }
}
