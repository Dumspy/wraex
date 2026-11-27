//! What a PoE2 price check shows from the trade site's answers.
//!
//! `parseFetchResult` (`pathofexile-trade.ts`) builds the tooltip-like display
//! item of a fetched listing. Listings are read as JSON, as leniently as the
//! reference reads them; where it would throw (`values[0][1]` of an empty
//! list), the display item is an error, which upstream logs and leaves out.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::data::Data;
use crate::parser::parse_affix_strings;

/// `TradeNumberColors`
pub mod colors {
    pub const WHITE: f64 = 0.0;
    pub const AUGMENTED: f64 = 1.0;
    pub const UNMET: f64 = 2.0;
    pub const ENCHANT: f64 = 8729.0;
    pub const FRACTURED: f64 = 8730.0;
    pub const DESECRATED: f64 = 8731.0;
    pub const SANCTIFIED: f64 = 8732.0;
    pub const MUTATED: f64 = 8733.0;
}

/// `TradePropType` values the name block reads.
mod prop {
    pub const MAP_TIER: i64 = 1;
    pub const MAP_QUANTITY_BONUS: i64 = 2;
    pub const MAP_RARITY_BONUS: i64 = 3;
    pub const MAP_PACK_SIZE_BONUS: i64 = 4;
    pub const QUALITY: i64 = 6;
    pub const WEAPON_PHYSICAL_DAMAGE: i64 = 9;
    pub const WEAPON_ELEMENTAL_DAMAGE: i64 = 10;
    pub const WEAPON_CRITICAL_CHANCE: i64 = 12;
    pub const WEAPON_SPEED: i64 = 13;
    pub const SHIELD_BLOCK: i64 = 15;
    pub const ARMOUR_REDUCTION: i64 = 16;
    pub const ARMOUR_EVASION: i64 = 17;
    pub const ARMOUR_ENERGY_SHIELD: i64 = 18;
    pub const SPIRIT: i64 = 25;
    pub const ARMOUR_WARD: i64 = 54;
    pub const MAP_ITEM_DROP_CHANCE_BONUS: i64 = 66;
    pub const WEAPON_RELOAD_TIME: i64 = 97;
    pub const MAP_PORTALS: i64 = 98;
    pub const MAP_GOLD_QUANTITY_BONUS: i64 = 100;
    pub const MAP_MAGIC_MONSTERS_QUANTITY_BONUS: i64 = 102;
    pub const MAP_RARE_MONSTERS_QUANTITY_BONUS: i64 = 103;
}

/// `DisplayItemLine`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisplayItemLine {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tier: Option<String>,
    /// A string or a number; `undefined` when the listing lacks it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// A `TradeNumberColors` value, or whatever the listing says.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<Value>,
}

impl DisplayItemLine {
    fn new(text: impl Into<String>, value: Option<Value>, color: Option<Value>) -> Self {
        Self { text: text.into(), tier: None, value, color }
    }
}

fn color(c: f64) -> Value {
    Value::from(c)
}

/// `DisplayItem`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayItem {
    pub title: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rarity: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_type: Option<Value>,
    pub name_block: Vec<DisplayItemLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_props: Option<Vec<DisplayItemLine>>,
    pub grant_skill: Vec<DisplayItemLine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enchant_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rune_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub implicit_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fractured_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explicit_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crafted_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desecrated_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mutated_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pseudo_mods: Option<Vec<DisplayItemLine>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub veiled_mods: Option<Vec<DisplayItemLine>>,
    /// Passed through as the site sends them.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sockets: Option<Value>,
    pub item_tags: Vec<DisplayItemLine>,
    pub icon: Icon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Icon {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<Value>,
}

/// A fetched listing (`FetchResult`).
///
/// Fields the reference treats as plain JavaScript values (truthiness,
/// `String(x)`, pass-through) stay [`Value`]; the ones it reads with a known
/// type are typed. Everything the site may omit is optional.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FetchResult {
    pub id: String,
    pub item: FetchItem,
    pub listing: FetchListing,
    #[serde(default)]
    pub gone: Option<Value>,
}

