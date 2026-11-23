//! PoE2 game data: `ref/ee2/renderer/src/assets/data/{index,interfaces}.ts` and
//! the trade-site data from `src/web/background/TradeData.ts`.
//!
//! Reads the files EE2 ships in `renderer/public/data/<lang>/` through a
//! [`DataSource`]; the hash indexes are built in memory by `wraex_core::data`.
//! Not here yet: `TRADE_ITEM_BY_REF`, which builds items from `ItemCategory` and
//! `ItemRarity` and arrives with `parser/meta.ts`; `item-drop.json` and
//! `patrons.json`, which item-check and the about page need.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use wraex_core::data::{ClientStrings, DataError, DataSource, DirSource, ItemTable, StatKeys, StatTable, js_string};

/// The languages EE2 ships game data for (`make-index-files.mjs`). `th` has UI
/// text only.
pub const LANGUAGES: [&str; 9] = ["en", "ru", "cmn-Hant", "ko", "ja", "de", "es", "pt", "fr"];

/// `ref/ee2/renderer/public/data`, the data EE2 ships.
pub fn reference_source() -> DirSource {
    DirSource::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/ee2/renderer/public/data"))
}

/// EE2's snapshot of `/api/trade2/data/{items,stats}` (`renderer/specs/data/`),
/// which its tests and the golden harness serve instead of the trade site.
pub fn reference_trade_data() -> TradeData {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/ee2/renderer/specs/data");
    let read = |name: &str| std::fs::read_to_string(format!("{dir}/{name}.json")).unwrap();
    TradeData::from_json(&read("items"), &read("stats")).expect("EE2's trade data snapshot parses")
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
    /// Amulet anointment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oils: Option<String>,
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
    /// In the data, not in `interfaces.ts`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dp: Option<bool>,
    pub matchers: Vec<StatMatcher>,
    /// See [`stat_better`].
    pub better: i8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_area_mods: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_uber_area_mods: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from_heist_area_mods: Option<bool>,
    /// Ring anointments.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anointments: Option<Vec<Anointment>>,
    pub trade: StatTrade,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anointment {
    pub roll: f64,
    pub oils: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatTrade {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inverted: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<bool>,
    /// Modifier type (`explicit`, `rune`, ...) to trade stat ids, in file order.
    /// `null` for some stats, which the parser checks for (`!found.stat.trade.ids`).
    pub ids: Option<IndexMap<String, Vec<String>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Namespace {
    DivinationCard,
    CapturedBeast,
    Unique,
    Item,
    Gem,
}

impl Namespace {
    pub const fn as_str(self) -> &'static str {
        match self {
            Namespace::DivinationCard => "DIVINATION_CARD",
            Namespace::CapturedBeast => "CAPTURED_BEAST",
            Namespace::Unique => "UNIQUE",
            Namespace::Item => "ITEM",
            Namespace::Gem => "GEM",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BaseType {
    /// Missing on a few nameless uniques EE2 ships; JavaScript reads `undefined`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Missing where `name` is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
    pub namespace: Namespace,
    pub icon: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_disc: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disc: Option<Disc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub craftable: Option<Craftable>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<UniqueInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map: Option<MapInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem: Option<GemInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armour: Option<ArmourInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub augment: Option<Vec<AugmentLineData>>,
    pub tags: Vec<String>,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GemInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awakened: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transfigured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normal_variant: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArmourInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ar: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ev: Option<[u32; 2]>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub es: Option<[u32; 2]>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentLineData {
    /// `ItemCategory` values.
    pub categories: Vec<String>,
    pub string: String,
    pub values: Vec<f64>,
    /// `null` in the data for some lines, which `loadUltraLateItems` skips.
    #[serde(default, skip_serializing_if = "Option::is_none", deserialize_with = "present")]
    pub trade_id: Option<Option<Vec<String>>>,
    /// In the data, not in `interfaces.ts`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_bound: Option<bool>,
}

impl AugmentLineData {
    /// `tradeId` if it is truthy (present and not `null`).
    pub fn trade_ids(&self) -> Option<&[String]> {
        self.trade_id.as_ref()?.as_deref()
    }
}

/// Tells a `null` field from a missing one: `Some(None)` is `null`.
fn present<'de, T, D>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

/// `AugmentData`: one augment (rune, soul core, ...) stat, keyed by augment or by
/// trade id. Serialises with `interfaces.ts` names.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AugmentData {
    pub augment: String,
    pub ref_name: String,
    pub base_stat: String,
    pub values: Vec<f64>,
    pub id: String,
    pub categories: Vec<String>,
    pub icon: String,
}

/// `AugmentGroup<BaseType>`, as `groupAugments` sorts them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct AugmentGroups {
    pub rune: RuneGroups,
    pub legacy: Vec<BaseType>,
    pub soul_core: SoulCoreGroups,
    pub idol: Vec<BaseType>,
    pub other: Vec<BaseType>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct RuneGroups {
    pub lesser: Vec<BaseType>,
    pub normal: Vec<BaseType>,
    pub greater: Vec<BaseType>,
    pub perfect: Vec<BaseType>,
    pub other: Vec<BaseType>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct SoulCoreGroups {
    pub normal: Vec<BaseType>,
    pub special: Vec<BaseType>,
}

/// `CATALYST_TYPES`
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "PascalCase")]
pub struct CatalystTypes {
    pub normal: Vec<BaseType>,
    pub refined: Vec<BaseType>,
}

/// The trade site's item and stat lists (`/api/trade2/data/{items,stats}`), as
/// `useTradeData` reduces them. Always the English site: "nameplates are always
/// REF".
#[derive(Debug, Clone, Default)]
pub struct TradeData {
    /// `itemData`: every entry's `type` and `text`.
    items: std::collections::HashSet<String>,
    /// `statData`: stat text to modifier type to trade ids.
    stats: IndexMap<String, IndexMap<String, Vec<String>>>,
    /// `statDataSet`: every trade stat id.
    stat_ids: std::collections::HashSet<String>,
}

#[derive(Deserialize)]
struct TradeList<T> {
    result: Vec<TradeCategory<T>>,
}

#[derive(Deserialize)]
struct TradeCategory<T> {
    #[serde(default)]
    id: String,
    entries: Vec<T>,
}

#[derive(Deserialize)]
struct TradeItem {
    #[serde(default, rename = "type")]
    kind: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct TradeStat {
    id: String,
    text: String,
}

impl TradeData {
    /// `loadItemData` and `loadStatData` over the two responses. Like `load`, the
    /// result stays empty unless both lists have entries ("don't regress").
    pub fn from_json(items: &str, stats: &str) -> Result<Self, serde_json::Error> {
        let items: TradeList<TradeItem> = serde_json::from_str(items)?;
        let stats: TradeList<TradeStat> = serde_json::from_str(stats)?;
        let mut out = Self::default();
        for category in items.result {
            for entry in category.entries {
                out.items.extend([entry.kind, entry.text].into_iter().flatten().filter(|s| !s.is_empty()));
            }
        }
        for category in stats.result {
            for TradeStat { id, text } in category.entries {
                out.stats.entry(text).or_default().entry(category.id.clone()).or_default().push(id.clone());
                out.stat_ids.insert(id);
            }
        }
        if out.items.is_empty() || out.stat_ids.is_empty() {
            return Ok(Self::default());
        }
        Ok(out)
    }

    /// Whether the trade site lists an item with this `type` or `text`.
    pub fn has_item(&self, name: &str) -> bool {
        self.items.contains(name)
    }
}

/// `TRADE_ITEM_BY_REF`'s `itemQuery`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TradeItemQuery<'a> {
    pub base_type: Option<&'a str>,
    pub name: Option<&'a str>,
    pub rarity: Option<wraex_core::Rarity>,
    pub category: Option<&'a crate::parser::meta::ItemCategory>,
}

/// The keys `make-index-files.mjs` indexes a PoE2 stats line under: its ref, and
/// each matcher's `string` and, if set, `advanced`.
fn stat_keys(stat: &Stat) -> StatKeys {
    let mut keys = StatKeys { refs: vec![stat.stat_ref.clone()], matchers: Vec::new() };
    for m in &stat.matchers {
        keys.matchers.push(m.string.clone());
        if let Some(advanced) = m.advanced.as_deref().filter(|a| !a.is_empty()) {
            keys.matchers.push(advanced.to_owned());
        }
    }
    keys
}

fn parse<T: for<'de> Deserialize<'de>>(line: &str) -> T {
    serde_json::from_str(line).expect("every line parsed when the tables were built")
}

/// Catalyst trade id to the client-string tag keys it covers (`CATALYST_TO_TAG`).
const CATALYST_TAGS: [(&str, &[&str]); 13] = [
    ("life_catalyst", &["LIFE_TAG"]),
    ("mana_catalyst", &["MANA_TAG"]),
    ("defences_catalyst", &["ARMOUR_TAG", "EVASION_TAG", "ENERGY_SHIELD_TAG"]),
    ("physical_catalyst", &["PHYSICAL_TAG"]),
    ("fire_catalyst", &["FIRE_TAG"]),
    ("cold_catalyst", &["COLD_TAG"]),
    ("lightning_catalyst", &["LIGHTNING_TAG"]),
    ("chaos_catalyst", &["CHAOS_TAG"]),
    ("attack_catalyst", &["ATTACK_TAG"]),
    ("caster_catalyst", &["CASTER_TAG"]),
    ("speed_catalyst", &["SPEED_TAG"]),
    ("attribute_catalyst", &["ATTRIBUTE_TAG"]),
    ("minion_catalyst", &["MINION_TAG"]),
];

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
    gem_ns_names: Vec<String>,
    unique_ns_names: Vec<String>,
    item_ns_names: Vec<String>,
    trade_tag_to_ref: IndexMap<String, String>,
    augment_data_by_augment: IndexMap<String, Vec<AugmentData>>,
    augment_data_by_trade_id: IndexMap<String, Vec<AugmentData>>,
    grouped_augments: AugmentGroups,
    catalyst_types: CatalystTypes,
    catalyst_to_tag: IndexMap<String, Vec<String>>,
    tag_to_catalyst: IndexMap<String, String>,
    trade: Option<TradeData>,
}

impl Data {
    /// `init(lang)` without the stat ref validation, which needs the refs the
    /// ported modules use (see [`Data::check_stat_refs`]). `trade` is the trade
    /// site's data; without it, lookups against it find nothing, as when
    /// `loadTradeData` fails.
    pub fn load(source: &dyn DataSource, lang: &str, trade: Option<TradeData>) -> Result<Self, DataError> {
        let client_strings_at = |lang: &str| {
            let path = format!("{lang}/client_strings.js");
            ClientStrings::parse(&path, &source.read_string(&path)?)
        };
        let client_strings_ref = client_strings_at("en")?;
        let client_strings = if lang == "en" { client_strings_ref.clone() } else { client_strings_at(lang)? };

        let items_path = format!("{lang}/items.ndjson");
        let items = ItemTable::new(&items_path, source.read_string(&items_path)?)?;
        let path = format!("{lang}/stats.ndjson");
        let stats = StatTable::new(&path, source.read_string(&path)?, stat_keys)?;

        let lines = |includes| items.find_lines(includes, &[]).map(parse::<BaseType>);
        // `cached += item.name + "\n"`: nameless items add "undefined".
        let names = |includes| lines(includes).map(|item| js_string(&item.name).to_owned()).collect();
        let gem_ns_names = names(r#"": "GEM""#);
        let unique_ns_names = names(r#"": "UNIQUE""#);
        let item_ns_names = names(r#"": "ITEM""#);
        let mut trade_tag_to_ref = IndexMap::new();
        for item in lines(r#""tradeTag":"#) {
            let ref_name = js_string(&item.ref_name).to_owned();
            trade_tag_to_ref.insert(item.trade_tag.expect("searched for tradeTag"), ref_name);
        }

        // `loadUltraLateItems`
        let augment_list: Vec<BaseType> = lines(r#""craftable": {"category": "SoulCore"}"#)
            .filter(|r| r.augment.as_ref().is_some_and(|a| a.iter().any(|s| s.trade_ids().is_some())))
            .map(|mut r| {
                r.augment.as_mut().unwrap().retain(|s| s.trade_ids().is_some());
                r
            })
            .collect();
        let (augment_data_by_augment, augment_data_by_trade_id) = augments_to_lookups(&augment_list)
            .map_err(|message| DataError::Syntax { path: items_path.clone(), line: 0, message })?;
        let grouped_augments = group_augments(augment_list);
        let catalyst_types = CatalystTypes {
            normal: lines(r#""tags": ["catalyst""#).collect(),
            refined: lines(r#""tags": ["jewel_catalyst""#).collect(),
        };
        let mut catalyst_to_tag = IndexMap::new();
        let mut tag_to_catalyst = IndexMap::new();
        for (catalyst, keys) in CATALYST_TAGS {
            let mut tags = Vec::new();
            for key in keys {
                let tag = client_strings.text(key).ok_or_else(|| DataError::Syntax {
                    path: format!("{lang}/client_strings.js"),
                    line: 0,
                    message: format!("missing {key}"),
                })?;
                tags.push(tag.to_owned());
                tag_to_catalyst.insert(tag.to_owned(), catalyst.to_owned());
            }
            catalyst_to_tag.insert(catalyst.to_owned(), tags);
        }

        Ok(Self {
            lang: lang.to_owned(),
            client_strings,
            client_strings_ref,
            items,
            stats,
            gem_ns_names,
            unique_ns_names,
            item_ns_names,
            trade_tag_to_ref,
            augment_data_by_augment,
            augment_data_by_trade_id,
            grouped_augments,
            catalyst_types,
            catalyst_to_tag,
            tag_to_catalyst,
            trade,
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

    /// `CLIENT_STRINGS_REF[key]` (English) as text.
    pub fn s_ref(&self, key: &str) -> &str {
        self.client_strings_ref.text(key).unwrap_or_else(|| panic!("client string {key} is not text"))
    }

    /// `CLIENT_STRINGS_REF[key]` (English) as a regex.
    pub fn re_ref(&self, key: &str) -> &wraex_core::js_regex::Regex {
        self.client_strings_ref.regex(key).unwrap_or_else(|| panic!("client string {key} is not a regex"))
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

    /// `ITEMS_ITERATOR`: searches the raw line text, which has a space after
    /// every `:` and `,` in EE2's files.
    pub fn items_iter<'a>(&'a self, includes: &'a str, and_includes: &'a [&'a str]) -> impl Iterator<Item = BaseType> {
        self.items.find_lines(includes, and_includes).map(parse)
    }

    /// `GEM_NS_NAMES`
    pub fn gem_ns_names(&self) -> &[String] {
        &self.gem_ns_names
    }

    /// `UNIQUE_NS_NAMES`
    pub fn unique_ns_names(&self) -> &[String] {
        &self.unique_ns_names
    }

    /// `ITEM_NS_NAMES`
    pub fn item_ns_names(&self) -> &[String] {
        &self.item_ns_names
    }

    /// `TRADE_TAG_TO_REF`
    pub fn trade_tag_to_ref(&self, trade_tag: &str) -> Option<&str> {
        self.trade_tag_to_ref.get(trade_tag).map(String::as_str)
    }

    /// `STAT_BY_REF`: the line the ref's hash points at, unverified.
    pub fn stat_by_ref(&self, stat_ref: &str) -> Option<Stat> {
        self.stats.line_by_ref(stat_ref).map(parse)
    }

    /// `STAT_BY_MATCH_STR`: `None` on a hash collision.
    pub fn stat_by_match_str(&self, match_str: &str) -> Option<(Stat, StatMatcher)> {
        let stat: Stat = parse(self.stats.line_by_matcher(match_str)?);
        let matcher = stat.matchers.iter().find(|m| m.is(match_str))?.clone();
        Some((stat, matcher))
    }

    /// `STATS_ITERATOR`
    pub fn stats_iter<'a>(&'a self, includes: &'a str, and_includes: &'a [&'a str]) -> impl Iterator<Item = Stat> {
        self.stats.find_lines(includes, and_includes).map(parse)
    }

    /// `AUGMENT_DATA_BY_AUGMENT[refName]`
    pub fn augment_data_by_augment(&self, ref_name: &str) -> Option<&[AugmentData]> {
        self.augment_data_by_augment.get(ref_name).map(Vec::as_slice)
    }

    /// `AUGMENT_DATA_BY_TRADE_ID[tradeId]`
    pub fn augment_data_by_trade_id(&self, trade_id: &str) -> Option<&[AugmentData]> {
        self.augment_data_by_trade_id.get(trade_id).map(Vec::as_slice)
    }

    /// `GROUPED_AUGMENTS`
    pub fn grouped_augments(&self) -> &AugmentGroups {
        &self.grouped_augments
    }

    /// `CATALYST_TYPES`
    pub fn catalyst_types(&self) -> &CatalystTypes {
        &self.catalyst_types
    }

    /// `CATALYST_TO_TAG[catalyst]`: the translated tags a catalyst improves.
    pub fn catalyst_to_tag(&self, catalyst: &str) -> Option<&[String]> {
        self.catalyst_to_tag.get(catalyst).map(Vec::as_slice)
    }

    /// `TAG_TO_CATALYST[tag]`
    pub fn tag_to_catalyst(&self, tag: &str) -> Option<&str> {
        self.tag_to_catalyst.get(tag).map(String::as_str)
    }

    /// The trade site's data, if it loaded.
    pub fn trade(&self) -> Option<&TradeData> {
        self.trade.as_ref()
    }

    /// `TRADE_ITEM_BY_REF`: a stand-in `BaseType` for an item the trade site
    /// lists but the shipped data lacks. `None` without trade data, as when
    /// `loadTradeData` fails.
    pub fn trade_item_by_ref(&self, query: &TradeItemQuery<'_>, force_craftable: bool) -> Option<Vec<BaseType>> {
        use crate::parser::meta::{GEM, ItemCategory};
        let trade = self.trade.as_ref()?;
        let stub = |name: &str, namespace| BaseType {
            name: Some(name.to_owned()),
            ref_name: Some(name.to_owned()),
            namespace,
            icon: "%NOT_FOUND%".to_owned(),
            w: None,
            h: None,
            trade_tag: None,
            trade_disc: None,
            disc: None,
            craftable: None,
            unique: None,
            map: None,
            gem: None,
            armour: None,
            augment: None,
            tags: Vec::new(),
        };
        let craftable =
            |category: &str| Craftable { category: category.to_owned(), corrupted: None, unique_only: None };
        let name = query.name.filter(|n| !n.is_empty());
        let base = if query.category.is_some_and(|c| GEM.contains(c)) {
            name.filter(|n| trade.has_item(n))
                .map(|n| BaseType { gem: Some(GemInfo::default()), ..stub(n, Namespace::Gem) })
        } else if query.rarity == Some(wraex_core::Rarity::Unique) {
            // `${name} ${baseType}`; an undefined base type prints as "undefined".
            let base_type = query.base_type.unwrap_or("undefined");
            name.filter(|n| trade.has_item(&format!("{n} {base_type}"))).map(|n| BaseType {
                unique: Some(UniqueInfo { base: base_type.to_owned(), fixed_stats: None }),
                ..stub(n, Namespace::Unique)
            })
        } else if query.base_type.is_none_or(str::is_empty) {
            // TODO (upstream): currency works without tradeTag, just ninja only
            name.filter(|n| trade.has_item(n)).map(|n| {
                let category = match query.category {
                    Some(c) => Some(craftable(c.as_str())),
                    None if force_craftable => Some(craftable(n)),
                    None => None,
                };
                BaseType { craftable: category, ..stub(n, Namespace::Item) }
            })
        } else {
            let base_type = query.base_type.unwrap();
            trade.has_item(base_type).then(|| BaseType {
                craftable: Some(craftable(ItemCategory::Unknown.as_str())),
                ..stub(base_type, Namespace::Item)
            })
        };
        base.map(|b| vec![b])
    }

    /// `TRADE_STAT_BY_STAT_ID`
    pub fn trade_stat_by_stat_id(&self, trade_id: &str) -> bool {
        self.trade.as_ref().is_some_and(|t| t.stat_ids.contains(trade_id))
    }

    /// `TRADE_STAT_BY_MATCH_STR`: modifier type to trade ids for a trade-site stat text.
    pub fn trade_stat_by_match_str(&self, text: &str) -> Option<&IndexMap<String, Vec<String>>> {
        self.trade.as_ref()?.stats.get(text)
    }

    /// The `DELAYED_STAT_VALIDATION` check `init` runs over every ref passed to
    /// `stat()`. EE2 only logs missing refs; this returns them, and ported modules
    /// test their refs through it.
    pub fn check_stat_refs<'a>(&self, refs: impl IntoIterator<Item = &'a str>) -> Result<(), Vec<String>> {
        let missing: Vec<String> =
            refs.into_iter().filter(|text| self.stat_by_ref(text).is_none()).map(str::to_owned).collect();
        if missing.is_empty() { Ok(()) } else { Err(missing) }
    }
}

type AugmentLookup = IndexMap<String, Vec<AugmentData>>;

/// `augmentsToLookup` and `augmentsToLookupTradeId`. The reference takes
/// `tradeId[0]`; an empty list would give `id: undefined`, so it is an error here
/// (no shipped data has one).
fn augments_to_lookups(augments: &[BaseType]) -> Result<(AugmentLookup, AugmentLookup), String> {
    let (mut by_augment, mut by_trade_id) = (AugmentLookup::new(), AugmentLookup::new());
    for augment in augments {
        for stat in augment.augment.iter().flatten() {
            let Some(trade_ids) = stat.trade_ids() else { continue };
            let ref_name = js_string(&augment.ref_name);
            let id = trade_ids.first().ok_or_else(|| format!("{ref_name}: empty tradeId"))?;
            let data = AugmentData {
                augment: js_string(&augment.name).to_owned(),
                ref_name: ref_name.to_owned(),
                base_stat: stat.string.clone(),
                values: stat.values.clone(),
                id: id.clone(),
                categories: stat.categories.clone(),
                icon: augment.icon.clone(),
            };
            by_augment.entry(ref_name.to_owned()).or_default().push(data.clone());
            by_trade_id.entry(id.clone()).or_default().push(data);
        }
    }
    Ok((by_augment, by_trade_id))
}

/// `groupAugments`, by English `refName`.
fn group_augments(augments: Vec<BaseType>) -> AugmentGroups {
    let normal_runes: std::collections::HashSet<String> = augments
        .iter()
        .map(|a| js_string(&a.ref_name).to_owned())
        .filter(|r| {
            if !r.contains("Rune") || r.starts_with("Greater") || r.starts_with("Perfect") || r.starts_with("Lesser") {
                return false;
            }
            let split: Vec<&str> = r.split(' ').collect();
            split.len() == 2 && split[1] == "Rune"
        })
        .collect();

    let mut g = AugmentGroups::default();
    for augment in augments {
        let r = js_string(&augment.ref_name).to_owned();
        let r = r.as_str();
        if r.contains("Rune") {
            let not_first = r.split(' ').skip(1).collect::<Vec<_>>().join(" ");
            if normal_runes.contains(&not_first) || normal_runes.contains(r) {
                if r.starts_with("Greater") {
                    g.rune.greater.push(augment);
                } else if r.starts_with("Perfect") {
                    g.rune.perfect.push(augment);
                } else if r.starts_with("Lesser") {
                    g.rune.lesser.push(augment);
                } else {
                    g.rune.normal.push(augment);
                }
            } else {
                g.rune.other.push(augment);
            }
        } else if r.starts_with("Legacy") {
            g.legacy.push(augment);
        } else if r.contains("Soul Core") {
            if r.starts_with("Soul Core") {
                g.soul_core.normal.push(augment);
            } else {
                g.soul_core.special.push(augment);
            }
        } else if r.contains("Thesis") {
            g.soul_core.special.push(augment);
        } else if r.contains("Idol") {
            g.idol.push(augment);
        } else {
            g.other.push(augment);
        }
    }
    g
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::OnceLock;

    pub(crate) fn en() -> &'static Data {
        static EN: OnceLock<Data> = OnceLock::new();
        EN.get_or_init(|| Data::load(&reference_source(), "en", Some(reference_trade_data())).unwrap())
    }

    #[test]
    fn loads_every_language() {
        for lang in LANGUAGES {
            let data = Data::load(&reference_source(), lang, None).unwrap_or_else(|e| panic!("{lang}: {e}"));
            assert_eq!(data.client_strings_ref.text("RARITY_RARE"), Some("Rare"));
            assert!(data.client_strings.text("RARITY_RARE").is_some(), "{lang}");
            assert!(!data.gem_ns_names().is_empty(), "{lang}");
            assert!(data.augment_data_by_augment("Iron Rune").is_some(), "{lang}");
            assert!(!data.trade_stat_by_stat_id("explicit.stat_3299347043"));
        }
    }

    /// [`Data::s`] and friends rely on this.
    #[test]
    fn every_language_has_the_english_client_string_keys() {
        let kind = |c: &wraex_core::data::ClientString| std::mem::discriminant(c);
        let en = en();
        for lang in LANGUAGES {
            let data = Data::load(&reference_source(), lang, None).unwrap();
            for (key, value) in en.client_strings.iter() {
                let other = data.client_strings.get(key).unwrap_or_else(|| panic!("{lang}: no {key}"));
                assert_eq!(kind(other), kind(value), "{lang}: {key}");
            }
        }
    }

    #[test]
    fn stands_in_for_items_only_the_trade_site_lists() {
        let d = en();
        let query = TradeItemQuery { name: Some("Crimson Amulet"), ..Default::default() };
        let item = &d.trade_item_by_ref(&query, true).unwrap()[0];
        assert_eq!(
            (item.icon.as_str(), item.craftable.as_ref().unwrap().category.as_str()),
            ("%NOT_FOUND%", "Crimson Amulet")
        );
        assert!(
            d.trade_item_by_ref(&TradeItemQuery { name: Some("Not Listed"), ..Default::default() }, false).is_none()
        );
    }

    /// `specs/data/dataLoader.test.ts`
    #[test]
    fn random_stats_are_present() {
        let data = en();
        assert!(augments_to_lookups(&[]).is_ok());
        data.check_stat_refs([
            "Adds # to # Physical Damage",
            "Adds # to # Lightning Damage",
            "#% to Fire Resistance",
            "Knockback direction is reversed",
            "Regenerate # Life per second per Maximum Energy Shield",
            "Increases and Reductions to Minion Attack Speed also affect you",
            "Notable Passive Skills in Radius also grant Projectiles have #% chance for an additional Projectile when Forking",
            "Every Rage also grants #% increased Armour",
            "Recover #% of maximum Life for each Endurance Charge consumed",
            "#% increased Freeze Buildup",
            "Has Purple Smoke",
            "On Corruption, Item gains two Enchantments",
        ])
        .unwrap();
        assert_eq!(data.check_stat_refs(["no such stat"]), Err(vec!["no such stat".to_owned()]));
    }

    #[test]
    fn looks_up_items_and_stats() {
        let data = en();
        let item = &data.item_by_ref(Namespace::Item, "Exalted Orb").unwrap()[0];
        assert_eq!(item.trade_tag.as_deref(), Some("exalted"));
        assert_eq!(data.trade_tag_to_ref("exalted"), Some("Exalted Orb"));
        let (stat, matcher) = data.stat_by_match_str("# to maximum Life").unwrap();
        assert_eq!(stat.stat_ref, "# to maximum Life");
        assert_eq!(matcher.string, "# to maximum Life");
        assert!(data.stat_by_match_str("no such stat text").is_none());
        assert!(data.stats_iter("to maximum Life", &[]).any(|s| s.stat_ref == "# to maximum Life"));
    }

    #[test]
    fn derives_augment_and_catalyst_tables() {
        let data = en();
        let iron = data.augment_data_by_augment("Iron Rune").unwrap();
        assert!(iron.iter().all(|a| a.augment == "Iron Rune" && !a.categories.is_empty()));
        let by_id = data.augment_data_by_trade_id(&iron[0].id).unwrap();
        assert!(by_id.iter().any(|a| a.ref_name == "Iron Rune"));
        let groups = data.grouped_augments();
        assert!(groups.rune.normal.iter().any(|a| a.ref_name.as_deref() == Some("Iron Rune")));
        assert!(groups.rune.greater.iter().all(|a| a.ref_name.as_ref().unwrap().starts_with("Greater")));
        assert!(groups.soul_core.normal.iter().all(|a| a.ref_name.as_ref().unwrap().starts_with("Soul Core")));
        assert!(!data.catalyst_types().normal.is_empty());
        assert_eq!(data.catalyst_to_tag("defences_catalyst").unwrap(), ["Armour", "Evasion", "Energy Shield"]);
        assert_eq!(data.tag_to_catalyst("Evasion"), Some("defences_catalyst"));
    }

    #[test]
    fn reads_the_trade_site_data() {
        let data = en();
        let trade = data.trade().unwrap();
        assert!(trade.has_item("Crimson Amulet"));
        assert!(data.trade_stat_by_stat_id("pseudo.pseudo_total_cold_resistance"));
        let ids = data.trade_stat_by_match_str("+#% total to Cold Resistance").unwrap();
        assert_eq!(ids["pseudo"], ["pseudo.pseudo_total_cold_resistance"]);
        // "Don't regress": an empty list leaves the data empty.
        let empty = TradeData::from_json(r#"{"result":[]}"#, r#"{"result":[]}"#).unwrap();
        assert!(!empty.has_item("Crimson Amulet"));
    }

    /// Every line of every language reads into the typed schema and writes back
    /// unchanged, so a field the data gains or changes upstream fails here.
    #[test]
    fn the_schema_covers_every_line() {
        let source = reference_source();
        for lang in LANGUAGES {
            for (file, check) in
                [("items", roundtrip::<BaseType> as fn(&str) -> Result<(), String>), ("stats", roundtrip::<Stat>)]
            {
                let path = format!("{lang}/{file}.ndjson");
                for (n, line) in source.read_string(&path).unwrap().lines().enumerate() {
                    check(line).unwrap_or_else(|e| panic!("{path}:{}: {e}", n + 1));
                }
            }
        }
    }

    /// See the PoE1 test of the same name.
    #[test]
    #[ignore = "needs the generated index files: devenv tasks run golden:prepare"]
    fn indexes_match_the_reference_index_files() {
        let source = reference_source();
        for lang in LANGUAGES {
            let data = Data::load(&source, lang, None).unwrap();
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
