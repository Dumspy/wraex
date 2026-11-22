//! PoE1 game data: `ref/apt/renderer/src/assets/data/{index,interfaces}.ts`.
//!
//! Reads the files upstream ships in `renderer/public/data/<lang>/` through a
//! [`DataSource`]; the hash indexes are built in memory by `wraex_core::data`.
//! `item-drop.json` and `patrons.json` are not loaded yet: item-check and the
//! about page need them, not the parser.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use wraex_core::data::{ClientStrings, DataError, DataSource, DirSource, ItemTable, StatKeys, StatTable};

/// The languages upstream ships data for (`make-index-files.mjs`).
pub const LANGUAGES: [&str; 4] = ["en", "ru", "cmn-Hant", "ko"];

/// `ref/apt/renderer/public/data`, the data upstream ships.
pub fn reference_source() -> DirSource {
    DirSource::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/apt/renderer/public/data"))
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatMatcher {
    pub string: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advanced: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negate: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
}

impl StatMatcher {
    fn is(&self, text: &str) -> bool {
        self.string == text || self.advanced.as_deref() == Some(text)
    }
}

/// `StatBetter`: which way a roll is better.
pub mod stat_better {
    pub const NEGATIVE_ROLL: i8 = -1;
    pub const POSITIVE_ROLL: i8 = 1;
    pub const NOT_COMPARABLE: i8 = 0;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    #[serde(rename = "ref")]
    pub stat_ref: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dp: Option<bool>,
    pub matchers: Vec<StatMatcher>,
    /// See [`stat_better`].
    pub better: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mod_family: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reminder_text: Option<String>,
    /// `"yes"`, `"ubermap_exclusive"` or `"heist_exclusive"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_area_mods: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anointments: Option<Vec<Anointment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mercenary: Option<MercenaryStat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jewellery_quality: Option<JewelleryQuality>,
    pub trade: StatTrade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anointment {
    pub roll: f64,
    pub oils: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MercenaryStat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canonical: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub synthetic_family: Option<bool>,
    /// In the data, not in `interfaces.ts`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tier: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JewelleryQuality {
    pub catalyst: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatTrade {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inverted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<bool>,
    /// Modifier type (`explicit`, `pseudo`, ...) to trade stat ids, in file order.
    pub ids: IndexMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatGroup {
    pub resolve: StatGroupResolver,
    pub stats: Vec<Stat>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "strat", rename_all = "kebab-case")]
pub enum StatGroupResolver {
    Select { test: Vec<Option<String>> },
    TrivialMerge,
    PercentMerge { kind: Vec<MergeKind> },
    FlagMerge { kind: Vec<MergeKind> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MergeKind {
    Percent,
    Value,
    Flag,
}

/// A `stats.ndjson` line. JavaScript tells them apart with `'stats' in x`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[expect(clippy::large_enum_variant, reason = "parsed per lookup and unpacked right away")]
pub enum StatOrGroup {
    Group(StatGroup),
    Stat(Stat),
}

impl StatOrGroup {
    /// The group's stats, or the stat itself.
    pub fn stats(&self) -> &[Stat] {
        match self {
            StatOrGroup::Group(g) => &g.stats,
            StatOrGroup::Stat(s) => std::slice::from_ref(s),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Namespace {
    DivinationCard,
    CapturedBeast,
    Unique,
    Item,
    Gem,
    Area,
    MercenaryBuild,
}

impl Namespace {
    pub const fn as_str(self) -> &'static str {
        match self {
            Namespace::DivinationCard => "DIVINATION_CARD",
            Namespace::CapturedBeast => "CAPTURED_BEAST",
            Namespace::Unique => "UNIQUE",
            Namespace::Item => "ITEM",
            Namespace::Gem => "GEM",
            Namespace::Area => "AREA",
            Namespace::MercenaryBuild => "MERCENARY_BUILD",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseType {
    pub name: String,
    pub ref_name: String,
    pub namespace: Namespace,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exchangeable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_disc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<Disc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub craftable: Option<Craftable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<UniqueInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area: Option<Area>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem: Option<GemInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armour: Option<ArmourInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mercenary_build: Option<MercenaryBuildRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Disc {
    #[serde(rename = "propAR", skip_serializing_if = "Option::is_none")]
    pub prop_ar: Option<bool>,
    #[serde(rename = "propEV", skip_serializing_if = "Option::is_none")]
    pub prop_ev: Option<bool>,
    #[serde(rename = "propES", skip_serializing_if = "Option::is_none")]
    pub prop_es: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_implicit: Option<StatRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_explicit: Option<StatRef>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub section_text: Option<String>,
    /// `"W"`, `"Y"` or `"R"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatRef {
    #[serde(rename = "ref")]
    pub stat_ref: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Craftable {
    /// An `ItemCategory` value; typed when `parser/meta.ts` is ported.
    pub category: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrupted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_only: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UniqueInfo {
    pub base: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_stats: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disenchant_value: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Area {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub special: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blighted: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GemInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfigured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal_variant: Option<String>,
    pub max_level: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmourInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ar: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ev: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub es: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ward: Option<[u32; 2]>,
}

/// `mercenaryBuild`: another build's `refName`, or the build itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MercenaryBuildRef {
    Ref(String),
    Build(MercenaryBuild),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MercenaryBuild {
    pub skills: Vec<MercenarySkill>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MercenarySkill {
    /// `"primary"`, `"secondary"` or `"utility"`.
    #[serde(rename = "type")]
    pub kind: String,
    pub name: String,
}

/// The keys `make-index-files.mjs` indexes a PoE1 stats line under: every stat of
/// a group, each matcher by `advanced` if set, else by `string`.
fn stat_keys(line: &StatOrGroup) -> StatKeys {
    let mut keys = StatKeys::default();
    for stat in line.stats() {
        keys.refs.push(stat.stat_ref.clone());
        for m in &stat.matchers {
            let key = m.advanced.as_deref().filter(|a| !a.is_empty()).unwrap_or(&m.string);
            keys.matchers.push(key.to_owned());
        }
    }
    keys
}

fn parse<T: for<'de> Deserialize<'de>>(line: &str) -> T {
    serde_json::from_str(line).expect("every line parsed when the tables were built")
}

/// One language's data, as `init(lang)` leaves the module state.
#[derive(Debug)]
pub struct Data {
    lang: String,
    /// `CLIENT_STRINGS`
    pub client_strings: ClientStrings,
    /// `CLIENT_STRINGS_REF`, always English.
    pub client_strings_ref: ClientStrings,
    items: ItemTable,
    stats: StatTable,
    altq_gem_names: Vec<String>,
    replica_unique_names: Vec<String>,
}

impl Data {
    /// `init(lang)` without the stat ref validation, which needs the refs the
    /// ported modules use (see [`Data::check_stat_refs`]).
    pub fn load(source: &dyn DataSource, lang: &str) -> Result<Self, DataError> {
        let client_strings_at = |lang: &str| {
            let path = format!("{lang}/client_strings.js");
            ClientStrings::parse(&path, &source.read_string(&path)?)
        };
        let client_strings_ref = client_strings_at("en")?;
        let client_strings = if lang == "en" { client_strings_ref.clone() } else { client_strings_at(lang)? };

        let path = format!("{lang}/items.ndjson");
        let items = ItemTable::new(&path, source.read_string(&path)?)?;
        let path = format!("{lang}/stats.ndjson");
        let stats = StatTable::new(&path, source.read_string(&path)?, stat_keys)?;

        let names = |includes| items.find_lines(includes, &[]).map(|l| parse::<BaseType>(l).name).collect();
        let altq_gem_names = names(r#"altQuality":["Anomalous"#);
        let replica_unique_names = names(r#"refName":"Replica"#);

        Ok(Self {
            lang: lang.to_owned(),
            client_strings,
            client_strings_ref,
            items,
            stats,
            altq_gem_names,
            replica_unique_names,
        })
    }

    pub fn lang(&self) -> &str {
        &self.lang
    }

    /// `CLIENT_STRINGS[key]` as text. Every language defines the same keys as
    /// English (tested), so a missing key is a bug in the caller.
    pub fn s(&self, key: &str) -> &str {
        self.client_strings.text(key).unwrap_or_else(|| panic!("client string {key} is not text"))
    }

    /// `CLIENT_STRINGS[key]` as a regex.
    pub fn re(&self, key: &str) -> &wraex_core::js_regex::Regex {
        self.client_strings.regex(key).unwrap_or_else(|| panic!("client string {key} is not a regex"))
    }

    /// `CLIENT_STRINGS[key]` as a list.
    pub fn list(&self, key: &str) -> &[String] {
        self.client_strings.list(key).unwrap_or_else(|| panic!("client string {key} is not a list"))
    }

    pub fn items(&self) -> &ItemTable {
        &self.items
    }

    pub fn stats(&self) -> &StatTable {
        &self.stats
    }

    /// `ITEM_BY_TRANSLATED`
    pub fn item_by_translated(&self, ns: Namespace, name: &str) -> Option<Vec<BaseType>> {
        Some(self.items.by_translated(ns.as_str(), name)?.into_iter().map(parse).collect())
    }

    /// `ITEM_BY_REF`
    pub fn item_by_ref(&self, ns: Namespace, name: &str) -> Option<Vec<BaseType>> {
        Some(self.items.by_ref(ns.as_str(), name)?.into_iter().map(parse).collect())
    }

    /// `ITEMS_ITERATOR`: searches the raw line text (no spaces after `:` here).
    pub fn items_iter<'a>(&'a self, includes: &'a str, and_includes: &'a [&'a str]) -> impl Iterator<Item = BaseType> {
        self.items.find_lines(includes, and_includes).map(parse)
    }

    /// `ALTQ_GEM_NAMES`
    pub fn altq_gem_names(&self) -> &[String] {
        &self.altq_gem_names
    }

    /// `REPLICA_UNIQUE_NAMES`
    pub fn replica_unique_names(&self) -> &[String] {
        &self.replica_unique_names
    }

    /// `STAT_BY_REF_V2`: the line the ref's hash points at, unverified.
    pub fn stat_by_ref_v2(&self, stat_ref: &str) -> Option<StatOrGroup> {
        self.stats.line_by_ref(stat_ref).map(parse)
    }

    /// `STAT_BY_MATCH_STR_V2`: `None` on a hash collision.
    pub fn stat_by_match_str_v2(&self, match_str: &str) -> Option<StatOrGroup> {
        let found: StatOrGroup = parse(self.stats.line_by_matcher(match_str)?);
        found.stats().iter().any(|s| s.matchers.iter().any(|m| m.is(match_str))).then_some(found)
    }

    /// `STAT_BY_MATCH_STR`: `None` when a group has no stat, or several, with this
    /// matcher.
    pub fn stat_by_match_str(&self, match_str: &str) -> Option<(Stat, StatMatcher)> {
        let stat = match self.stat_by_match_str_v2(match_str)? {
            StatOrGroup::Group(group) => {
                let mut stats = group.stats.into_iter().filter(|s| s.matchers.iter().any(|m| m.is(match_str)));
                match (stats.next(), stats.next()) {
                    (Some(stat), None) => stat,
                    _ => return None,
                }
            }
            StatOrGroup::Stat(stat) => stat,
        };
        let matcher = stat.matchers.iter().find(|m| m.is(match_str)).expect("checked above").clone();
        Some((stat, matcher))
    }

    /// `STATS_ITERATOR`: lines matching the raw text, groups flattened.
    pub fn stats_iter<'a>(&'a self, includes: &'a str, and_includes: &'a [&'a str]) -> impl Iterator<Item = Stat> {
        self.stats.find_lines(includes, and_includes).flat_map(|line| match parse(line) {
            StatOrGroup::Group(g) => g.stats,
            StatOrGroup::Stat(s) => vec![s],
        })
    }

    /// `pseudoStatByRef`
    pub fn pseudo_stat_by_ref(&self, stat_ref: &str) -> Option<Stat> {
        match self.stat_by_ref_v2(stat_ref)? {
            StatOrGroup::Group(g) => {
                g.stats.into_iter().find(|s| s.stat_ref == stat_ref && s.trade.ids.contains_key("pseudo"))
            }
            StatOrGroup::Stat(s) => Some(s),
        }
    }

    /// The `DELAYED_STAT_VALIDATION` check `init` runs over every ref passed to
    /// `stat()`: each must exist, and in English a group holding it must hold only
    /// that ref. Ported modules list their refs and test them through this.
    pub fn check_stat_refs<'a>(&self, refs: impl IntoIterator<Item = &'a str>) -> Result<(), String> {
        for text in refs {
            match self.stat_by_ref_v2(text) {
                None => return Err(format!("Cannot find stat: {text}")),
                Some(StatOrGroup::Group(g)) if self.lang == "en" && g.stats.iter().any(|s| s.stat_ref != text) => {
                    return Err(format!("Some stats have different ref text: {text}"));
                }
                Some(_) => {}
            }
        }
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::OnceLock;

    pub(crate) fn en() -> &'static Data {
        static EN: OnceLock<Data> = OnceLock::new();
        EN.get_or_init(|| Data::load(&reference_source(), "en").unwrap())
    }

    #[test]
    fn loads_every_language() {
        for lang in LANGUAGES {
            let data = Data::load(&reference_source(), lang).unwrap_or_else(|e| panic!("{lang}: {e}"));
            assert_eq!(data.client_strings_ref.text("RARITY_RARE"), Some("Rare"));
            assert!(data.client_strings.text("RARITY_RARE").is_some(), "{lang}");
            assert!(data.client_strings.regex("MAP_TIER").is_some(), "{lang}");
        }
    }

    /// [`Data::s`] and friends rely on this.
    #[test]
    fn every_language_has_the_english_client_string_keys() {
        let kind = |c: &wraex_core::data::ClientString| std::mem::discriminant(c);
        let en = en();
        for lang in LANGUAGES {
            let data = Data::load(&reference_source(), lang).unwrap();
            for (key, value) in en.client_strings.iter() {
                let other = data.client_strings.get(key).unwrap_or_else(|| panic!("{lang}: no {key}"));
                assert_eq!(kind(other), kind(value), "{lang}: {key}");
            }
        }
    }

    #[test]
    fn looks_up_items() {
        let data = en();
        let hh = data.item_by_ref(Namespace::Unique, "Headhunter").unwrap();
        assert_eq!(hh.len(), 1);
        assert_eq!(hh[0].unique.as_ref().unwrap().base, "Leather Belt");
        assert_eq!(
            data.item_by_translated(Namespace::Item, "Chaos Orb").unwrap()[0].trade_tag.as_deref(),
            Some("chaos")
        );
        assert!(data.item_by_ref(Namespace::Item, "Not An Item").is_none());
        assert!(!data.replica_unique_names().is_empty());
        assert!(data.replica_unique_names().iter().all(|n| n.starts_with("Replica ")));
        let gems: Vec<_> = data.items_iter(r#""namespace":"GEM""#, &["Empower"]).collect();
        assert!(gems.iter().any(|g| g.ref_name == "Empower Support"));
    }

    #[test]
    fn looks_up_stats() {
        let data = en();
        // Matcher strings carry no sign; the roll does.
        let (stat, matcher) = data.stat_by_match_str("# to maximum Life").unwrap();
        assert_eq!(stat.stat_ref, "+# to maximum Life");
        assert_eq!(matcher.string, "# to maximum Life");
        assert!(stat.trade.ids["explicit"].contains(&"explicit.stat_3299347043".to_owned()));
        assert!(data.stat_by_match_str("no such stat text").is_none());
        assert!(data.pseudo_stat_by_ref("+# total maximum Life").is_some());
        assert!(data.stats_iter("to maximum Life", &[]).any(|s| s.stat_ref == "+# to maximum Life"));
        data.check_stat_refs(["+# to maximum Life", "#% increased Attack Speed"]).unwrap();
        assert!(data.check_stat_refs(["no such stat"]).is_err());
    }

    /// Every line of every language reads into the typed schema and writes back
    /// unchanged, so a field the data gains or changes upstream fails here.
    #[test]
    fn the_schema_covers_every_line() {
        let source = reference_source();
        for lang in LANGUAGES {
            for (file, check) in [
                ("items", roundtrip::<BaseType> as fn(&str) -> Result<(), String>),
                ("stats", roundtrip::<StatOrGroup>),
            ] {
                let path = format!("{lang}/{file}.ndjson");
                for (n, line) in source.read_string(&path).unwrap().lines().enumerate() {
                    check(line).unwrap_or_else(|e| panic!("{path}:{}: {e}", n + 1));
                }
            }
        }
    }

    /// The indexes built here hold the same rows, in the same order, as the
    /// `*.index.bin` files `make-index-files.mjs` writes, so lookups land on the
    /// same line even when hashes collide.
    #[test]
    #[ignore = "needs the generated index files: devenv tasks run golden:prepare"]
    fn indexes_match_the_reference_index_files() {
        let source = reference_source();
        for lang in LANGUAGES {
            let data = Data::load(&source, lang).unwrap();
            for (file, ours, ndjson) in [
                ("items-name", data.items().name_index(), data.items().ndjson()),
                ("items-ref", data.items().ref_index(), data.items().ndjson()),
                ("stats-ref", data.stats().ref_index(), data.stats().ndjson()),
                ("stats-matcher", data.stats().matcher_index(), data.stats().ndjson()),
            ] {
                let path = format!("{lang}/{file}.index.bin");
                let reference = wraex_core::data::HashIndex::from_reference_bin(&source.read(&path).unwrap(), ndjson)
                    .unwrap_or_else(|| panic!("{path}: malformed"));
                assert!(ours.rows() == reference.rows(), "{path}: rows differ");
            }
        }
    }

    fn roundtrip<T: Serialize + for<'de> Deserialize<'de>>(line: &str) -> Result<(), String> {
        let original: serde_json::Value = serde_json::from_str(line).unwrap();
        let typed: T = serde_json::from_str(line).map_err(|e| e.to_string())?;
        let back = serde_json::to_value(&typed).unwrap();
        wraex_core::data::json_eq(&original, &back).then_some(()).ok_or_else(|| format!("round trip gives {back}"))
    }
}
