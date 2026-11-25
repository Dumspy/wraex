//! What a PoE2 price check shows from the trade site's answers:
//! `parseFetchResult` (`pathofexile-trade.ts`) builds the tooltip-like display
//! item of a fetched listing. Listings are read as JSON, as leniently as the
//! reference reads them; where it would throw (`values[0][1]` of an empty
//! list), the display item is an error, which upstream logs and leaves out.

use serde::Serialize;
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
#[derive(Debug, Clone, PartialEq, Serialize)]
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

fn color(c: f64) -> Option<Value> {
    Some(serde_json::json!(c))
}

/// `DisplayItem`
#[derive(Debug, Clone, PartialEq, Serialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Icon {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<Value>,
}

/// What `parseFetchResult` throws on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayError(pub String);

type R<T> = Result<T, DisplayError>;

/// `x` unless it is `undefined` or `null`.
fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key).filter(|x| !x.is_null())
}

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
fn first_value(values: &Value, i: usize) -> R<Option<&Value>> {
    let first =
        values.get(0).ok_or_else(|| DisplayError("Cannot read properties of undefined (reading '1')".into()))?;
    Ok(first.get(i))
}

fn values_len(values: Option<&Value>) -> usize {
    values.and_then(Value::as_array).map_or(0, Vec::len)
}

/// `parseFetchResult`
pub fn parse_fetch_result(d: &Data, result: &Value) -> R<DisplayItem> {
    let item = result.get("item").unwrap_or(&Value::Null);

    let mut title = Vec::new();
    if let Some(name) = get(item, "name").and_then(Value::as_str).filter(|n| !n.is_empty()) {
        title.push(name.to_owned());
    }
    if truthy(item.get("typeLine")) {
        title.push(text(item.get("typeLine")));
    }

    let mut item_tags = Vec::new();
    // Exclusive to every other tag but corrupted.
    if item.get("identified") == Some(&Value::Bool(false)) {
        let tag = if truthy(item.get("unidentifiedTier")) {
            format!("Unidentified (Tier {})", text(item.get("unidentifiedTier")))
        } else {
            "item.unidentified".to_owned()
        };
        item_tags.push(DisplayItemLine::new(tag, None, color(colors::UNMET)));
    }
    // The rest should be exclusive.
    if truthy(item.get("doubleCorrupted")) {
        item_tags.push(DisplayItemLine::new(d.s("DOUBLE_CORRUPTED"), None, color(colors::UNMET)));
    } else if truthy(item.get("corrupted")) {
        item_tags.push(DisplayItemLine::new(d.s("CORRUPTED"), None, color(colors::UNMET)));
    }
    if truthy(item.get("duplicated")) {
        item_tags.push(DisplayItemLine::new(d.s("MIRRORED"), None, color(colors::AUGMENTED)));
    }
    if truthy(item.get("sanctified")) {
        item_tags.push(DisplayItemLine::new(d.s("SANCTIFIED"), None, color(colors::SANCTIFIED)));
    }

    let extended = get(item, "extended");
    let mod_block = |key: &str, block_color: Option<f64>, kind: &str| {
        parse_mod_block(
            get(item, key),
            block_color.unwrap_or(colors::AUGMENTED),
            extended.and_then(|e| get(e, "mods")).and_then(|m| get(m, kind)),
            extended.and_then(|e| get(e, "hashes")).and_then(|h| get(h, kind)),
        )
    };

    let veiled_mods = get(item, "veiledMods").and_then(Value::as_array).map(|mods| {
        mods.iter()
            .map(|vm| {
                let t = match vm {
                    Value::String(s) => s.clone(),
                    other => text(other.get("description")),
                };
                let label = if t.starts_with("Prefix") { "Unrevealed Prefix" } else { "Unrevealed Suffix" };
                DisplayItemLine::new(label, None, color(colors::DESECRATED))
            })
            .collect()
    });

    Ok(DisplayItem {
        title,
        rarity: get(item, "rarity").cloned(),
        frame_type: get(item, "frameType").cloned(),
        name_block: build_name_block(extended, get(item, "properties"))?,
        item_props: build_item_props(item.get("ilvl"), get(item, "requirements"))?,
        grant_skill: build_grant_skill_block(get(item, "grantedSkills"))?,
        enchant_mods: mod_block("enchantMods", Some(colors::ENCHANT), "enchant"),
        rune_mods: mod_block("runeMods", Some(colors::ENCHANT), "rune"),
        implicit_mods: mod_block("implicitMods", None, "implicit"),
        fractured_mods: mod_block("fracturedMods", Some(colors::FRACTURED), "fractured"),
        explicit_mods: mod_block("explicitMods", None, "explicit"),
        crafted_mods: mod_block("craftedMods", None, "crafted"),
        desecrated_mods: mod_block("desecratedMods", Some(colors::DESECRATED), "desecrated"),
        mutated_mods: mod_block("mutatedMods", Some(colors::MUTATED), "mutated"),
        pseudo_mods: mod_block("pseudoMods", None, "pseudo"),
        veiled_mods,
        sockets: get(item, "sockets").cloned(),
        item_tags,
        icon: Icon { url: get(item, "icon").cloned(), w: get(item, "w").cloned(), h: get(item, "h").cloned() },
    })
}