/// `FetchResult['item']`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchItem {
    #[serde(default)]
    pub w: Option<Value>,
    #[serde(default)]
    pub h: Option<Value>,
    #[serde(default)]
    pub icon: Option<Value>,
    #[serde(default)]
    pub sockets: Option<Value>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub type_line: Option<Value>,
    #[serde(default)]
    pub rarity: Option<Value>,
    #[serde(default)]
    pub frame_type: Option<Value>,
    /// The site's item level, shown as sent when the item has no `78` property.
    #[serde(default)]
    pub ilvl: Option<Value>,
    /// Tested strictly (`=== false`), so a non-boolean stays as sent.
    #[serde(default)]
    pub identified: Option<Value>,
    #[serde(default)]
    pub unidentified_tier: Option<Value>,
    #[serde(default)]
    pub note: Option<Value>,
    #[serde(default)]
    pub sanctified: Option<Value>,
    #[serde(default)]
    pub duplicated: Option<Value>,
    #[serde(default)]
    pub stack_size: Option<Value>,
    #[serde(default)]
    pub corrupted: Option<Value>,
    #[serde(default)]
    pub double_corrupted: Option<Value>,
    /// Only its length is read.
    #[serde(default)]
    pub gem_sockets: Option<Vec<Value>>,
    #[serde(default)]
    pub properties: Option<Vec<TradeDataRichLine>>,
    #[serde(default)]
    pub requirements: Option<Vec<TradeDataRichLine>>,
    /// The site sends `[]` for an item without extended data; upstream reads it
    /// as an empty object.
    #[serde(default, deserialize_with = "lenient_extended")]
    pub extended: Option<FetchResultExtended>,
    #[serde(default)]
    pub granted_skills: Option<Vec<TradeDataRichLine>>,
    #[serde(default)]
    pub implicit_mods: Option<Mods>,
    #[serde(default)]
    pub explicit_mods: Option<Mods>,
    #[serde(default)]
    pub crafted_mods: Option<Mods>,
    #[serde(default)]
    pub mutated_mods: Option<Mods>,
    #[serde(default)]
    pub enchant_mods: Option<Mods>,
    #[serde(default)]
    pub rune_mods: Option<Mods>,
    #[serde(default)]
    pub veiled_mods: Option<Vec<VeiledMod>>,
    #[serde(default)]
    pub pseudo_mods: Option<Mods>,
    #[serde(default)]
    pub desecrated_mods: Option<Mods>,
    #[serde(default)]
    pub fractured_mods: Option<Mods>,
    #[serde(default)]
    pub socketed_items: Option<Vec<Self>>,
}

/// `FetchResult['item']['extended']`: the item's totals and mod tiers.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct FetchResultExtended {
    #[serde(default)]
    pub dps: Option<f64>,
    #[serde(default)]
    pub pdps: Option<f64>,
    #[serde(default)]
    pub edps: Option<f64>,
    #[serde(default)]
    pub ar: Option<f64>,
    #[serde(default)]
    pub ev: Option<f64>,
    #[serde(default)]
    pub es: Option<f64>,
    #[serde(default)]
    pub ward: Option<f64>,
    #[serde(default)]
    pub dps_aug: Option<bool>,
    #[serde(default)]
    pub pdps_aug: Option<bool>,
    #[serde(default)]
    pub edps_aug: Option<bool>,
    #[serde(default)]
    pub ar_aug: Option<bool>,
    #[serde(default)]
    pub ev_aug: Option<bool>,
    #[serde(default)]
    pub es_aug: Option<bool>,
    #[serde(default)]
    pub ward_aug: Option<bool>,
    /// `Record<string, TradeModMetadata[]>`, keyed by mod block.
    #[serde(default)]
    pub mods: Option<BTreeMap<String, Vec<TradeModMetadata>>>,
    /// `Record<string, TradeModHashes[]>`, keyed by mod block.
    #[serde(default)]
    pub hashes: Option<BTreeMap<String, Vec<TradeModHashes>>>,
}

/// `[displayHash, modIndexes]`: the `mods` a line's hash points to.
pub type TradeModHashes = (String, Option<Vec<u64>>);

/// `TradeModMetadata`; only the tier is displayed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TradeModMetadata {
    #[serde(default)]
    pub tier: Option<Value>,
}

