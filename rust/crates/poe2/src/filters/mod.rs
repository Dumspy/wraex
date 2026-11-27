//! `ref/ee2/renderer/src/web/price-check/filters`: the default filters a price
//! check starts with ([`create_presets`]).
//!
//! Where the reference throws (`undefined.x`, `filter.roll!.value`) or asserts a
//! missing field with `!`, the functions here return a [`FilterError`].
//! `fill-augments.ts` (and `calc-base.ts` behind it) is not ported: only the
//! item editor and the settings page call it.

pub mod common;
pub mod create_item_filters;
pub mod create_presets;
pub mod create_stat_filters;
pub mod create_unique_filters;
pub mod pseudo;

use indexmap::IndexMap;
use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

pub use create_presets::{CreatePresetsOptions, Presets, create_presets};

use crate::data::BaseType;
use crate::parser::item::ItemInfluence;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::{ModifierType, StatSource};

/// Something the reference throws on, or a field it asserts that the item lacks.
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
    pub stats: Vec<StatFilter>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
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
    pub category: Option<ItemCategory>,
    /// Only on `searchRelaxed`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemFilters {
    pub search_exact: SearchFilter,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_relaxed: Option<SearchFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discriminator: Option<Discriminator>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<ValueFilter<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_number: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked_sockets: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub white_sockets: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub augment_sockets: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_editor_selection: Option<ItemEditorSelection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrupted: Option<CorruptedFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fractured: Option<ValueFilter<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mirrored: Option<Disabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sanctified: Option<Disabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foil: Option<Disabled>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub influences: Option<Vec<InfluenceFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_blighted: Option<ValueFilter<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unidentified: Option<UnidentifiedFilter<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unidentified_tier: Option<UnidentifiedFilter<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veiled: Option<VeiledFilter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awarded_ascendancy_points: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultimatum_hint: Option<UnidentifiedFilter<&'static str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_wings_revealed: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_charge: Option<FilterNumeric>,
    pub trade: TradeFilters,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temp_augment_storage: Option<Vec<StatFilter>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<RequiresFilter>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Discriminator {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trade: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemEditorSelection {
    pub disabled: bool,
    pub editing: bool,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CorruptedFilter {
    pub value: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exact: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValueFilter<T> {
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Disabled {
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InfluenceFilter {
    pub value: ItemInfluence,
    pub disabled: bool,
}

/// `{ value, disabled }` (`unidentified`, `unidentifiedTier`, `ultimatumHint`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnidentifiedFilter<T> {
    pub value: T,
    pub disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VeiledFilter {
    pub stat_refs: Vec<String>,
    pub veiled_count: f64,
    pub disabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct RequiresFilter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub str: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dex: Option<FilterNumeric>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub int: Option<FilterNumeric>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeFilters {
    pub offline: bool,
    pub online_in_league: bool,
    /// `any`, `online`, `securable`, `available` or `onlineleague`.
    pub listing_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub listed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    pub league: String,
    /// `app` or `api`.
    pub collapse_listings: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FilterNumeric {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    pub disabled: bool,
}

impl FilterNumeric {
    pub const fn new(value: f64, disabled: bool) -> Self {
        Self { value, max: None, disabled }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct MinMax {
    pub min: f64,
    pub max: f64,
}

/// `StatFilterRoll`
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

/// `{ value }`; the value is `undefined` (written as `{}`) when a trade-option
/// stat's source has no option.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FilterOption {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
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
    /// `undefined` entries (no such oil) are written as `null`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oils: Option<Vec<Option<&'static str>>>,
    pub sources: Vec<StatSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<FilterRoll>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<FilterOption>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<&'static str>,
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_info: Option<AdditionalInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub editor_added: Option<BaseType>,
}

impl StatFilter {
    /// A filter with no sources, roll or option.
    pub fn plain(trade_id: &str, text: &str, tag: FilterTag, disabled: bool) -> Self {
        Self {
            trade_id: Some(vec![trade_id.to_owned()]),
            stat_ref: text.to_owned(),
            text: text.to_owned(),
            tag,
            oils: None,
            sources: Vec::new(),
            roll: None,
            option: None,
            hidden: None,
            disabled,
            additional_info: None,
            editor_added: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub elemental_info: Option<IndexMap<&'static str, FilterRoll>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub empty_modifier_info: Option<EmptyModifierCounts>,
}

/// `Record<ItemHasEmptyModifier, number>`, keyed `"0"`, `"1"`, `"2"`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EmptyModifierCounts(pub [f64; 3]);

impl Serialize for EmptyModifierCounts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(3))?;
        for (i, n) in self.0.iter().enumerate() {
            map.serialize_entry(&i.to_string(), n)?;
        }
        map.end()
    }
}

/// `FilterTag`. Stat filters take their modifier type as the tag (`type as
/// unknown as FilterTag`), which covers `rune`, `desecrated`, `skill`, ...
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterTag {
    Type(ModifierType),
    Corrupted,
    Synthesised,
    Eldritch,
    Variant,
    Property,
    Delve,
    Unveiled,
    Incursion,
    Mutated,
}

impl FilterTag {
    pub const PSEUDO: Self = Self::Type(ModifierType::Pseudo);
    pub const EXPLICIT: Self = Self::Type(ModifierType::Explicit);
    pub const IMPLICIT: Self = Self::Type(ModifierType::Implicit);
    pub const ENCHANT: Self = Self::Type(ModifierType::Enchant);
    pub const FRACTURED: Self = Self::Type(ModifierType::Fractured);
    pub const DESECRATED: Self = Self::Type(ModifierType::Desecrated);
    pub const CRAFTED: Self = Self::Type(ModifierType::Crafted);
    pub const SKILL: Self = Self::Type(ModifierType::Skill);
    pub const AUGMENT: Self = Self::Type(ModifierType::Augment);
    pub const ADDED_AUGMENT: Self = Self::Type(ModifierType::AddedAugment);
}

impl Serialize for FilterTag {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(match self {
            Self::Type(t) => t.as_str(),
            Self::Corrupted => "corrupted",
            Self::Synthesised => "synthesised",
            Self::Eldritch => "eldritch",
            Self::Variant => "variant",
            Self::Property => "property",
            Self::Delve => "explicit-delve",
            Self::Unveiled => "explicit-veiled",
            Self::Incursion => "explicit-incursion",
            Self::Mutated => "mutated",
        })
    }
}

/// `INTERNAL_TRADE_IDS`: filter ids the app handles itself. The order matters:
/// the elemental affix ids are read by index (12 to 15).
pub const INTERNAL_TRADE_IDS: [&str; 27] = [
    "item.base_percentile",
    "item.armour",
    "item.evasion_rating",
    "item.energy_shield",
    "item.block",
    "item.total_dps",
    "item.physical_dps",
    "item.elemental_dps",
    "item.crit",
    "item.aps",
    "item.has_empty_modifier",
    "item.spirit",
    "item.has_elemental_affix",
    "item.has_elemental_fire_affix",
    "item.has_elemental_cold_affix",
    "item.has_elemental_lightning_affix",
    "item.reload_time",
    "item.rarity_magic",
    "item.map_revives",
    "item.map_pack_size",
    "item.map_drop_chance",
    "item.map_item_rarity",
    "item.map_magic_monsters",
    "item.map_rare_monsters",
    "item.map_gold",
    "item.runic_ward",
    "item.duplicates",
];

/// `ItemHasEmptyModifier`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemHasEmptyModifier {
    Any = 0,
    Prefix = 1,
    Suffix = 2,
}

/// `ItemIsElementalModifier`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemIsElementalModifier {
    Any = 0,
    Fire = 1,
    Cold = 2,
    Lightning = 3,
}

impl ItemIsElementalModifier {
    /// The enum's number, which `item.elemental_dps`'s option carries.
    pub const fn value(self) -> u8 {
        match self {
            Self::Any => 0,
            Self::Fire => 1,
            Self::Cold => 2,
            Self::Lightning => 3,
        }
    }
}

impl ItemHasEmptyModifier {
    /// The enum's number, which indexes [`EmptyModifierCounts`].
    pub const fn value(self) -> usize {
        match self {
            Self::Any => 0,
            Self::Prefix => 1,
            Self::Suffix => 2,
        }
    }
}
