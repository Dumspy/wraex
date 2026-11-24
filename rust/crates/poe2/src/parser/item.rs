//! `ref/ee2/renderer/src/parser/ParsedItem.ts`, plus the `ParserState` fields
//! (`name`, `baseType`, `infoVariants`) that the reference keeps on the item and
//! so serialises.

use serde::Serialize;
use wraex_core::Rarity;

use super::advanced_mod_desc::ParsedModifier;
use super::meta::ItemCategory;
use super::modifiers::{ModifierType, StatCalculated};
use crate::data::{AugmentLineData, BaseType};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub enum ItemInfluence {
    Crusader,
    Elder,
    Hunter,
    Redeemer,
    Shaper,
    Warlord,
}

/// `IncursionRoom`: values match the trade site's options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncursionRoom {
    Open = 1,
    Obstructed = 2,
}

/// `EditorItem`: an augment in a socket, as the item editor shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub existing: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub socket_bound: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ref_name: Option<String>,
    pub icon: String,
    pub display_string: String,
    pub stats: Vec<AugmentLineData>,
    pub base_item: BaseType,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AugmentSockets {
    pub empty: f64,
    pub current: f64,
    pub normal: f64,
    /// `null` for an empty socket.
    pub augments: Vec<Option<EditorItem>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GemSockets {
    pub number: f64,
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

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heist {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wings_revealed: Option<f64>,
    /// `Enchants`, `Trinkets`, `Gems` or `Replicas`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Requires {
    pub level: f64,
    pub str: f64,
    pub dex: f64,
    pub int: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Trials {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_of_trials: Option<f64>,
    /// `Victorious`, `Cowardly` or `Deadly`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ultimatum_hint: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
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
    #[serde(rename = "armourRW", skip_serializing_if = "Option::is_none")]
    pub armour_rw: Option<f64>,
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
    #[serde(rename = "weaponFIRE", skip_serializing_if = "Option::is_none")]
    pub weapon_fire: Option<f64>,
    #[serde(rename = "weaponCOLD", skip_serializing_if = "Option::is_none")]
    pub weapon_cold: Option<f64>,
    #[serde(rename = "weaponLIGHTNING", skip_serializing_if = "Option::is_none")]
    pub weapon_lightning: Option<f64>,
    #[serde(rename = "weaponCHAOS", skip_serializing_if = "Option::is_none")]
    pub weapon_chaos: Option<f64>,
    #[serde(rename = "weaponRELOAD", skip_serializing_if = "Option::is_none")]
    pub weapon_reload: Option<f64>,
    #[serde(rename = "weaponSPIRIT", skip_serializing_if = "Option::is_none")]
    pub weapon_spirit: Option<f64>,
    /// `Blighted` or `Blight-ravaged`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_blighted: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_pack_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_item_rarity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_revives: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_drop_chance: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_magic_monsters: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_rare_monsters: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_gold: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_monster_rarity: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_effectiveness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub talisman_tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub augment_sockets: Option<AugmentSockets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_sockets: Option<GemSockets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<StackSize>,
    pub is_unidentified: bool,
    pub is_corrupted: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_unmodifiable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_mirrored: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_sanctified: Option<bool>,
    pub influences: Vec<ItemInfluence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logbook_area_mods: Option<Vec<Vec<ParsedModifier>>>,
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
    pub stats_by_type: Vec<StatCalculated>,
    pub new_mods: Vec<ParsedModifier>,
    pub unknown_modifiers: Vec<UnknownModifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heist: Option<Heist>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<ItemCategory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub requires: Option<Requires>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unidentified_tier: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trials: Option<Trials>,
    /// Set by `findInDatabase`; parsing fails before anything reads it unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub info: Option<BaseType>,
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
    pub fn info(&self) -> &BaseType {
        self.info.as_ref().expect("info is set by findInDatabase")
    }

    /// `item.info.refName === name`
    pub fn ref_name_is(&self, name: &str) -> bool {
        self.info().ref_name.as_deref() == Some(name)
    }

    /// `itemIsModifiable`
    pub fn is_modifiable(&self) -> bool {
        self.info().craftable.is_some()
            && !self.is_corrupted
            && self.is_mirrored != Some(true)
            && self.is_sanctified != Some(true)
    }

    pub(crate) fn empty(name: String, base_type: Option<String>) -> Self {
        Self {
            rarity: None,
            item_level: None,
            armour_ar: None,
            armour_ev: None,
            armour_es: None,
            armour_rw: None,
            armour_block: None,
            base_percentile: None,
            weapon_crit: None,
            weapon_as: None,
            weapon_physical: None,
            weapon_elemental: None,
            weapon_fire: None,
            weapon_cold: None,
            weapon_lightning: None,
            weapon_chaos: None,
            weapon_reload: None,
            weapon_spirit: None,
            map_blighted: None,
            map_tier: None,
            map_pack_size: None,
            map_item_rarity: None,
            map_revives: None,
            map_drop_chance: None,
            map_magic_monsters: None,
            map_rare_monsters: None,
            map_gold: None,
            map_monster_rarity: None,
            map_effectiveness: None,
            gem_level: None,
            area_level: None,
            talisman_tier: None,
            quality: None,
            quality_type: None,
            augment_sockets: None,
            gem_sockets: None,
            stack_size: None,
            is_unidentified: false,
            is_corrupted: false,
            is_unmodifiable: None,
            is_mirrored: None,
            is_sanctified: None,
            influences: Vec::new(),
            logbook_area_mods: None,
            sentinel_charge: None,
            is_synthesised: None,
            is_fractured: None,
            is_veiled: None,
            is_foil: None,
            stats_by_type: Vec::new(),
            new_mods: Vec::new(),
            unknown_modifiers: Vec::new(),
            heist: None,
            note: None,
            category: None,
            requires: None,
            unidentified_tier: None,
            trials: None,
            info: None,
            raw_text: String::new(),
            name,
            base_type,
            info_variants: None,
        }
    }

    /// `createVirtualItem`: an item the parser never saw, standing in for a base type the
    /// item search picked (`WidgetItemSearch.vue`'s `starredItemClick`).
    ///
    /// The reference spreads the props it is given over a fresh `ParsedItem` and hard-codes
    /// `rawText: 'VIRTUAL_ITEM'`, the empty lists and `isUnidentified: false`; `name` and
    /// `baseType` stay `undefined` there, which [`Item::empty`] spells as empty text.
    fn virtual_item(info: BaseType) -> Self {
        let mut item = Self::empty(String::new(), None);
        item.info = Some(info);
        item.raw_text = "VIRTUAL_ITEM".to_owned();
        item
    }

    /// `createVirtualItem({ category: ItemCategory.Gem, info, gemLevel: 1 })`: a skill gem at
    /// level 1.
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

    /// `createVirtualItem({ rarity: ItemRarity.Normal, info })`: a base type, which EE2's
    /// third tab searches.
    pub fn virtual_normal(info: BaseType) -> Self {
        let mut item = Self::virtual_item(info);
        item.rarity = Some(Rarity::Normal);
        item
    }
}