/// `FetchResultMod`: a mod line with its tier-bearing mods.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FetchResultMod {
    #[serde(default)]
    pub description: Option<Value>,
    #[serde(default)]
    pub mods: Option<Vec<FetchModInfo>>,
}

/// `FetchModInfo`; only the tier is displayed.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FetchModInfo {
    #[serde(default)]
    pub tier: Option<Value>,
}

/// `FetchResult['item']['*Mods']`: `FetchResultMod[] | string[]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Mods {
    /// String lines; their tiers come from `extended.hashes`.
    Text(Vec<String>),
    /// Object lines with their mod tiers inline.
    Rich(Vec<FetchResultMod>),
}

/// `FetchResult['item']['veiledMods'][]`: a string or `{ description }`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum VeiledMod {
    Text(String),
    Rich(FetchResultMod),
}

/// `extended` is an object when the item has one; the site sends `[]` for an
/// item without extended data (the Djinn Barya in the recorded answer), and
/// upstream reads that as an object with no keys.
fn lenient_extended<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<FetchResultExtended>, D::Error> {
    let value = Value::deserialize(d)?;
    if value.is_null() {
        return Ok(None);
    }
    Ok(Some(serde_json::from_value(value).unwrap_or_default()))
}

/// A `TradeDataRichLine`: `values` is `[[text, displayMode], ...]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct TradeDataRichLine {
    #[serde(default)]
    pub name: Option<Value>,
    #[serde(default)]
    pub values: Vec<Vec<Value>>,
    #[serde(rename = "type", default)]
    pub kind: Option<i64>,
}

/// `FetchResult['listing']`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FetchListing {
    #[serde(default)]
    pub indexed: Option<String>,
    #[serde(default)]
    pub price: Option<Price>,
    /// An instant buyout lists a `fee`; upstream tests it against `null`.
    #[serde(default)]
    pub fee: Option<Value>,
    #[serde(default)]
    pub account: Account,
    #[serde(default)]
    pub in_demand: Option<Value>,
}

/// `FetchResult['listing']['price']`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Price {
    #[serde(default)]
    pub amount: Option<f64>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
}

/// `Account`
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub last_character_name: Option<String>,
    /// `{ status }`, or `null` for an offline account. The reference tests it
    /// for truthiness, so it stays a [`Value`].
    #[serde(default)]
    pub online: Option<Value>,
}

/// What `parseFetchResult` throws on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayError(pub String);

type R<T> = Result<T, DisplayError>;

fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `${x}` of a JSON value.
fn text(v: Option<&Value>) -> String {
    match v {
        None => "undefined".to_owned(),
        Some(Value::Null) => "null".to_owned(),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.as_f64().map_or_else(|| n.to_string(), wraex_core::js::number_to_string),
        Some(other) => other.to_string(),
    }
}

/// `values[0][i]`, which throws when `values` is empty.
fn first_value(values: &[Vec<Value>], i: usize) -> R<Option<&Value>> {
    let first =
        values.first().ok_or_else(|| DisplayError("Cannot read properties of undefined (reading '1')".into()))?;
    Ok(first.get(i))
}

/// `parseFetchResult`: a listing as the trade site sent it.
pub fn parse_fetch_result(d: &Data, result: &Value) -> R<DisplayItem> {
    let fetched: FetchResult =
        serde_json::from_value(result.clone()).map_err(|error| DisplayError(error.to_string()))?;
    display_item(d, &fetched)
}

