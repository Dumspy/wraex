//! `ref/apt/renderer/src/web/price-check/filters`: the default filters a price
//! check starts with ([`create_presets`]).
//!
//! The reference asserts some item fields with `!` (an area level on a Chronicle
//! of Atzoatl, a map area on a Scrying Orb). Where such a field is missing, it
//! carries `undefined` along or throws; here it is a [`FilterError`] instead.

pub mod create_item_filters;
pub mod create_presets;
pub mod create_stat_filters;
pub mod pseudo;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

pub use create_presets::{CreatePresetsOptions, Presets, create_presets};

use crate::data::Stat;
use crate::parser::item::ItemInfluence;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::{ModifierMechanic, ModifierType, StatSource};

/// Something the reference throws on (`Unexpected stat shape: ...`), or a field
/// it asserts that the item lacks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FilterError(pub String);

impl std::fmt::Display for FilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for FilterError {}

pub(crate) type R<T> = Result<T, FilterError>;

pub(crate) fn missing(what: &str) -> FilterError {
    FilterError(format!("missing {what}"))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilterPreset {
    pub id: String,
    pub filters: ItemFilters,
    pub stats: Vec<FilterOrGroup>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_trade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type_trade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator_trade: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ItemCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<Box<SearchFilter>>,
    /// Only on `sub` and `searchRelaxed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemFilters {
    pub search_exact: SearchFilter,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_relaxed: Option<SearchFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<RarityFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_sockets: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub white_sockets: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrupted: Option<CorruptedFilter>,
    /// `{ value: false }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fractured: Option<ValueFilter<bool>>,
    /// `{ disabled: true }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imbued_gem: Option<Disabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirrored: Option<HiddenFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub split: Option<HiddenFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_catalysed: Option<Disabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foil: Option<Disabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foulborn: Option<ValueFilter<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vestigial: Option<ValueFilter<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub influences: Option<Vec<InfluenceFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_blighted: Option<ValueFilter<MapBlighted>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_completion_reward: Option<MapCompletionReward>,
    /// An Inscribed Ultimatum's four trade options, which upstream has no support for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultimatum: Option<UltimatumFilters>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<FilterNumeric>,
    /// `{ value: true, disabled }`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unidentified: Option<UnidentifiedFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veiled: Option<VeiledFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_charge: Option<FilterNumeric>,
    pub trade: TradeFilters,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RarityFilter {
    /// `magic` or `nonunique`.
    pub value: &'static str,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorruptedFilter {
    pub value: bool,
    pub twice: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ValueFilter<T> {
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Disabled {
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HiddenFilter {
    pub disabled: bool,
    pub hidden: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InfluenceFilter {
    pub value: ItemInfluence,
    pub disabled: bool,
}

/// `'Blighted' | 'Blight-ravaged' | false`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapBlighted {
    Blighted,
    BlightRavaged,
    No,
}

impl Serialize for MapBlighted {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            MapBlighted::Blighted => s.serialize_str("Blighted"),
            MapBlighted::BlightRavaged => s.serialize_str("Blight-ravaged"),
            MapBlighted::No => s.serialize_bool(false),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MapCompletionReward {
    pub name: String,
    pub name_trade: String,
}

/// The `ultimatum_*` filters an Inscribed Ultimatum offers (`filters.ultimatum`). Upstream
/// has no Ultimatum support, so this is the port's own shape; the trade request writes each
/// enabled filter's `value` as its option.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UltimatumFilters {
    /// `ultimatum_challenge`: off by default, since the challenge is not what a buyer prices.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub challenge: Option<UltimatumFilter>,
    /// `ultimatum_reward`: `DoubleCurrency`, `DoubleDivCards`, `MirrorRare` or `ExchangeUnique`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reward: Option<UltimatumFilter>,
    /// `ultimatum_input`: the item the Ultimatum demands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<UltimatumFilter>,
    /// `ultimatum_output`: the unique an `ExchangeUnique` reward pays out.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<UltimatumFilter>,
}

/// One `ultimatum_*` filter: the trade site's option (a trade id, or an item's name) and the
/// label its chip shows.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UltimatumFilter {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

impl UltimatumFilter {
    fn new(value: impl Into<String>, label: impl Into<String>, disabled: bool) -> Self {
        Self { value: value.into(), label: label.into(), disabled }
    }
}

impl From<&crate::ultimatum::Ultimatum> for UltimatumFilters {
    fn from(u: &crate::ultimatum::Ultimatum) -> Self {
        let challenge = u.challenge.trade_id().map(|id| UltimatumFilter::new(id, u.challenge.label(), true));
        let reward = u.reward.trade_id().map(|id| UltimatumFilter::new(id, u.reward.label(), false));
        let input = (!u.sacrifice.name.is_empty())
            .then(|| UltimatumFilter::new(u.sacrifice.name.as_str(), u.sacrifice.name.as_str(), false));
        let output = match &u.reward {
            crate::ultimatum::Reward::ExchangeUnique(name) => Some(UltimatumFilter::new(name, name, false)),
            _ => None,
        };
        Self { challenge, reward, input, output }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct UnidentifiedFilter {
    pub value: bool,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeiledFilter {
    pub stat_refs: Vec<String>,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeFilters {
    pub offline: bool,
    pub online_in_league: bool,
    pub merchant_only: bool,
    pub listed: Option<String>,
    pub currency: Option<String>,
    pub league: String,
    /// `app` or `api`.
    pub collapse_listings: String,
    pub collapse_merchant: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilterNumeric {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    pub disabled: bool,
}

impl FilterNumeric {
    pub fn new(value: f64, disabled: bool) -> Self {
        Self { value, max: None, disabled }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum FilterOrGroup {
    Stat(StatFilter),
    Group(FilterGroup),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GroupKind {
    Not,
    One,
    Mercenary,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilterGroup {
    pub group: GroupKind,
    pub expanded: bool,
    pub meta: StatFilter,
    pub stats: Vec<StatFilter>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatFilter {
    /// `stat.trade.ids[type]`, which the stat may not have.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_id: Option<Vec<String>>,
    pub stat_ref: String,
    pub text: String,
    pub tag: FilterTag,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// `undefined` entries (no such oil) are written as `null`.
    pub oils: Option<Vec<Option<&'static str>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mercenary: Option<MercenaryFilterInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reminder_text: Option<String>,
    pub sources: Vec<FilterSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<FilterRoll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<ValueFilter<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<&'static str>,
    pub disabled: bool,
}

impl StatFilter {
    /// `filter.tradeId[0]`
    pub fn first_trade_id(&self) -> Option<&str> {
        self.trade_id.as_ref().and_then(|ids| ids.first()).map(String::as_str)
    }

    /// The sources that are modifier stats (all but a mercenary's 6-link filter).
    pub fn stat_sources(&self) -> impl Iterator<Item = &StatSource> {
        self.sources.iter().filter_map(|s| match s {
            FilterSource::Stat(s) => Some(s),
            FilterSource::Family(_) => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MercenaryFilterInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tier: Option<bool>,
}

/// `StatFilter['sources']`. A mercenary's 6-link filter stores the support gem
/// families as `{ modifier: { stats: [{ stat }] } }` (`encodeFamilyToSource`).
#[derive(Debug, Clone, PartialEq)]
#[expect(clippy::large_enum_variant, reason = "nearly every source is a stat; families are rare")]
pub enum FilterSource {
    Stat(StatSource),
    Family(Vec<Stat>),
}

impl Serialize for FilterSource {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            FilterSource::Stat(source) => source.serialize(s),
            FilterSource::Family(family) => {
                #[derive(Serialize)]
                struct FamilyStat<'a> {
                    stat: &'a Stat,
                }
                #[derive(Serialize)]
                struct Modifier<'a> {
                    stats: Vec<FamilyStat<'a>>,
                }
                let mut map = s.serialize_map(Some(1))?;
                let stats = family.iter().map(|stat| FamilyStat { stat }).collect();
                map.serialize_entry("modifier", &Modifier { stats })?;
                map.end()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MinMax {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FilterRoll {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    pub default: MinMax,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<MinMax>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade_invert: Option<bool>,
    pub dp: bool,
    pub is_negated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goodness: Option<f64>,
}

/// `FilterTag`: a modifier type, a modifier mechanic, or one of the extras.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTag {
    Type(ModifierType),
    Mechanic(ModifierMechanic),
    Variant,
    Property,
    Brick,
    MercenaryPrimary,
    MercenarySecondary,
    MercenaryUtility,
    MercenarySupport,
    FilterGroup,
}

impl FilterTag {
    pub const PSEUDO: FilterTag = FilterTag::Type(ModifierType::Pseudo);
    pub const EXPLICIT: FilterTag = FilterTag::Type(ModifierType::Explicit);
    pub const IMPLICIT: FilterTag = FilterTag::Type(ModifierType::Implicit);
    pub const ENCHANT: FilterTag = FilterTag::Type(ModifierType::Enchant);
    pub const FRACTURED: FilterTag = FilterTag::Type(ModifierType::Fractured);

    /// The tag's value, as [`Serialize`] writes it and as `filters.tag_<tag>` spells its key.
    pub fn as_str(self) -> &'static str {
        match self {
            FilterTag::Type(t) => t.as_str(),
            FilterTag::Mechanic(m) => m.as_str(),
            FilterTag::Variant => "variant",
            FilterTag::Property => "property",
            FilterTag::Brick => "brick",
            FilterTag::MercenaryPrimary => "mercenary-primary",
            FilterTag::MercenarySecondary => "mercenary-secondary",
            FilterTag::MercenaryUtility => "mercenary-utility",
            FilterTag::MercenarySupport => "mercenary-support",
            FilterTag::FilterGroup => "filter-group",
        }
    }
}

impl Serialize for FilterTag {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            FilterTag::Type(t) => t.serialize(s),
            FilterTag::Mechanic(m) => m.serialize(s),
            FilterTag::Variant => s.serialize_str("variant"),
            FilterTag::Property => s.serialize_str("property"),
            FilterTag::Brick => s.serialize_str("brick"),
            FilterTag::MercenaryPrimary => s.serialize_str("mercenary-primary"),
            FilterTag::MercenarySecondary => s.serialize_str("mercenary-secondary"),
            FilterTag::MercenaryUtility => s.serialize_str("mercenary-utility"),
            FilterTag::MercenarySupport => s.serialize_str("mercenary-support"),
            FilterTag::FilterGroup => s.serialize_str("filter-group"),
        }
    }
}

/// `INTERNAL_TRADE_IDS`: filter ids the app handles itself, not trade stat ids.
pub const INTERNAL_TRADE_IDS: [&str; 33] = [
    "item.not_group",
    "item.count_one_group",
    "item.base_percentile",
    "item.memory_strands",
    "item.armour",
    "item.evasion_rating",
    "item.energy_shield",
    "item.ward",
    "item.block",
    "item.total_dps",
    "item.physical_dps",
    "item.elemental_dps",
    "item.crit",
    "item.aps",
    "item.has_empty_modifier",
    "item.mercenary_6link",
    "item.map_item_quantity",
    "item.map_atlas_drops",
    "item.map_item_rarity",
    "item.map_pack_size",
    "item.heist_job_lockpicking",
    "item.heist_job_bruteforce",
    "item.heist_job_perception",
    "item.heist_job_demolition",
    "item.heist_job_counterthaumaturgy",
    "item.heist_job_trapdisarmament",
    "item.heist_job_agility",
    "item.heist_job_deception",
    "item.heist_job_engineering",
    "item.heist_target_priceless",
    "item.heist_wings_revealed",
    "item.heist_wings_total",
    "item.chart_sulphur",
];

/// `ItemHasEmptyModifier`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemHasEmptyModifier {
    Any = 0,
    Prefix = 1,
    Suffix = 2,
}