/// `parseModBlock`: strings (tiers from `extended`) or objects (tiers inline).
/// A missing color is `TradeNumberColors.Augmented`, the parameter's default.
fn parse_mod_block(
    translated: Option<&Value>,
    block_color: f64,
    mods: Option<&Value>,
    hashes: Option<&Value>,
) -> Option<Vec<DisplayItemLine>> {
    let lines = translated?.as_array()?;
    if lines.is_empty() {
        return Some(Vec::new());
    }
    if lines[0].is_string() {
        return Some(
            lines
                .iter()
                .enumerate()
                .map(|(index, s)| DisplayItemLine {
                    text: parse_affix_strings(&text(Some(s))),
                    tier: get_tier(index, mods, hashes),
                    value: None,
                    color: color(block_color),
                })
                .collect(),
        );
    }
    Some(
        lines
            .iter()
            .map(|s| DisplayItemLine {
                text: parse_affix_strings(&text(s.get("description"))),
                tier: get_tier_v2(get(s, "mods")),
                value: None,
                color: color(block_color),
            })
            .collect(),
    )
}

/// `getTier`: the tiers of the mods a line's hash points to.
fn get_tier(display_index: usize, mods: Option<&Value>, hashes: Option<&Value>) -> Option<String> {
    let mods = mods?.as_array().filter(|m| !m.is_empty())?;
    let entry = hashes?.get(display_index)?;
    let indexes = get(entry, "1").or_else(|| entry.get(1)).filter(|x| !x.is_null())?.as_array()?;
    let tiers: Vec<String> = indexes
        .iter()
        .filter_map(|i| i.as_u64().and_then(|i| mods.get(i as usize)))
        .filter_map(|m| get(m, "tier"))
        .map(|t| text(Some(t)))
        .collect();
    Some(tiers.join(" + "))
}

/// `getTierV2`: the tiers of a line's mods (`undefined` joins as empty).
fn get_tier_v2(mods: Option<&Value>) -> Option<String> {
    let mods = mods?.as_array().filter(|m| !m.is_empty())?;
    Some(mods.iter().map(|m| get(m, "tier").map_or(String::new(), |t| text(Some(t)))).collect::<Vec<_>>().join(" + "))
}