/// `parseFetchResult` on an already parsed listing.
fn display_item(d: &Data, result: &FetchResult) -> R<DisplayItem> {
    let item = &result.item;

    let mut title = Vec::new();
    if let Some(name) = item.name.as_ref().filter(|n| !n.is_empty()) {
        title.push(name.clone());
    }
    if truthy(item.type_line.as_ref()) {
        title.push(text(item.type_line.as_ref()));
    }

    let mut item_tags = Vec::new();
    // Exclusive to every other tag but corrupted.
    if item.identified.as_ref() == Some(&Value::Bool(false)) {
        let tag = if truthy(item.unidentified_tier.as_ref()) {
            format!("Unidentified (Tier {})", text(item.unidentified_tier.as_ref()))
        } else {
            "item.unidentified".to_owned()
        };
        item_tags.push(DisplayItemLine::new(tag, None, Some(color(colors::UNMET))));
    }
    // The rest should be exclusive.
    if truthy(item.double_corrupted.as_ref()) {
        item_tags.push(DisplayItemLine::new(d.s("DOUBLE_CORRUPTED"), None, Some(color(colors::UNMET))));
    } else if truthy(item.corrupted.as_ref()) {
        item_tags.push(DisplayItemLine::new(d.s("CORRUPTED"), None, Some(color(colors::UNMET))));
    }
    if truthy(item.duplicated.as_ref()) {
        item_tags.push(DisplayItemLine::new(d.s("MIRRORED"), None, Some(color(colors::AUGMENTED))));
    }
    if truthy(item.sanctified.as_ref()) {
        item_tags.push(DisplayItemLine::new(d.s("SANCTIFIED"), None, Some(color(colors::SANCTIFIED))));
    }

    let extended = item.extended.as_ref();
    let mod_block = |translated: Option<&Mods>, block_color: Option<f64>, kind: &str| {
        parse_mod_block(
            translated,
            block_color.unwrap_or(colors::AUGMENTED),
            extended.and_then(|e| e.mods.as_ref()).and_then(|m| m.get(kind)).map(Vec::as_slice),
            extended.and_then(|e| e.hashes.as_ref()).and_then(|h| h.get(kind)).map(Vec::as_slice),
        )
    };

    let veiled_mods = item.veiled_mods.as_ref().map(|mods| {
        mods.iter()
            .map(|vm| {
                let t = match vm {
                    VeiledMod::Text(s) => s.clone(),
                    VeiledMod::Rich(m) => text(m.description.as_ref()),
                };
                let label = if t.starts_with("Prefix") { "Unrevealed Prefix" } else { "Unrevealed Suffix" };
                DisplayItemLine::new(label, None, Some(color(colors::DESECRATED)))
            })
            .collect()
    });

    Ok(DisplayItem {
        title,
        rarity: item.rarity.clone(),
        frame_type: item.frame_type.clone(),
        name_block: build_name_block(extended, item.properties.as_deref())?,
        item_props: build_item_props(item.ilvl.as_ref(), item.requirements.as_deref())?,
        grant_skill: build_grant_skill_block(item.granted_skills.as_deref())?,
        enchant_mods: mod_block(item.enchant_mods.as_ref(), Some(colors::ENCHANT), "enchant"),
        rune_mods: mod_block(item.rune_mods.as_ref(), Some(colors::ENCHANT), "rune"),
        implicit_mods: mod_block(item.implicit_mods.as_ref(), None, "implicit"),
        fractured_mods: mod_block(item.fractured_mods.as_ref(), Some(colors::FRACTURED), "fractured"),
        explicit_mods: mod_block(item.explicit_mods.as_ref(), None, "explicit"),
        crafted_mods: mod_block(item.crafted_mods.as_ref(), None, "crafted"),
        desecrated_mods: mod_block(item.desecrated_mods.as_ref(), Some(colors::DESECRATED), "desecrated"),
        mutated_mods: mod_block(item.mutated_mods.as_ref(), Some(colors::MUTATED), "mutated"),
        pseudo_mods: mod_block(item.pseudo_mods.as_ref(), None, "pseudo"),
        veiled_mods,
        sockets: item.sockets.clone(),
        item_tags,
        icon: Icon { url: item.icon.clone(), w: item.w.clone(), h: item.h.clone() },
    })
}

/// `parseModBlock`: strings (tiers from `extended`) or objects (tiers inline).
/// A missing color is `TradeNumberColors.Augmented`, the parameter's default.
fn parse_mod_block(
    translated: Option<&Mods>,
    block_color: f64,
    mods: Option<&[TradeModMetadata]>,
    hashes: Option<&[TradeModHashes]>,
) -> Option<Vec<DisplayItemLine>> {
    let lines = translated?;
    match lines {
        Mods::Text(lines) => {
            if lines.is_empty() {
                return Some(Vec::new());
            }
            Some(
                lines
                    .iter()
                    .enumerate()
                    .map(|(index, s)| DisplayItemLine {
                        text: parse_affix_strings(s),
                        tier: get_tier(index, mods, hashes),
                        value: None,
                        color: Some(color(block_color)),
                    })
                    .collect(),
            )
        }
        Mods::Rich(lines) => {
            if lines.is_empty() {
                return Some(Vec::new());
            }
            Some(
                lines
                    .iter()
                    .map(|s| DisplayItemLine {
                        text: parse_affix_strings(&text(s.description.as_ref())),
                        tier: get_tier_v2(s.mods.as_deref()),
                        value: None,
                        color: Some(color(block_color)),
                    })
                    .collect(),
            )
        }
    }
}

