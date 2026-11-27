//! `ref/apt/renderer/src/parser/ParsedItem.ts`, plus the `ParserState` fields
//! (`name`, `baseType`, `infoVariants`) that the reference keeps on the item and
//! so serialises.

use serde::Serialize;
use wraex_core::Rarity;

use super::advanced_mod_desc::ParsedModifier;
use super::meta::ItemCategory;
use super::modifiers::{ModifierType, StatCalculated};
use super::stat_translations::ParsedStat;
use crate::data::BaseType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ItemInfluence {
    Crusader,
    Elder,
    Hunter,
    Redeemer,
    Shaper,
    Warlord,
}

impl ItemInfluence {
    /// The value, as the trade site and `influence-<value>.png` spell it.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Crusader => "Crusader",
            Self::Elder => "Elder",
            Self::Hunter => "Hunter",
            Self::Redeemer => "Redeemer",
            Self::Shaper => "Shaper",
            Self::Warlord => "Warlord",
        }
    }
}

/// `IncursionRoom`: values match the trade site's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncursionRoom {
    Open = 1,
    Obstructed = 2,
}

impl IncursionRoom {
    /// The room's value, which is also the option's `value` on the trade site.
    pub const fn as_value(self) -> f64 {
        match self {
            Self::Open => 1.0,
            Self::Obstructed => 2.0,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeistBlueprint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wings_revealed: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wings_total: Option<f64>,
    /// `Enchants`, `Trinkets`, `Gems` or `Replicas`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeistContract {
    /// `Lockpicking`, `Brute Force`, ...
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_job: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_level: Option<f64>,
    /// `Priceless`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_value: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sockets {
    /// Only 5 or 6.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub linked: Option<u8>,
    pub white: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StackSize {
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Corrupted {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twice: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnknownModifier {
    pub text: String,
    #[serde(rename = "type")]
    pub kind: ModifierType,
}

/// A parsed clipboard item: `ParsedItem` and the parser's state on it.
///
/// Numbers are doubles, as in the reference, and keep its `NaN`s (written as
/// `null`). Serialise with [`wraex_core::js::json_value`] for the reference's JSON.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<Rarity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_level: Option<f64>,

    #[serde(rename = "armourAR", skip_serializing_if = "Option::is_none")]
    pub armour_ar: Option<f64>,
    #[serde(rename = "armourEV", skip_serializing_if = "Option::is_none")]
    pub armour_ev: Option<f64>,
    #[serde(rename = "armourES", skip_serializing_if = "Option::is_none")]
    pub armour_es: Option<f64>,
    #[serde(rename = "armourWARD", skip_serializing_if = "Option::is_none")]
    pub armour_ward: Option<f64>,
    #[serde(rename = "armourBLOCK", skip_serializing_if = "Option::is_none")]
    pub armour_block: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_percentile: Option<f64>,
    #[serde(rename = "weaponCRIT", skip_serializing_if = "Option::is_none")]
    pub weapon_crit: Option<f64>,
    #[serde(rename = "weaponAS", skip_serializing_if = "Option::is_none")]
    pub weapon_as: Option<f64>,
    #[serde(rename = "weaponPHYSICAL", skip_serializing_if = "Option::is_none")]
    pub weapon_physical: Option<f64>,
    #[serde(rename = "weaponELEMENTAL", skip_serializing_if = "Option::is_none")]
    pub weapon_elemental: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_area: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_item_quantity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_atlas_drops: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_item_rarity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_pack_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_completion_reward: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_maps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_threads: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_currency: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_more_div_cards: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_blueprint: Option<HeistBlueprint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist_contract: Option<HeistContract>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logbook_area_mods: Option<Vec<Vec<ParsedModifier>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart_sulphur: Option<f64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vaal_gem: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub imbued_gem: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mercenary_build: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mercenary_skills: Option<Vec<Vec<ParsedStat>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub talisman_tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_strands: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<Sockets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<StackSize>,
    pub is_unidentified: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_corrupted: Option<Corrupted>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_unmodifiable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_mirrored: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_split: Option<bool>,
    pub influences: Vec<ItemInfluence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentinel_charge: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_synthesised: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fractured: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_veiled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_foil: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_foulborn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_vestigial: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dust_equivalent: Option<f64>,
    pub stats_by_type: Vec<StatCalculated>,
    pub new_mods: Vec<ParsedModifier>,
    pub unknown_modifiers: Vec<UnknownModifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ItemCategory>,
    /// Set by `findInDatabase`; parsing fails before anything reads it unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<BaseType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique_base: Option<BaseType>,
    pub raw_text: String,

    // `ParserState`
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info_variants: Option<Vec<BaseType>>,
}

impl Item {
    /// The item's `info`, which every parser after `findInDatabase` may read.
    #[expect(
        clippy::expect_used,
        reason = "`findInDatabase` sets `info` before any parser reads it (upstream's `item.info!`)"
    )]
    pub const fn info(&self) -> &BaseType {
        self.info.as_ref().expect("info is set by findInDatabase")
    }

    /// `item.info.refName === name`
    pub fn ref_name_is(&self, name: &str) -> bool {
        self.info().ref_name == name
    }

    /// `item.baseType || item.name`: an empty base type counts as missing.
    pub fn base_type_or_name(&self) -> &str {
        self.base_type.as_deref().filter(|b| !b.is_empty()).unwrap_or(&self.name)
    }

    pub(crate) const fn empty(name: String, base_type: Option<String>) -> Self {
        Self {
            rarity: None,
            item_level: None,
            armour_ar: None,
            armour_ev: None,
            armour_es: None,
            armour_ward: None,
            armour_block: None,
            base_percentile: None,
            weapon_crit: None,
            weapon_as: None,
            weapon_physical: None,
            weapon_elemental: None,
            map_area: None,
            area_level: None,
            area_item_quantity: None,
            area_atlas_drops: None,
            area_item_rarity: None,
            area_pack_size: None,
            map_completion_reward: None,
            map_tier: None,
            map_more_maps: None,
            map_more_threads: None,
            map_more_currency: None,
            map_more_div_cards: None,
            heist_blueprint: None,
            heist_contract: None,
            logbook_area_mods: None,
            chart_sulphur: None,
            gem_level: None,
            vaal_gem: None,
            imbued_gem: None,
            mercenary_build: None,
            mercenary_skills: None,
            talisman_tier: None,
            memory_strands: None,
            quality: None,
            sockets: None,
            stack_size: None,
            is_unidentified: false,
            is_corrupted: None,
            is_unmodifiable: None,
            is_mirrored: None,
            is_split: None,
            influences: Vec::new(),
            sentinel_charge: None,
            is_synthesised: None,
            is_fractured: None,
            is_veiled: None,
            is_foil: None,
            is_foulborn: None,
            is_vestigial: None,
            dust_equivalent: None,
            stats_by_type: Vec::new(),
            new_mods: Vec::new(),
            unknown_modifiers: Vec::new(),
            category: None,
            info: None,
            unique_base: None,
            raw_text: String::new(),
            name,
            base_type,
            info_variants: None,
        }
    }

    /// `createVirtualItem`: an item the parser never saw, standing in for a base type the
    /// item search picked (`WidgetItemSearch.vue`'s `selectedItemClick`).
    ///
    /// The reference spreads the props it is given over a fresh `ParsedItem` and hard-codes
    /// `rawText: 'VIRTUAL_ITEM'`, the empty lists and `isUnidentified: false`; `name` and
    /// `baseType` stay `undefined` there, which [`Item::empty`] spells as empty text.
    fn virtual_item(info: BaseType) -> Self {
        let mut item = Self::empty(String::new(), None);
        item.info = Some(info);
        "VIRTUAL_ITEM".clone_into(&mut item.raw_text);
        item
    }

    /// `createVirtualItem({ category: ItemCategory.Gem, info, gemLevel: 1 })`: a skill gem at
    /// level 1, the way the heist gem finder prices one.
    pub fn virtual_gem(info: BaseType) -> Self {
        let mut item = Self::virtual_item(info);
        item.category = Some(ItemCategory::Gem);
        item.gem_level = Some(1.0);
        item
    }

    /// `createVirtualItem({ rarity: ItemRarity.Unique, info })`: a unique, by name.
    pub fn virtual_unique(info: BaseType) -> Self {
        let mut item = Self::virtual_item(info);
        item.rarity = Some(Rarity::Unique);
        item
    }
}