/// `buildNameBlock`: the item's properties, defences and DPS from `extended`.
fn build_name_block(extended: Option<&Value>, props: Option<&Value>) -> R<Vec<DisplayItemLine>> {
    let (Some(extended), Some(props)) = (extended, props) else { return Ok(Vec::new()) };
    let ext = |key: &str| get(extended, key).cloned();
    // `useColor(aug)`
    let use_color = |key: &str| if truthy(extended.get(key)) { colors::AUGMENTED } else { colors::WHITE };

    let mut block: Vec<DisplayItemLine> = Vec::new();
    let mut has_damage = false;
    for p in props.as_array().map(Vec::as_slice).unwrap_or_default() {
        let values = p.get("values").unwrap_or(&Value::Null);
        let name = text(p.get("name"));
        // `useColor(aug) || values[0][1]`
        let or_value_color = |key: &str| -> R<Option<Value>> {
            let c = use_color(key);
            if c != 0.0 { Ok(color(c)) } else { Ok(first_value(values, 1)?.cloned()) }
        };
        let label: Option<String> = match p.get("type").and_then(Value::as_i64) {
            Some(prop::WEAPON_PHYSICAL_DAMAGE) => {
                block.push(DisplayItemLine::new("item.physical_dps", ext("pdps"), color(use_color("pdps_aug"))));
                has_damage = true;
                continue;
            }
            Some(prop::WEAPON_ELEMENTAL_DAMAGE) => {
                block.push(DisplayItemLine::new("item.elemental_dps", ext("edps"), color(use_color("edps_aug"))));
                has_damage = true;
                continue;
            }
            Some(prop::ARMOUR_REDUCTION) => {
                block.push(DisplayItemLine::new("item.armour", ext("ar"), or_value_color("ar_aug")?));
                continue;
            }
            Some(prop::ARMOUR_EVASION) => {
                block.push(DisplayItemLine::new("item.evasion_rating", ext("ev"), or_value_color("ev_aug")?));
                continue;
            }
            Some(prop::ARMOUR_ENERGY_SHIELD) => {
                block.push(DisplayItemLine::new("item.energy_shield", ext("es"), or_value_color("es_aug")?));
                continue;
            }
            Some(prop::ARMOUR_WARD) => {
                block.push(DisplayItemLine::new("item.runic_ward", ext("ward"), or_value_color("ward_aug")?));
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
            Some(prop::MAP_QUANTITY_BONUS) | Some(_) | None => None,
        };

        let has_values = values_len(Some(values)) > 0;
        // If there is a value, there should be just one.
        block.push(DisplayItemLine::new(
            label.unwrap_or_else(|| format!("{} ", parse_affix_strings(&name))),
            Some(if has_values { first_value(values, 0)?.cloned().unwrap_or(Value::Null) } else { Value::from("") }),
            if has_values { first_value(values, 1)?.cloned() } else { color(colors::WHITE) },
        ));
    }
    if has_damage {
        let index = block.iter().position(|l| l.text.contains("dps")).expect("a dps line was pushed");
        block.insert(index, DisplayItemLine::new("item.total_dps", ext("dps"), color(use_color("dps_aug"))));
    }
    Ok(block)
}

/// `buildItemProps`: item level and requirements.
fn build_item_props(ilvl: Option<&Value>, requirements: Option<&Value>) -> R<Option<Vec<DisplayItemLine>>> {
    if !truthy(ilvl) && requirements.is_none() {
        return Ok(None);
    }
    let mut block = Vec::new();
    if truthy(ilvl) {
        block.push(DisplayItemLine::new("item.item_level", ilvl.cloned(), color(colors::WHITE)));
    }
    if let Some(reqs) = requirements.and_then(Value::as_array).filter(|r| !r.is_empty()) {
        let first_text =
            |r: &Value| -> R<String> { Ok(text(first_value(r.get("values").unwrap_or(&Value::Null), 0)?)) };
        let mut value = first_value(reqs[0].get("values").unwrap_or(&Value::Null), 0)?.cloned().unwrap_or(Value::Null);
        if reqs.len() > 1 {
            let mut rest = Vec::new();
            for r in &reqs[1..] {
                rest.push(format!("{} {}", first_text(r)?, text(r.get("name"))));
            }
            value = Value::from(parse_affix_strings(&format!("{}, {}", text(Some(&value)), rest.join(", "))));
        }
        // No colon for requirements.
        block.push(DisplayItemLine::new(
            format!("{} ", parse_affix_strings(&text(reqs[0].get("name")))),
            Some(value),
            color(colors::WHITE),
        ));
    }
    Ok(Some(block))
}

/// `buildGrantSkillBlock`
fn build_grant_skill_block(skills: Option<&Value>) -> R<Vec<DisplayItemLine>> {
    let mut block = Vec::new();
    for skill in skills.and_then(Value::as_array).map(Vec::as_slice).unwrap_or_default() {
        let values = skill.get("values").unwrap_or(&Value::Null);
        block.push(DisplayItemLine::new(
            format!("{}: ", parse_affix_strings(&text(skill.get("name")))),
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

    #[test]
    fn rows_from_ee2s_recorded_answer() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/ee2/renderer/specs/docs/fetchResponses.json");
        let answer: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let listings = answer["result"].as_array().unwrap();
        let mut prices = crate::prices::Prices::default();
        let now = wraex_core::relative_time::parse_iso("2026-01-07T18:47:51Z").unwrap();
        let rows = pricing_results(en(), &mut prices, listings, "joukerone#4206", now);
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
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/ee2/renderer/specs/docs/fetchResponses.json");
        let answer: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let tiers = |prefix: &str| {
            let result = answer["result"]
                .as_array()
                .unwrap()
                .iter()
                .find(|result| result["id"].as_str().unwrap_or_default().starts_with(prefix))
                .unwrap_or_else(|| panic!("no result {prefix}"));
            let display = parse_fetch_result(en(), result).unwrap();
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
fn property(item: &Value, kind: i64) -> Option<Value> {
    let prop =
        item.get("properties")?.as_array()?.iter().find(|p| p.get("type").and_then(Value::as_i64) == Some(kind))?;
    prop.get("values")?.get(0)?.get(0).cloned()
}

/// `getCurrencyDetailsId`: the poe.ninja query for a trade currency id.
fn currency_details_id(d: &Data, id: &str) -> crate::prices::DbQuery {
    match d.trade_tag_to_ref(id) {
        Some(ref_name) => crate::prices::DbQuery { ns: "ITEM".into(), name: ref_name.to_owned(), variant: None },
        // Not a value, so nothing matches.
        None => crate::prices::DbQuery { ns: "nan ns".into(), name: "nan item".into(), variant: None },
    }
}

/// `requestResults`' rows, for the user's `account_name` at `now`. A display
/// item that fails to build is left out, as upstream logs and skips it.
pub fn pricing_results(
    d: &Data,
    prices: &mut crate::prices::Prices,
    listings: &[Value],
    account_name: &str,
    now: wraex_core::relative_time::EpochMillis,
) -> Vec<PricingResult> {
    listings
        .iter()
        .map(|result| {
            let item = result.get("item").unwrap_or(&Value::Null);
            let listing = result.get("listing").unwrap_or(&Value::Null);
            let account = listing.get("account").unwrap_or(&Value::Null);
            let price = get(listing, "price");

            let mut currency = price.and_then(|p| get(p, "currency")).map(|c| text(Some(c)));
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
            let amount = price.and_then(|p| p.get("amount")).and_then(Value::as_f64);
            let currency = currency.unwrap_or_else(|| "no price".to_owned());

            let query = currency_details_id(d, &currency);
            let normalized = prices.cached_currency_by_query(&query, amount.unwrap_or(0.0));
            let normalized_price_currency = if normalized.is_some_and(|n| n.currency == "div") {
                Some(crate::prices::div_currency())
            } else {
                prices.selected_core_currency().cloned()
            };

            // `gemSockets.length`, else the fewest of the socketed items'.
            let len = |v: Option<&Value>| v.and_then(Value::as_array).map(Vec::len);
            let gem_sockets = len(item.get("gemSockets")).filter(|&n| n > 0).or_else(|| {
                let socketed = item.get("socketedItems")?.as_array()?;
                socketed.iter().filter_map(|s| len(get(s, "gemSockets"))).min()
            });

            PricingResult {
                id: text(result.get("id")),
                item_level: property(item, 78).map_or_else(|| text(item.get("ilvl")), |v| text(Some(&v))),
                stack_size: get(item, "stackSize").cloned(),
                corrupted: get(item, "corrupted").cloned(),
                quality: property(item, 6),
                level: property(item, 5),
                gem_sockets,
                relative_date: listing
                    .get("indexed")
                    .and_then(Value::as_str)
                    .and_then(|i| wraex_core::relative_time::iso_to_relative_short(i, now))
                    .unwrap_or_default(),
                price_amount: amount.unwrap_or(0.0),
                price_currency: currency,
                price_currency_rank: rank,
                normalized_price: normalized.map(|n| crate::prices::display_rounding(n.min, false, false)),
                normalized_price_currency,
                is_mine: account.get("name").and_then(Value::as_str) == Some(account_name),
                has_note: get(item, "note").is_some(),
                is_instant_buyout: get(listing, "fee").is_some(),
                account_name: text(account.get("name")),
                account_status: match get(account, "online") {
                    Some(online) if online.get("status").and_then(Value::as_str) == Some("afk") => AccountStatus::Afk,
                    Some(_) => AccountStatus::Online,
                    None => AccountStatus::Offline,
                },
                ign: text(account.get("lastCharacterName")),
                display_item: parse_fetch_result(d, result).ok(),
                in_demand: get(listing, "in_demand").cloned(),
                gone: get(result, "gone").cloned(),
            }
        })
        .collect()
}