/// `getTier`: the tiers of the mods a line's hash points to.
fn get_tier(
    display_index: usize,
    mods: Option<&[TradeModMetadata]>,
    hashes: Option<&[TradeModHashes]>,
) -> Option<String> {
    let mods = mods.filter(|m| !m.is_empty())?;
    let entry = hashes?.get(display_index)?;
    let indexes = entry.1.as_ref()?;
    let tiers: Vec<String> = indexes
        .iter()
        .filter_map(|i| usize::try_from(*i).ok().and_then(|i| mods.get(i)))
        .filter_map(|m| m.tier.as_ref())
        .map(|t| text(Some(t)))
        .collect();
    Some(tiers.join(" + "))
}

/// `getTierV2`: the tiers of a line's mods (`undefined` joins as empty).
fn get_tier_v2(mods: Option<&[FetchModInfo]>) -> Option<String> {
    let mods = mods.filter(|m| !m.is_empty())?;
    Some(
        mods.iter()
            .map(|m| m.tier.as_ref().map_or_else(String::new, |t| text(Some(t))))
            .collect::<Vec<_>>()
            .join(" + "),
    )
}

/// `buildNameBlock`: the item's properties, defences and DPS from `extended`.
fn build_name_block(
    extended: Option<&FetchResultExtended>,
    props: Option<&[TradeDataRichLine]>,
) -> R<Vec<DisplayItemLine>> {
    let (Some(extended), Some(props)) = (extended, props) else { return Ok(Vec::new()) };
    let ext = |value: Option<f64>| value.map(Value::from);
    // `useColor(aug)`
    let use_color = |aug: Option<bool>| if aug == Some(true) { colors::AUGMENTED } else { colors::WHITE };

    let mut block: Vec<DisplayItemLine> = Vec::new();
    let mut has_damage = false;
    for p in props {
        let values = p.values.as_slice();
        let name = text(p.name.as_ref());
        // `useColor(aug) || values[0][1]`
        let or_value_color = |aug: Option<bool>| -> R<Option<Value>> {
            let c = use_color(aug);
            if c == 0.0 { Ok(first_value(values, 1)?.cloned()) } else { Ok(Some(color(c))) }
        };
        let label: Option<String> = match p.kind {
            Some(prop::WEAPON_PHYSICAL_DAMAGE) => {
                block.push(DisplayItemLine::new(
                    "item.physical_dps",
                    ext(extended.pdps),
                    Some(color(use_color(extended.pdps_aug))),
                ));
                has_damage = true;
                continue;
            }
            Some(prop::WEAPON_ELEMENTAL_DAMAGE) => {
                block.push(DisplayItemLine::new(
                    "item.elemental_dps",
                    ext(extended.edps),
                    Some(color(use_color(extended.edps_aug))),
                ));
                has_damage = true;
                continue;
            }
            Some(prop::ARMOUR_REDUCTION) => {
                block.push(DisplayItemLine::new("item.armour", ext(extended.ar), or_value_color(extended.ar_aug)?));
                continue;
            }
            Some(prop::ARMOUR_EVASION) => {
                block.push(DisplayItemLine::new(
                    "item.evasion_rating",
                    ext(extended.ev),
                    or_value_color(extended.ev_aug)?,
                ));
                continue;
            }
            Some(prop::ARMOUR_ENERGY_SHIELD) => {
                block.push(DisplayItemLine::new(
                    "item.energy_shield",
                    ext(extended.es),
                    or_value_color(extended.es_aug)?,
                ));
                continue;
            }
            Some(prop::ARMOUR_WARD) => {
                block.push(DisplayItemLine::new(
                    "item.runic_ward",
                    ext(extended.ward),
                    or_value_color(extended.ward_aug)?,
                ));
                continue;
            }
            Some(prop::QUALITY) => Some(format!("{}: {{0}}", parse_affix_strings(&name))),
            Some(prop::WEAPON_SPEED) => Some("item.aps".into()),
            Some(prop::WEAPON_CRITICAL_CHANCE) => Some("item.crit".into()),
            Some(prop::WEAPON_RELOAD_TIME) => Some("item.reload_time".into()),
            Some(prop::SHIELD_BLOCK) => Some("item.block".into()),
            Some(prop::SPIRIT) => Some("item.spirit".into()),
            // Map properties
            Some(prop::MAP_TIER) => Some("item.map_tier".into()),
            Some(prop::MAP_PORTALS) => Some("item.map_revives".into()),
            Some(prop::MAP_PACK_SIZE_BONUS) => Some("item.map_pack_size".into()),
            Some(prop::MAP_MAGIC_MONSTERS_QUANTITY_BONUS) => Some("item.map_magic_monsters".into()),
            Some(prop::MAP_RARE_MONSTERS_QUANTITY_BONUS) => Some("item.map_rare_monsters".into()),
            Some(prop::MAP_ITEM_DROP_CHANCE_BONUS) => Some("item.map_drop_chance".into()),
            Some(prop::MAP_RARITY_BONUS) => Some("item.map_item_rarity".into()),
            Some(prop::MAP_GOLD_QUANTITY_BONUS) => Some("item.map_gold".into()),
            Some(prop::MAP_QUANTITY_BONUS | _) | None => None,
        };

        let has_values = !values.is_empty();
        // If there is a value, there should be just one.
        block.push(DisplayItemLine::new(
            label.unwrap_or_else(|| format!("{} ", parse_affix_strings(&name))),
            Some(if has_values { first_value(values, 0)?.cloned().unwrap_or(Value::Null) } else { Value::from("") }),
            if has_values { first_value(values, 1)?.cloned() } else { Some(color(colors::WHITE)) },
        ));
    }
    if has_damage && let Some(index) = block.iter().position(|l| l.text.contains("dps")) {
        block.insert(
            index,
            DisplayItemLine::new("item.total_dps", ext(extended.dps), Some(color(use_color(extended.dps_aug)))),
        );
    }
    Ok(block)
}

