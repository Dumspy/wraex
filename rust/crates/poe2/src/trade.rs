//! `ref/ee2/renderer/src/web/price-check/trade`: which API a preset searches
//! with (`common.ts`) and the search request for it (`pathofexile-trade.ts`
//! `createTradeRequest`; the bulk request is `wraex_core::trade`). Sending
//! requests comes with the services (PLAN phase 4).

use serde::Serialize;
use serde_json::{Map, Value, json};
use wraex_core::Rarity;
use wraex_core::trade::{BulkRequest, bulk_request, prop_set};

use crate::data::Data;
use crate::filters::create_item_filters::{area_level_by_ascendancy_points, category_to_trade_id};
use crate::filters::{
    FilterError, FilterNumeric, FilterRoll, INTERNAL_TRADE_IDS, ItemFilters, ItemIsElementalModifier, MinMax,
    StatFilter,
};
use crate::parser::item::Item;
use crate::parser::meta::ItemCategory;

type R<T> = Result<T, FilterError>;

/// `apiToSatisfySearch`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Api {
    Trade,
    Bulk,
}

/// `apiToSatisfySearch`: bulk for items with a trade tag and no enabled stat
/// filter; divination cards and waystones only with their stack size filter on.
pub fn api_to_satisfy_search(item: &Item, stats: &[StatFilter], filters: &ItemFilters) -> Api {
    if stats.iter().any(|s| !s.disabled) {
        return Api::Trade;
    }
    if let Some(stack) = &filters.stack_size
        && matches!(item.category, Some(ItemCategory::DivinationCard | ItemCategory::Map))
    {
        return if stack.disabled { Api::Trade } else { Api::Bulk };
    }
    if item.info().trade_tag.is_some() { Api::Bulk } else { Api::Trade }
}

/// `createTradeRequest` (`pathofexile-bulk.ts`). Securable and available
/// listings don't exist for bulk; those become `online`.
pub fn create_bulk_request(filters: &ItemFilters, item: &Item, have: Vec<String>) -> BulkRequest {
    let status = match filters.trade.listing_type.as_str() {
        "securable" | "available" => "online".to_owned(),
        other => other.to_owned(),
    };
    let minimum = filters.stack_size.as_ref().filter(|s| !s.disabled).map(|s| s.value);
    bulk_request(have, item.info().trade_tag.clone(), status, minimum)
}

const EMPTY_MODIFIERS: [&str; 3] = ["# Empty Modifiers", "# Empty Prefix Modifiers", "# Empty Suffix Modifiers"];