/// `buildItemProps`: item level and requirements.
fn build_item_props(
    ilvl: Option<&Value>,
    requirements: Option<&[TradeDataRichLine]>,
) -> R<Option<Vec<DisplayItemLine>>> {
    if !truthy(ilvl) && requirements.is_none() {
        return Ok(None);
    }
    let mut block = Vec::new();
    if truthy(ilvl) {
        block.push(DisplayItemLine::new("item.item_level", ilvl.cloned(), Some(color(colors::WHITE))));
    }
    if let Some((first, rest)) = requirements.and_then(<[TradeDataRichLine]>::split_first) {
        let mut value = first_value(&first.values, 0)?.cloned().unwrap_or(Value::Null);
        if !rest.is_empty() {
            let parts = rest
                .iter()
                .map(|r| -> R<String> { Ok(format!("{} {}", text(first_value(&r.values, 0)?), text(r.name.as_ref()))) })
                .collect::<R<Vec<_>>>()?;
            value = Value::from(parse_affix_strings(&format!("{}, {}", text(Some(&value)), parts.join(", "))));
        }
        // No colon for requirements.
        block.push(DisplayItemLine::new(
            format!("{} ", parse_affix_strings(&text(first.name.as_ref()))),
            Some(value),
            Some(color(colors::WHITE)),
        ));
    }
    Ok(Some(block))
}

/// `buildGrantSkillBlock`
fn build_grant_skill_block(skills: Option<&[TradeDataRichLine]>) -> R<Vec<DisplayItemLine>> {
    let mut block = Vec::new();
    for skill in skills.unwrap_or_default() {
        let values = skill.values.as_slice();
        block.push(DisplayItemLine::new(
            format!("{}: ", parse_affix_strings(&text(skill.name.as_ref()))),
            Some(Value::from(parse_affix_strings(&text(first_value(values, 0)?)))),
            first_value(values, 1)?.cloned(),
        ));
    }
    Ok(block)
}

#[cfg(test)]
mod row_tests {
    use super::*;
    use crate::data::tests::en;