/// The search request (`/api/trade2/search`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TradeRequest {
    pub query: Query,
    pub sort: Sort,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sort {
    pub price: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Query {
    pub status: Status,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<NameQuery>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<NameQuery>,
    pub stats: Vec<StatGroup>,
    /// Built path by path, as `dot-prop` does.
    pub filters: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Status {
    pub option: String,
}

/// `nameToQuery`
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum NameQuery {
    Name(String),
    Discriminated {
        #[serde(skip_serializing_if = "Option::is_none")]
        discriminator: Option<String>,
        option: String,
    },
}

fn name_to_query(name: &str, filters: &ItemFilters) -> NameQuery {
    match &filters.discriminator {
        None => NameQuery::Name(name.to_owned()),
        Some(d) => NameQuery::Discriminated { discriminator: d.trade.clone(), option: name.to_owned() },
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatGroup {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Range>,
    pub filters: Vec<StatQuery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Range {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatQuery {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Range>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

fn group(kind: &'static str, value: Option<Range>, filters: Vec<StatQuery>, disabled: Option<bool>) -> StatGroup {
    StatGroup { kind, value, filters, disabled }
}

/// `stat.tradeId`, which throws when used while `undefined`.
fn trade_ids(stat: &StatFilter) -> R<&[String]> {
    stat.trade_id.as_deref().ok_or_else(|| FilterError(format!("no trade id: {}", stat.stat_ref)))
}

/// `stat.tradeId[0]`
fn first_id(stat: &StatFilter) -> R<Option<&str>> {
    Ok(trade_ids(stat)?.first().map(String::as_str))
}

fn roll(stat: &StatFilter) -> R<&FilterRoll> {
    stat.roll.as_ref().ok_or_else(|| FilterError(format!("no roll: {}", stat.stat_ref)))
}

fn stat_trade_id(d: &Data, stat_ref: &str, kind: &str) -> R<String> {
    d.stat_by_ref(stat_ref)
        .and_then(|s| s.trade.ids.and_then(|ids| ids.get(kind).and_then(|v| v.first().cloned())))
        .ok_or_else(|| FilterError(format!("no {kind} trade id: {stat_ref}")))
}

/// `createTradeRequest`
pub fn create_trade_request(d: &Data, filters: &ItemFilters, stats: &[StatFilter], item: &Item) -> R<TradeRequest> {
    let trade = &filters.trade;
    // Online-in-league listings don't exist for trade.
    let listing_type = if trade.listing_type == "onlineleague" { "securable" } else { trade.listing_type.as_str() };
    let mut q = Query {
        status: Status { option: listing_type.to_owned() },
        name: None,
        kind: None,
        stats: vec![group("and", None, Vec::new(), None)],
        filters: Map::new(),
    };
    let f = &mut q.filters;
    let set = |f: &mut Map<String, Value>, path: &str, value: Value| prop_set(f, path, Some(value));
    let text = |b: bool| json!(b.to_string());
    let enabled = |n: &Option<FilterNumeric>| n.as_ref().filter(|n| !n.disabled).cloned();
    let truthy = |x: Option<f64>| x.filter(|v| *v != 0.0 && !v.is_nan());

    if let Some(currency) = trade.currency.as_deref().filter(|c| !c.is_empty()) {
        set(f, "trade_filters.filters.price.option", json!(currency));
    }
    if trade.collapse_listings == "api" {
        set(f, "trade_filters.filters.collapse.option", text(true));
    }
    if let Some(listed) = trade.listed.as_deref().filter(|l| !l.is_empty()) {
        set(f, "trade_filters.filters.indexed.option", json!(listed));
    }

    // Search by category, not base type?
    let active = match &filters.search_relaxed {
        Some(relaxed) if relaxed.disabled != Some(true) => relaxed,
        _ => &filters.search_exact,
    };
    let nonempty = |s: &Option<String>| s.clone().filter(|s| !s.is_empty());
    if let Some(name) = nonempty(&active.name_trade).or_else(|| nonempty(&active.name)) {
        q.name = Some(name_to_query(&name, filters));
    }
    if let Some(base) = nonempty(&active.base_type_trade).or_else(|| nonempty(&active.base_type)) {
        q.kind = Some(name_to_query(&base, filters));
    }

    // Type filters
    if let Some(category) = &active.category {
        let id = category_to_trade_id(category)
            .ok_or_else(|| FilterError(format!("Invalid category: {}", category.as_str())))?;
        set(f, "type_filters.filters.category.option", json!(id));
    }

    if filters.foil.as_ref().is_some_and(|x| !x.disabled) {
        set(f, "type_filters.filters.rarity.option", json!("uniquefoil"));
    } else if let Some(rarity) = &filters.rarity {
        set(f, "type_filters.filters.rarity.option", json!(rarity.value));
    }

    if let Some(n) = enabled(&filters.item_level) {
        set(f, "type_filters.filters.ilvl.min", json!(n.value));
        if let Some(max) = truthy(n.max) {
            set(f, "type_filters.filters.ilvl.max", json!(max));
        }
    }

    if let Some(level) = filters.requires.as_ref().and_then(|r| r.level.as_ref()).filter(|l| !l.disabled) {
        set(f, "req_filters.filters.lvl.max", json!(level.value));
    }

    if let Some(n) = enabled(&filters.quality) {
        set(f, "type_filters.filters.quality.min", json!(n.value));
    }

    // Equipment filters
    if let Some(n) = enabled(&filters.augment_sockets) {
        set(f, "equipment_filters.filters.rune_sockets.min", json!(n.value));
    }

    // Map (waystone) filters
    if let Some(n) = enabled(&filters.map_tier) {
        set(f, "map_filters.filters.map_tier.min", json!(n.value));
        set(f, "map_filters.filters.map_tier.max", json!(n.value));
    }
    if let Some(hint) = filters.ultimatum_hint.as_ref().filter(|h| !h.disabled) {
        set(f, "map_filters.filters.ultimatum_hint.option", json!(hint.value));
    }

    // Misc filters
    if let Some(n) = enabled(&filters.gem_level) {
        set(f, "misc_filters.filters.gem_level.min", json!(n.value));
        if let Some(max) = truthy(n.max) {
            set(f, "misc_filters.filters.gem_level.max", json!(max));
        }
    }
    if let Some(n) = enabled(&filters.socket_number) {
        set(f, "misc_filters.filters.gem_sockets.min", json!(n.value));
    }
    if let Some(n) = enabled(&filters.area_level) {
        // With ascendancy points a lower area level is worth more: exclude
        // the worse (higher) ones.
        let bound = if filters.awarded_ascendancy_points.is_some() { "max" } else { "min" };
        set(f, &format!("misc_filters.filters.area_level.{bound}"), json!(n.value));
    }
    if let Some(points) = enabled(&filters.awarded_ascendancy_points) {
        let ref_name = item.info().ref_name.as_deref().unwrap_or("undefined");
        let level = area_level_by_ascendancy_points(ref_name, points.value).map(|l| json!(l));
        prop_set(f, "misc_filters.filters.area_level.min", level);
    }
    if filters.unidentified.as_ref().is_some_and(|u| !u.disabled) {
        set(f, "misc_filters.filters.identified.option", text(false));
    }
    if let Some(tier) = filters.unidentified_tier.as_ref().filter(|t| !t.disabled) {
        set(f, "misc_filters.filters.unidentified_tier.min", json!(tier.value));
    }

    let sanctified_off = filters.sanctified.as_ref().is_none_or(|s| s.disabled);
    if let Some(corrupted) = &filters.corrupted
        && (!corrupted.value || corrupted.exact == Some(true))
        && sanctified_off
    {
        set(f, "misc_filters.filters.corrupted.option", text(corrupted.value));
    }

    if filters.fractured.as_ref().is_some_and(|x| !x.value) {
        set(f, "misc_filters.filters.fractured_item.option", text(false));
    }

    let craftable_rarity = matches!(item.rarity, Some(Rarity::Normal | Rarity::Magic | Rarity::Rare));
    match &filters.mirrored {
        Some(mirrored) => {
            if mirrored.disabled {
                set(f, "misc_filters.filters.mirrored.option", text(false));
            }
        }
        None if craftable_rarity => set(f, "misc_filters.filters.mirrored.option", text(false)),
        None => {}
    }

    let corrupted_only = filters.corrupted.as_ref().is_some_and(|c| c.value && c.exact != Some(true));
    if filters.sanctified.is_some() || corrupted_only {
        if filters.sanctified.as_ref().is_some_and(|s| s.disabled) {
            set(f, "misc_filters.filters.sanctified.option", text(false));
        }
    } else if craftable_rarity {
        set(f, "misc_filters.filters.sanctified.option", text(false));
    }

    // Internal stats: empty modifiers, and dps/pdps/edps as filters.
    for stat in stats {
        let id = first_id(stat)?;
        if id == Some("item.has_empty_modifier") {
            let option = stat
                .option
                .and_then(|o| o.value)
                .and_then(|v| EMPTY_MODIFIERS.get(v as usize))
                .ok_or_else(|| FilterError("bad empty modifier option".to_owned()))?;
            let empty_id = stat_trade_id(d, option, "pseudo")?;
            let (min, max) = get_min_max(Some(roll(stat)?));
            q.stats.push(group(
                "count",
                Some(Range { min: Some(1.0), max: Some(1.0) }),
                vec![StatQuery { id: empty_id, value: Some(Range { min, max }), disabled: Some(stat.disabled) }],
                Some(stat.disabled),
            ));
        } else if item.category == Some(ItemCategory::Flask)
            && stat.stat_ref == "#% increased Charge Recovery"
            && !stats.iter().any(|s| s.stat_ref == "#% increased effect")
        {
            // https://github.com/SnosMe/awakened-poe-trade/issues/758
            let reduced_effect = stat_trade_id(d, "#% increased effect", "explicit")?;
            q.stats.push(group(
                "not",
                None,
                vec![StatQuery { id: reduced_effect, value: None, disabled: Some(stat.disabled) }],
                Some(stat.disabled),
            ));
        }

        if stat.disabled {
            continue;
        }

        let range_path = match id {
            Some("item.armour") => Some("equipment_filters.filters.ar"),
            Some("item.evasion_rating") => Some("equipment_filters.filters.ev"),
            Some("item.energy_shield") => Some("equipment_filters.filters.es"),
            Some("item.runic_ward") => Some("equipment_filters.filters.ward"),
            Some("item.block") => Some("equipment_filters.filters.block"),
            Some("item.total_dps") => Some("equipment_filters.filters.dps"),
            Some("item.physical_dps") => Some("equipment_filters.filters.pdps"),
            Some("item.elemental_dps") => Some("equipment_filters.filters.edps"),
            Some("item.crit") => Some("equipment_filters.filters.crit"),
            Some("item.aps") => Some("equipment_filters.filters.aps"),
            Some("item.spirit") => Some("equipment_filters.filters.spirit"),
            Some("item.reload_time") => Some("equipment_filters.filters.reload_time"),
            _ => None,
        };
        // FIXME (upstream): should also include *.max here
        let min_path = match id {
            Some("item.map_revives") => Some("map_filters.filters.map_revives"),
            Some("item.map_pack_size") => Some("map_filters.filters.map_packsize"),
            Some("item.map_magic_monsters") => Some("map_filters.filters.map_magic_monsters"),
            Some("item.map_rare_monsters") => Some("map_filters.filters.map_rare_monsters"),
            Some("item.map_drop_chance") => Some("map_filters.filters.map_bonus"),
            Some("item.map_item_rarity") => Some("map_filters.filters.map_iir"),
            Some("item.map_gold") => Some("map_filters.filters.map_gold"),
            _ => None,
        };

        if let Some(path) = range_path {
            let input = roll(stat)?;
            prop_set(&mut q.filters, &format!("{path}.min"), input.min.map(|m| json!(m)));
            prop_set(&mut q.filters, &format!("{path}.max"), input.max.map(|m| json!(m)));
            if id == Some("item.elemental_dps") {
                apply_elemental_damage_logical_filter(d, stat, &mut q.stats)?;
            }
        } else if let Some(path) = min_path {
            let input = roll(stat)?;
            prop_set(&mut q.filters, &format!("{path}.min"), input.min.map(|m| json!(m)));
        } else if id == Some("item.rarity_magic") {
            set(&mut q.filters, "type_filters.filters.rarity.option", json!("magic"));
        } else if id == Some("item.duplicates") {
            if !item.ref_name_is("Mageblood") {
                return Err(FilterError("Duplicates filter applied to non-mageblood".to_owned()));
            }
            // A count of the legacies; the ones on this item are disabled.
            let legacies: Vec<&StatFilter> = stats.iter().filter(|s| s.stat_ref.starts_with("Legacy of")).collect();
            if let Some(filter) = build_mage_blood_not_filter(stat, &legacies, &mut q.stats[0].filters)? {
                q.stats.push(filter);
            }
        }
    }

    let mut real_stats = Vec::new();
    for stat in stats {
        if !first_id(stat)?.is_some_and(|id| INTERNAL_TRADE_IDS.contains(&id)) {
            real_stats.push(stat);
        }
    }

    for stat in real_stats {
        if stat.stat_ref == "Only affects Passives in # Ring" {
            let meta = roll(stat)?;
            let count = meta.bounds.ok_or_else(|| FilterError("no bounds".to_owned()))?.max;
            let current = meta.min;
            let base = first_id(stat)?.unwrap_or("undefined");
            let filters = (0..count.max(0.0) as usize)
                .map(|index| StatQuery {
                    id: format!("{base}|{}", index + 1),
                    value: None,
                    disabled: Some(Some((index + 1) as f64) != current),
                })
                .collect();
            q.stats.push(group("count", Some(Range { min: Some(1.0), max: None }), filters, Some(stat.disabled)));
            continue;
        }
        let ids = trade_ids(stat)?;
        if ids.len() == 1 {
            q.stats[0].filters.push(trade_id_to_query(&ids[0], stat));
        } else {
            let filters = ids.iter().map(|id| trade_id_to_query(id, stat)).collect();
            q.stats.push(group("count", Some(Range { min: Some(1.0), max: None }), filters, Some(stat.disabled)));
        }
    }

    if let Some(veiled) = filters.veiled.as_ref().filter(|v| !v.disabled) {
        set(&mut q.filters, "misc_filters.filters.veiled.option", text(true));
        // HACK (upstream): a pseudo stat for the veiled count, not shown in the UI.
        let count = veiled.veiled_count;
        let id = "pseudo.pseudo_number_of_unrevealed_mods";
        let pseudo = StatFilter {
            roll: Some(FilterRoll {
                value: count,
                min: Some(count),
                max: None,
                default: MinMax { min: count, max: count },
                bounds: None,
                trade_invert: None,
                dp: false,
                is_negated: false,
                goodness: None,
            }),
            stat_ref: "# Unrevealed Modifiers".to_owned(),
            ..StatFilter::plain(id, "Unrevealed Modifiers", crate::filters::FilterTag::PSEUDO, false)
        };
        q.stats[0].filters.push(trade_id_to_query(id, &pseudo));
    }

    Ok(TradeRequest { query: q, sort: Sort { price: "asc" } })
}

/// `applyElementalDamageLogicalFilter`: a `not` group for the other elements.
fn apply_elemental_damage_logical_filter(d: &Data, stat: &StatFilter, all: &mut Vec<StatGroup>) -> R<()> {
    let ids_of = |stat_ref: &str| -> R<Vec<StatQuery>> {
        let stat = d.stat_by_ref(stat_ref).ok_or_else(|| FilterError(format!("no stat: {stat_ref}")))?;
        let ids = stat.trade.ids.ok_or_else(|| FilterError(format!("no trade ids: {stat_ref}")))?;
        Ok(ids.into_values().flatten().map(|id| StatQuery { id, value: None, disabled: None }).collect())
    };
    let selected = stat.option.and_then(|o| o.value);
    let is = |m: ItemIsElementalModifier| selected == Some(f64::from(m as u8));
    let (fire, cold, lightning) =
        ("Adds # to # Fire Damage", "Adds # to # Cold Damage", "Adds # to # Lightning Damage");
    let others: &[&str] = if is(ItemIsElementalModifier::Fire) {
        &[cold, lightning]
    } else if is(ItemIsElementalModifier::Cold) {
        &[fire, lightning]
    } else if is(ItemIsElementalModifier::Lightning) {
        &[fire, cold]
    } else {
        &[]
    };
    let mut not_group = Vec::new();
    for stat_ref in others {
        not_group.extend(ids_of(stat_ref)?);
    }
    if !not_group.is_empty() {
        all.push(group("not", None, not_group, None));
    }
    Ok(())
}

/// `getMinMax`
fn get_min_max(roll: Option<&FilterRoll>) -> (Option<f64>, Option<f64>) {
    let Some(roll) = roll else { return (None, None) };
    let invert = roll.trade_invert == Some(true);
    let sign = if invert { -1.0 } else { 1.0 };
    let a = roll.min.map(|m| m * sign);
    let b = roll.max.map(|m| m * sign);
    if invert { (b, a) } else { (a, b) }
}

/// `tradeIdToQuery`: an option goes into the id (`id|option`).
fn trade_id_to_query(id: &str, stat: &StatFilter) -> StatQuery {
    let id = match &stat.option {
        Some(option) => match option.value {
            Some(v) => format!("{id}|{}", wraex_core::js::number_to_string(v)),
            None => format!("{id}|undefined"),
        },
        None => id.to_owned(),
    };
    let (min, max) = get_min_max(stat.roll.as_ref());
    StatQuery { id, value: Some(Range { min, max }), disabled: Some(stat.disabled) }
}

/// `buildFilterWithValue`
fn build_filter_with_value(filter: &StatFilter, roll: Option<(f64, Option<f64>)>) -> R<StatQuery> {
    let id = first_id(filter)?.unwrap_or("undefined").to_owned();
    let Some((value, min)) = roll else { return Ok(trade_id_to_query(&id, filter)) };
    let with_roll = StatFilter {
        roll: Some(FilterRoll {
            value,
            min,
            max: None,
            default: MinMax { min: value, max: value },
            bounds: None,
            trade_invert: None,
            dp: false,
            is_negated: false,
            goodness: None,
        }),
        ..filter.clone()
    };
    Ok(trade_id_to_query(&id, &with_roll))
}

/// `EASY_LEGACY_DUPLICATE_TO_FILTER`: `(count, values)` per duplicate count.
fn easy_legacy_duplicate(dup_count: f64) -> Option<(f64, Vec<Range>)> {
    let min = |m: f64| Range { min: Some(m), max: None };
    if dup_count == 3.0 {
        Some((1.0, vec![min(4.0)]))
    } else if dup_count == 2.0 {
        Some((4.0, vec![min(1.0), min(2.0), min(3.0)]))
    } else if dup_count == 1.0 {
        Some((7.0, vec![min(1.0), min(2.0), Range { min: None, max: Some(2.0) }]))
    } else {
        None
    }
}

/// `buildMageBloodNotFilter`: the item's legacies go into the `and` group; a
/// count group covers the duplicates.
fn build_mage_blood_not_filter(
    duplicate_stat: &StatFilter,
    legacies: &[&StatFilter],
    q_and: &mut Vec<StatQuery>,
) -> R<Option<StatGroup>> {
    let mut count_filter = group("count", None, Vec::new(), None);
    let push_all = |q_and: &mut Vec<StatQuery>, roll: Option<(f64, Option<f64>)>| -> R<()> {
        for s in legacies {
            q_and.push(build_filter_with_value(s, roll)?);
        }
        Ok(())
    };

    let known_count = legacies.iter().filter(|s| !s.disabled).count() as f64;
    let dup_roll = roll(duplicate_stat)?;
    let dup_count = dup_roll.min.unwrap_or(dup_roll.value);
    if dup_count + known_count > 4.0 {
        return Err(FilterError("not valid Mageblood, reduce duplicates or unselect a Legacy".to_owned()));
    }

    // Disabled: no duplicate counting, the legacies only.
    if duplicate_stat.disabled || dup_count == 0.0 {
        push_all(q_and, None)?;
        return Ok(None);
    }

    // The "easy cases": known 1-3 and duplicates filling up to 4.
    if known_count + dup_count == 4.0 {
        if dup_count == 3.0 {
            push_all(q_and, Some((4.0, Some(4.0))))?;
            return Ok(None);
        }
        if dup_count == 2.0 || dup_count == 1.0 {
            push_all(q_and, None)?;
            let (count, values) = easy_legacy_duplicate(dup_count).expect("1 or 2");
            count_filter.value = Some(Range { min: Some(count), max: None });
            for s in legacies {
                let leg = build_filter_with_value(s, None)?;
                for v in &values {
                    count_filter.filters.push(StatQuery { value: Some(*v), ..leg.clone() });
                }
            }
            return Ok(Some(count_filter));
        }
    }

    // The "hard cases": known 1 dup 1, known 2 dup 1, known 1 dup 2.
    push_all(q_and, None)?;

    let some_legacy = legacies.first().ok_or_else(|| FilterError("no legacy".to_owned()))?;
    let trade_id = first_id(some_legacy)?.unwrap_or("undefined").split('|').next().unwrap_or_default().to_owned();
    let source = some_legacy.sources.first().ok_or_else(|| FilterError("no legacy source".to_owned()))?;
    let legacy_ids: Vec<String> = source
        .stat
        .stat
        .matchers
        .iter()
        .map(|m| match m.value {
            Some(v) => format!("{trade_id}|{}", wraex_core::js::number_to_string(v)),
            None => format!("{trade_id}|undefined"),
        })
        .collect();

    count_filter.value = Some(Range { min: Some(1.0), max: None });
    let min = if dup_count == 1.0 {
        Some(2.0)
    } else if dup_count == 2.0 {
        Some(1.0)
    } else {
        None
    };
    if let Some(min) = min {
        count_filter.filters.extend(legacy_ids.into_iter().map(|id| StatQuery {
            id,
            value: Some(Range { min: Some(min), max: None }),
            disabled: None,
        }));
    }
    Ok(Some(count_filter))
}