    fn recorded_answer() -> Value {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/ee2/renderer/specs/docs/fetchResponses.json");
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn rows_from_ee2s_recorded_answer() {
        let answer = recorded_answer();
        let listings: Vec<FetchResult> = serde_json::from_value(answer["result"].clone()).unwrap();
        let mut prices = crate::prices::Prices::default();
        let now = wraex_core::relative_time::parse_iso("2026-01-07T18:47:51Z").unwrap();
        let rows = pricing_results(en(), &mut prices, &listings, "joukerone#4206", now);
        assert_eq!(rows.len(), 21);
        let first = &rows[0];
        assert_eq!((first.price_amount, first.price_currency.as_str()), (2.0, "regal"));
        assert_eq!((first.is_mine, first.is_instant_buyout, first.relative_date.as_str()), (true, true, "1 day ago"));
        // Fee or not, an `online: null` account is offline here (unlike PoE1).
        assert_eq!(first.account_status, AccountStatus::Offline);
        assert!(first.display_item.is_some());
        // No prices loaded: nothing to normalise to.
        assert_eq!((first.normalized_price.as_deref(), first.normalized_price_currency.as_ref()), (None, None));
    }

    /// `specs/web/price-check/trade/pathofexile-trade.test.ts`: the affix tiers
    /// of each explicit line of a listing's tooltip.
    #[test]
    fn listing_tooltips_keep_their_affix_tiers() {
        let answer = recorded_answer();
        let listings: Vec<FetchResult> = serde_json::from_value(answer["result"].clone()).unwrap();
        let tiers = |prefix: &str| {
            let result = listings
                .iter()
                .find(|result| result.id.starts_with(prefix))
                .unwrap_or_else(|| panic!("no result {prefix}"));
            let display = display_item(en(), result).unwrap();
            display
                .explicit_mods
                .unwrap()
                .into_iter()
                .map(|line| line.tier.unwrap_or_default())
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(tiers("65c7c4e8"), "P2 S3");
        // A hybrid modifier's lines share one tier.
        assert_eq!(tiers("ece06af2"), "P7 P7 P5 S5 S6");
        assert_eq!(tiers("2fa2f1ee"), "P2 + P5 P6 P5 S4 S4 S4");
        assert_eq!(tiers("91c9f13"), "P19 S1 S1 S1");
    }
}

/// `CONVERT_CURRENCY`: the trade site's ids of greater and perfect orbs, as
/// the rows show them.
const CONVERT_CURRENCY: [(&str, &str); 10] = [
    ("greater-orb-of-transmutation", "G. transmute"),
    ("perfect-orb-of-transmutation", "P. transmute"),
    ("greater-orb-of-augmentation", "G. aug"),
    ("perfect-orb-of-augmentation", "P. aug"),
    ("greater-chaos-orb", "G. chaos"),
    ("perfect-chaos-orb", "P. chaos"),
    ("greater-regal-orb", "G. regal"),
    ("perfect-regal-orb", "P. regal"),
    ("greater-exalted-orb", "G. exalted"),
    ("perfect-exalted-orb", "P. exalted"),
];

/// `'online' | 'afk' | 'offline'`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountStatus {
    Offline,
    Online,
    Afk,
}

/// A price-check row (`PricingResult`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingResult {
    pub id: String,
    pub item_level: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_size: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub corrupted: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gem_sockets: Option<usize>,
    pub relative_date: String,
    pub price_amount: f64,
    pub price_currency: String,
    /// 2 for greater, 3 for perfect orbs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_currency_rank: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_price: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub normalized_price_currency: Option<crate::prices::CoreCurrency>,
    pub is_mine: bool,
    pub has_note: bool,
    pub is_instant_buyout: bool,
    pub account_name: String,
    pub account_status: AccountStatus,
    pub ign: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_item: Option<DisplayItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_demand: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gone: Option<Value>,
}

/// `prop.values[0][0]` of the first property of `kind`.
fn property(item: &FetchItem, kind: i64) -> Option<Value> {
    let prop = item.properties.as_ref()?.iter().find(|p| p.kind == Some(kind))?;
    prop.values.first()?.first().cloned()
}

/// `getCurrencyDetailsId`: the poe.ninja query for a trade currency id.
fn currency_details_id(d: &Data, id: &str) -> crate::prices::DbQuery {
    d.trade_tag_to_ref(id).map_or_else(
        // Not a value, so nothing matches.
        || crate::prices::DbQuery { ns: "nan ns".into(), name: "nan item".into(), variant: None },
        |ref_name| crate::prices::DbQuery { ns: "ITEM".into(), name: ref_name.to_owned(), variant: None },
    )
}

/// `account.online ? (status === "afk" ? "afk" : "online") : "offline"`, with
/// JavaScript truthiness for the value.
fn account_status(online: Option<&Value>) -> AccountStatus {
    match online {
        Some(Value::Object(map)) => match map.get("status") {
            Some(Value::String(status)) if status == "afk" => AccountStatus::Afk,
            _ => AccountStatus::Online,
        },
        Some(value) if truthy(Some(value)) => AccountStatus::Online,
        _ => AccountStatus::Offline,
    }
}

/// `requestResults`' rows, for the user's `account_name` at `now`. A display
/// item that fails to build is left out, as upstream logs and skips it.
pub fn pricing_results(
    d: &Data,
    prices: &mut crate::prices::Prices,
    listings: &[FetchResult],
    account_name: &str,
    now: wraex_core::relative_time::EpochMillis,
) -> Vec<PricingResult> {
    listings
        .iter()
        .map(|result| {
            let item = &result.item;
            let listing = &result.listing;
            let account = &listing.account;
            let price = listing.price.as_ref();

            let mut currency = price.and_then(|p| p.currency.clone());
            let mut rank = None;
            if let Some(converted) =
                currency.as_deref().and_then(|c| CONVERT_CURRENCY.iter().find(|(id, _)| *id == c)).map(|(_, v)| *v)
            {
                rank = match converted.chars().next() {
                    Some('G') => Some(2),
                    Some('P') => Some(3),
                    _ => None,
                };
                currency = Some(converted.to_owned());
            }
            let amount = price.and_then(|p| p.amount);
            let currency = currency.unwrap_or_else(|| "no price".to_owned());

            let query = currency_details_id(d, &currency);
            let normalized = prices.cached_currency_by_query(&query, amount.unwrap_or(0.0));
            let normalized_price_currency = if normalized.is_some_and(|n| n.currency == "div") {
                Some(crate::prices::div_currency())
            } else {
                prices.selected_core_currency().cloned()
            };

            // `gemSockets.length`, else the fewest of the socketed items'.
            let len = |v: Option<&Vec<Value>>| v.map(Vec::len);
            let gem_sockets = len(item.gem_sockets.as_ref()).filter(|&n| n > 0).or_else(|| {
                let socketed = item.socketed_items.as_ref()?;
                socketed.iter().filter_map(|s| len(s.gem_sockets.as_ref())).min()
            });

            PricingResult {
                id: result.id.clone(),
                item_level: property(item, 78).map_or_else(|| text(item.ilvl.as_ref()), |v| text(Some(&v))),
                stack_size: item.stack_size.clone(),
                corrupted: item.corrupted.clone(),
                quality: property(item, 6),
                level: property(item, 5),
                gem_sockets,
                relative_date: listing
                    .indexed
                    .as_deref()
                    .and_then(|i| wraex_core::relative_time::iso_to_relative_short(i, now))
                    .unwrap_or_default(),
                price_amount: amount.unwrap_or(0.0),
                price_currency: currency,
                price_currency_rank: rank,
                normalized_price: normalized.map(|n| crate::prices::display_rounding(n.min, false, false)),
                normalized_price_currency,
                is_mine: account.name.as_deref() == Some(account_name),
                has_note: item.note.is_some(),
                is_instant_buyout: listing.fee.is_some(),
                account_name: text_of(account.name.as_deref()),
                account_status: account_status(account.online.as_ref()),
                ign: text_of(account.last_character_name.as_deref()),
                display_item: display_item(d, result).ok(),
                in_demand: listing.in_demand.clone(),
                gone: result.gone.clone(),
            }
        })
        .collect()
}

/// `${x}` of a string the site may omit (`undefined`).
fn text_of(value: Option<&str>) -> String {
    value.map_or_else(|| "undefined".to_owned(), str::to_owned)
}
