//! `ref/apt/renderer/src/web/price-check/trade`: which API a preset searches
//! with (`common.ts`) and the search request for it (`pathofexile-trade.ts`
//! `createTradeRequest`; the bulk request is `wraex_core::trade`).
//!
//! Sending requests comes with the services (PLAN phase 4).

use serde::Serialize;
use serde_json::{Map, Value};
use wraex_core::num::{f64_to_usize, usize_to_f64};
use wraex_core::trade::{BulkRequest, bulk_request, prop_set};

use crate::data::{Data, Stat};
use crate::filters::create_item_filters::category_to_trade_id;
use crate::filters::pseudo::mercenary::SearchMode;
use crate::filters::{
    FilterError, FilterGroup, FilterNumeric, FilterOrGroup, FilterRoll, FilterSource, FilterTag, GroupKind,
    INTERNAL_TRADE_IDS, ItemFilters, MapBlighted, SearchFilter, StatFilter,
};
use crate::parser::item::{Item, ItemInfluence};

type R<T> = Result<T, FilterError>;

/// `apiToSatisfySearch`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Api {
    Trade,
    Bulk,
}

/// `apiToSatisfySearch`: the search API, bulk only for an item with a trade tag
/// and no enabled stat filter.
pub fn api_to_satisfy_search(item: &Item, stats: &[FilterOrGroup]) -> Api {
    let any_enabled = stats.iter().any(|s| match s {
        FilterOrGroup::Group(g) => !g.meta.disabled,
        FilterOrGroup::Stat(s) => !s.disabled,
    });
    if any_enabled || item.info().trade_tag.is_none() { Api::Trade } else { Api::Bulk }
}

/// `createTradeRequest` (`pathofexile-bulk.ts`).
pub fn create_bulk_request(filters: &ItemFilters, item: &Item, have: Vec<String>) -> BulkRequest {
    let status = if filters.trade.offline {
        "any"
    } else if filters.trade.online_in_league {
        "onlineleague"
    } else {
        "online"
    };
    let minimum = filters.stack_size.as_ref().filter(|s| !s.disabled).map(|s| s.value);
    bulk_request(have, item.info().trade_tag.clone(), status.to_owned(), minimum)
}

const CRAFTED_MODIFIERS: [&str; 3] =
    ["# Crafted Modifiers", "# Crafted Prefix Modifiers", "# Crafted Suffix Modifiers"];
const EMPTY_MODIFIERS: [&str; 3] = ["# Empty Modifiers", "# Empty Prefix Modifiers", "# Empty Suffix Modifiers"];
const TOTAL_MODIFIERS: [&str; 3] = ["# Modifiers", "# Prefix Modifiers", "# Suffix Modifiers"];

/// `INFLUENCE_PSEUDO_TEXT`
const fn influence_pseudo_text(influence: ItemInfluence) -> &'static str {
    match influence {
        ItemInfluence::Shaper => "Has Shaper Influence",
        ItemInfluence::Crusader => "Has Crusader Influence",
        ItemInfluence::Hunter => "Has Hunter Influence",
        ItemInfluence::Elder => "Has Elder Influence",
        ItemInfluence::Redeemer => "Has Redeemer Influence",
        ItemInfluence::Warlord => "Has Warlord Influence",
    }
}

/// The search request (`/api/trade/search`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TradeRequest {
    pub query: Query,
    pub sort: Sort,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Sort {
    pub price: &'static str,
}

/// The query object, in the order `createTradeRequest` builds it.
///
/// `status`, `stats` and `filters` come from the object literal, and `name`/`type`
/// are assigned after them. `serde_json` keeps that order (`preserve_order`), so a
/// request body is byte-identical to the reference's `JSON.stringify` output.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Query {
    pub status: Status,
    pub stats: Vec<StatGroup>,
    /// Built path by path, as `dot-prop` does.
    pub filters: Map<String, Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<NameQuery>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub kind: Option<NameQuery>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    /// `online`, `securable`, `available` or `any`.
    pub option: &'static str,
}

/// `nameToQuery`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum NameQuery {
    Name(String),
    Discriminated { discriminator: String, option: String },
}

fn name_to_query(name: &str, discriminator: Option<&str>) -> NameQuery {
    discriminator.filter(|d| !d.is_empty()).map_or_else(
        || NameQuery::Name(name.to_owned()),
        |d| NameQuery::Discriminated { discriminator: d.to_owned(), option: name.to_owned() },
    )
}

/// `TradeStatGroup`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatGroup {
    #[serde(rename = "type")]
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<Range>,
    /// Upstream writes `{ type, value, disabled, filters }` for `count` groups and
    /// `{ type, disabled, filters }` for `not` groups; one field order covers both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    pub filters: Vec<StatQuery>,
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
    pub value: Option<StatValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct StatValue {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub option: Option<f64>,
}

/// `BareStatFilter` plus `not`: what the stat queries are built from.
#[derive(Debug, Clone)]
struct Bare<'a> {
    trade_id: Vec<String>,
    roll: Option<&'a FilterRoll>,
    option: Option<f64>,
    disabled: bool,
    not: bool,
}

impl<'a> Bare<'a> {
    fn of(stat: &'a StatFilter) -> R<Self> {
        Ok(Self {
            trade_id: trade_ids(stat)?.to_vec(),
            roll: stat.roll.as_ref(),
            option: stat.option.as_ref().map(|o| o.value),
            disabled: stat.disabled,
            not: stat.not == Some(true),
        })
    }
}

/// `stat.tradeId`, which throws when used while `undefined`.
fn trade_ids(stat: &StatFilter) -> R<&[String]> {
    stat.trade_id.as_deref().ok_or_else(|| FilterError(format!("no trade id: {}", stat.stat_ref)))
}

/// `stat.tradeId[0]`
fn first_id(stat: &StatFilter) -> R<Option<&str>> {
    Ok(trade_ids(stat)?.first().map(String::as_str))
}

fn is_internal(id: Option<&str>) -> bool {
    id.is_some_and(|id| INTERNAL_TRADE_IDS.contains(&id))
}

/// `String(bool)`: the trade site's option filters take `"true"`/`"false"`.
fn bool_text(b: bool) -> Value {
    Value::from(b.to_string())
}

/// `isSearchModeOptional`/`isSearchModeRequired`: the option a mercenary gem
/// filter carries its [`SearchMode`] in.
#[expect(clippy::float_cmp, reason = "upstream compares the option number to the enum's value with ===")]
fn is_search_mode(option: f64, mode: SearchMode) -> bool {
    option
        == match mode {
            SearchMode::Optional => 0.0,
            SearchMode::Required => 1.0,
        }
}

/// `trade_filters` and `type_filters`: price, listings, rarity and category.
fn trade_and_type_filters(f: &mut Map<String, Value>, filters: &ItemFilters, active: &SearchFilter) -> R<()> {
    let trade = &filters.trade;
    if let Some(currency) = trade.currency.as_deref().filter(|c| !c.is_empty()) {
        prop_set(f, "trade_filters.filters.price.option", Some(Value::from(currency)));
    }

    if trade.collapse_listings == "api" && (trade.offline || !trade.merchant_only || trade.collapse_merchant) {
        prop_set(f, "trade_filters.filters.collapse.option", Some(bool_text(true)));
    }

    if let Some(listed) = trade.listed.as_deref().filter(|l| !l.is_empty()) {
        prop_set(f, "trade_filters.filters.indexed.option", Some(Value::from(listed)));
    }

    if filters.foil.as_ref().is_some_and(|f| !f.disabled) {
        prop_set(f, "type_filters.filters.rarity.option", Some(Value::from("uniquefoil")));
    } else if let Some(rarity) = &filters.rarity {
        let option = if rarity.disabled { "nonunique" } else { rarity.value };
        prop_set(f, "type_filters.filters.rarity.option", Some(Value::from(option)));
    }

    if let Some(category) = &active.category {
        let id = category_to_trade_id(category)
            .ok_or_else(|| FilterError(format!("Invalid category: {}", category.as_str())))?;
        prop_set(f, "type_filters.filters.category.option", Some(Value::from(id)));
    }
    Ok(())
}

/// The boolean `misc_filters` options.
fn misc_flag_filters(f: &mut Map<String, Value>, filters: &ItemFilters) {
    if let Some(corrupted) = &filters.corrupted {
        if !corrupted.value || corrupted.exact == Some(true) {
            prop_set(f, "misc_filters.filters.corrupted.option", Some(bool_text(corrupted.value)));
        }
        if corrupted.value && !corrupted.twice {
            prop_set(f, "misc_filters.filters.twice_corrupted.option", Some(bool_text(corrupted.twice)));
        }
    }
    if filters.fractured.as_ref().is_some_and(|x| !x.value) {
        prop_set(f, "misc_filters.filters.fractured_item.option", Some(bool_text(false)));
    }
    if filters.imbued_gem.as_ref().is_some_and(|x| x.disabled) {
        prop_set(f, "misc_filters.filters.gem_imbued.option", Some(bool_text(false)));
    }
    if filters.split.as_ref().is_some_and(|x| x.disabled) {
        prop_set(f, "misc_filters.filters.split.option", Some(bool_text(false)));
    }
    if filters.foulborn.as_ref().is_some_and(|x| !x.value) {
        prop_set(f, "misc_filters.filters.foulborn_item.option", Some(bool_text(false)));
    }
    if filters.vestigial.as_ref().is_some_and(|x| !x.value) {
        prop_set(f, "misc_filters.filters.vestigial.option", Some(bool_text(false)));
    }
    if filters.mirrored.as_ref().is_some_and(|x| x.disabled) {
        prop_set(f, "misc_filters.filters.mirrored.option", Some(bool_text(false)));
    }
    if filters.not_catalysed.as_ref().is_some_and(|x| !x.disabled) {
        prop_set(f, "misc_filters.filters.quality.max", Some(Value::from(0)));
    }
}

/// The `map_filters` options: the blight state and the completion reward.
fn map_option_filters(f: &mut Map<String, Value>, filters: &ItemFilters) {
    if let Some(blighted) = &filters.map_blighted {
        match blighted.value {
            MapBlighted::Blighted => {
                prop_set(f, "map_filters.filters.map_blighted.option", Some(bool_text(true)));
            }
            MapBlighted::BlightRavaged => {
                prop_set(f, "map_filters.filters.map_uberblighted.option", Some(bool_text(true)));
            }
            MapBlighted::No => {
                prop_set(f, "map_filters.filters.map_blighted.option", Some(bool_text(false)));
                prop_set(f, "map_filters.filters.map_uberblighted.option", Some(bool_text(false)));
            }
        }
    }

    if let Some(reward) = &filters.map_completion_reward {
        prop_set(f, "map_filters.filters.map_completion_reward.option", Some(Value::from(reward.name_trade.as_str())));
    }
}

/// The numeric `misc_filters`, `socket_filters` and `map_filters` minimums and
/// maximums.
fn numeric_filters(f: &mut Map<String, Value>, filters: &ItemFilters) {
    let enabled = |n: &Option<FilterNumeric>| n.as_ref().filter(|n| !n.disabled).cloned();
    let truthy = |x: Option<f64>| x.filter(|v| *v != 0.0 && !v.is_nan());

    if let Some(n) = enabled(&filters.gem_level) {
        prop_set(f, "misc_filters.filters.gem_level.min", Some(Value::from(n.value)));
    }
    if let Some(n) = enabled(&filters.quality) {
        prop_set(f, "misc_filters.filters.quality.min", Some(Value::from(n.value)));
    }
    if let Some(n) = enabled(&filters.item_level) {
        prop_set(f, "misc_filters.filters.ilvl.min", Some(Value::from(n.value)));
        if let Some(max) = truthy(n.max) {
            prop_set(f, "misc_filters.filters.ilvl.max", Some(Value::from(max)));
        }
    }
    if let Some(n) = enabled(&filters.stack_size) {
        prop_set(f, "misc_filters.filters.stack_size.min", Some(Value::from(n.value)));
    }
    if let Some(n) = enabled(&filters.linked_sockets) {
        prop_set(f, "socket_filters.filters.links.min", Some(Value::from(n.value)));
    }
    if let Some(n) = enabled(&filters.white_sockets) {
        prop_set(f, "socket_filters.filters.sockets.w", Some(Value::from(n.value)));
    }
    if let Some(n) = enabled(&filters.map_tier) {
        prop_set(f, "map_filters.filters.map_tier.min", Some(Value::from(n.value)));
        prop_set(f, "map_filters.filters.map_tier.max", Some(Value::from(n.value)));
    }
}

/// The `map_filters` area level and `sentinel_filters` durability minimums.
fn area_filters(f: &mut Map<String, Value>, filters: &ItemFilters) {
    let enabled = |n: &Option<FilterNumeric>| n.as_ref().filter(|n| !n.disabled).cloned();
    let truthy = |x: Option<f64>| x.filter(|v| *v != 0.0 && !v.is_nan());

    if let Some(n) = enabled(&filters.area_level) {
        prop_set(f, "map_filters.filters.area_level.min", Some(Value::from(n.value)));
        if let Some(max) = truthy(n.max) {
            prop_set(f, "map_filters.filters.area_level.max", Some(Value::from(max)));
        }
    }
    if let Some(n) = enabled(&filters.sentinel_charge) {
        prop_set(f, "sentinel_filters.filters.sentinel_durability.min", Some(Value::from(n.value)));
    }
}

/// The trade filter path an `item.*` stat writes its roll to, and the minimum
/// `createTradeRequest` defaults to when the roll has none.
fn stat_filter_path(id: &str) -> Option<(&'static str, Option<f64>)> {
    Some(match id {
        "item.base_percentile" => ("armour_filters.filters.base_defence_percentile", None),
        "item.memory_strands" => ("misc_filters.filters.memory_level", None),
        "item.armour" => ("armour_filters.filters.ar", None),
        "item.evasion_rating" => ("armour_filters.filters.ev", None),
        "item.energy_shield" => ("armour_filters.filters.es", None),
        "item.ward" => ("armour_filters.filters.ward", None),
        "item.block" => ("armour_filters.filters.block", None),
        "item.total_dps" => ("weapon_filters.filters.dps", None),
        "item.physical_dps" => ("weapon_filters.filters.pdps", None),
        "item.elemental_dps" => ("weapon_filters.filters.edps", None),
        "item.crit" => ("weapon_filters.filters.crit", None),
        "item.aps" => ("weapon_filters.filters.aps", None),
        "item.map_item_quantity" => ("map_filters.filters.map_iiq", None),
        "item.map_atlas_drops" => ("map_filters.filters.map_bonus", None),
        "item.map_item_rarity" => ("map_filters.filters.map_iir", None),
        "item.map_pack_size" => ("map_filters.filters.map_packsize", None),
        "item.heist_job_agility" => ("heist_filters.filters.heist_agility", Some(1.0)),
        "item.heist_job_bruteforce" => ("heist_filters.filters.heist_brute_force", Some(1.0)),
        "item.heist_job_counterthaumaturgy" => ("heist_filters.filters.heist_counter_thaumaturgy", Some(1.0)),
        "item.heist_job_deception" => ("heist_filters.filters.heist_deception", Some(1.0)),
        "item.heist_job_demolition" => ("heist_filters.filters.heist_demolition", Some(1.0)),
        "item.heist_job_engineering" => ("heist_filters.filters.heist_engineering", Some(1.0)),
        "item.heist_job_lockpicking" => ("heist_filters.filters.heist_lockpicking", Some(1.0)),
        "item.heist_job_perception" => ("heist_filters.filters.heist_perception", Some(1.0)),
        "item.heist_job_trapdisarmament" => ("heist_filters.filters.heist_trap_disarmament", Some(1.0)),
        "item.heist_wings_revealed" => ("heist_filters.filters.heist_wings", None),
        "item.heist_wings_total" => ("heist_filters.filters.heist_max_wings", None),
        "item.chart_sulphur" => ("map_filters.filters.chart_sulphur", None),
        _ => return None,
    })
}

/// `createTradeRequest`'s `item.*` stat filters, including the "empty modifier"
/// count groups they push to `groups`.
fn stat_filters(f: &mut Map<String, Value>, groups: &mut Vec<StatGroup>, d: &Data, stats: &[FilterOrGroup]) -> R<()> {
    for stat in stats {
        let FilterOrGroup::Stat(stat) = stat else { continue };
        let id = first_id(stat)?.ok_or_else(|| FilterError(format!("empty trade id: {}", stat.stat_ref)))?;
        if !id.starts_with("item.") {
            continue;
        }

        if id == "item.has_empty_modifier" {
            let option = stat.option.as_ref().map(|o| o.value).ok_or_else(|| FilterError("no option".to_owned()))?;
            let pseudo_id = |text: &str| -> R<String> {
                d.pseudo_stat_by_ref(text)
                    .and_then(|s| s.trade.ids.get("pseudo").and_then(|ids| ids.first().cloned()))
                    .ok_or_else(|| FilterError(format!("no pseudo stat: {text}")))
            };
            let slot = |list: &[&'static str; 3]| -> R<&'static str> {
                list.get(f64_to_usize(option)).copied().ok_or_else(|| FilterError("bad option".to_owned()))
            };
            let crafted = pseudo_id(slot(&CRAFTED_MODIFIERS)?)?;
            let empty = pseudo_id(slot(&EMPTY_MODIFIERS)?)?;
            let Some(total_text) = TOTAL_MODIFIERS.first().copied() else {
                return Err(FilterError("no total modifier stat".to_owned()));
            };
            let total = pseudo_id(total_text)?;
            let disabled = Some(stat.disabled);
            let exactly_one = Some(Range { min: Some(1.0), max: Some(1.0) });
            for (other, min) in [(crafted, 1.0), (total, 6.0)] {
                groups.push(StatGroup {
                    kind: "count",
                    value: exactly_one,
                    disabled,
                    filters: vec![
                        StatQuery {
                            id: empty.clone(),
                            value: Some(StatValue { min: Some(1.0), max: Some(1.0), option: None }),
                            disabled,
                        },
                        StatQuery {
                            id: other,
                            value: Some(StatValue { min: Some(min), ..Default::default() }),
                            disabled,
                        },
                    ],
                });
            }
        }

        if stat.disabled {
            continue;
        }

        let input = || stat.roll.as_ref().ok_or_else(|| FilterError(format!("no roll: {}", stat.stat_ref)));
        let range = |f: &mut Map<String, Value>, path: &str, default_min: Option<f64>| -> R<()> {
            let roll = input()?;
            prop_set(f, &format!("{path}.min"), roll.min.or(default_min).map(Value::from));
            prop_set(f, &format!("{path}.max"), roll.max.map(Value::from));
            Ok(())
        };
        if let Some((path, default_min)) = stat_filter_path(id) {
            range(f, path, default_min)?;
        } else if id == "item.heist_target_priceless" {
            prop_set(f, "heist_filters.filters.heist_objective_value.option", Some(Value::from("priceless")));
        }
    }
    Ok(())
}

/// Every stat the request queries after the stats loop: the enabled stats, the
/// veiled ones and the influences, each with its trade ids.
fn bare_stats<'a>(d: &Data, filters: &'a ItemFilters, stats: &'a [FilterOrGroup]) -> R<Vec<Bare<'a>>> {
    let mut real_stats: Vec<Bare> = Vec::new();
    for stat in stats {
        if let FilterOrGroup::Stat(stat) = stat
            && !is_internal(first_id(stat)?)
        {
            real_stats.push(Bare::of(stat)?);
        }
    }
    if let Some(veiled) = &filters.veiled {
        for stat_ref in &veiled.stat_refs {
            let stat_or_group =
                d.stat_by_ref_v2(stat_ref).ok_or_else(|| FilterError(format!("no stat: {stat_ref}")))?;
            let trade_id = stat_or_group
                .stats()
                .iter()
                .filter_map(|s| s.trade.ids.get("veiled").and_then(|ids| ids.first().cloned()))
                .collect();
            real_stats.push(Bare { trade_id, roll: None, option: None, disabled: veiled.disabled, not: false });
        }
    }
    if let Some(influences) = &filters.influences {
        for influence in influences {
            let text = influence_pseudo_text(influence.value);
            let trade_id = d
                .pseudo_stat_by_ref(text)
                .and_then(|s| s.trade.ids.get("pseudo").cloned())
                .ok_or_else(|| FilterError(format!("no pseudo stat: {text}")))?;
            real_stats.push(Bare { trade_id, roll: None, option: None, disabled: influence.disabled, not: false });
        }
    }
    Ok(real_stats)
}

/// `createTradeRequest`
pub fn create_trade_request(d: &Data, filters: &ItemFilters, stats: &[FilterOrGroup]) -> R<TradeRequest> {
    let trade = &filters.trade;
    let mut q = Query {
        status: Status {
            option: if trade.offline {
                "any"
            } else if trade.merchant_only {
                "securable"
            } else {
                "available"
            },
        },
        name: None,
        kind: None,
        stats: vec![StatGroup { kind: "and", value: None, filters: Vec::new(), disabled: None }],
        filters: Map::new(),
    };

    let mut active = match &filters.search_relaxed {
        Some(relaxed) if relaxed.disabled != Some(true) => relaxed,
        _ => &filters.search_exact,
    };
    if let Some(sub) = active.sub.as_deref().filter(|s| s.disabled != Some(true)) {
        active = sub;
    }
    let nonempty = |s: &Option<String>| s.clone().filter(|s| !s.is_empty());
    let disc = active.discriminator_trade.as_deref();

    if let Some(name) = nonempty(&active.name_trade).or_else(|| nonempty(&active.name)) {
        q.name = Some(name_to_query(&name, disc));
    }
    if let Some(base) = nonempty(&active.base_type_trade).or_else(|| nonempty(&active.base_type)) {
        q.kind = Some(name_to_query(&base, disc));
    }

    // The order of these calls is the order of the keys in the request body.
    trade_and_type_filters(&mut q.filters, filters, active)?;
    misc_flag_filters(&mut q.filters, filters);
    numeric_filters(&mut q.filters, filters);
    map_option_filters(&mut q.filters, filters);
    if filters.unidentified.as_ref().is_some_and(|u| !u.disabled) {
        prop_set(&mut q.filters, "misc_filters.filters.identified.option", Some(bool_text(false)));
    }
    area_filters(&mut q.filters, filters);
    stat_filters(&mut q.filters, &mut q.stats, d, stats)?;

    // Primary mercenary skills are always searched.
    let stats: Vec<FilterOrGroup> = stats
        .iter()
        .map(|s| match s {
            FilterOrGroup::Stat(stat) if stat.tag == FilterTag::MercenaryPrimary => {
                FilterOrGroup::Stat(StatFilter { disabled: false, ..stat.clone() })
            }
            other => other.clone(),
        })
        .collect();
    let real_stats = bare_stats(d, filters, &stats)?;

    let mut q_and: Vec<StatQuery> = Vec::new();
    let mut q_not: Vec<StatQuery> = Vec::new();

    for group in &stats {
        let FilterOrGroup::Group(group) = group else { continue };
        match group.group {
            GroupKind::Not => {
                let mut filters = Vec::new();
                for stat in &group.stats {
                    filters.extend(every_trade_id_to_query(&Bare::of(stat)?));
                }
                q.stats.push(StatGroup { kind: "not", value: None, filters, disabled: Some(group.meta.disabled) });
            }
            GroupKind::One => {
                let mut filters = Vec::new();
                for stat in &group.stats {
                    filters.extend(every_trade_id_to_query(&Bare::of(stat)?));
                }
                q.stats.push(StatGroup {
                    kind: "count",
                    value: Some(Range { min: Some(1.0), max: None }),
                    filters,
                    disabled: Some(group.meta.disabled || group.stats.iter().all(|s| s.disabled)),
                });
            }
            GroupKind::Mercenary => mercenary_groups(group, &mut q_and, &mut q_not, &mut q.stats)?,
        }
    }

    for stat in &real_stats {
        if stat.not {
            q_not.extend(every_trade_id_to_query(stat));
        } else {
            append_and_filter(stat, &mut q_and, &mut q.stats);
        }
    }

    // The `and` group was pushed first; the stats above fill it.
    if let Some(first) = q.stats.first_mut() {
        first.filters = q_and;
    }
    if !q_not.is_empty() {
        q.stats.push(StatGroup { kind: "not", value: None, filters: q_not, disabled: None });
    }

    Ok(TradeRequest { query: q, sort: Sort { price: "asc" } })
}

/// The `mercenary` branch of `createTradeRequest`'s group loop.
fn mercenary_groups(
    group: &FilterGroup,
    q_and: &mut Vec<StatQuery>,
    q_not: &mut Vec<StatQuery>,
    all: &mut Vec<StatGroup>,
) -> R<()> {
    let skill = &group.meta;
    let stats = &group.stats;
    let skill_ids = trade_ids(skill)?.to_vec();

    if skill.tag == FilterTag::MercenaryPrimary {
        append_and_filter(&Bare { disabled: false, ..Bare::of(skill)? }, q_and, all);
    } else if !skill.disabled {
        append_and_filter(&Bare::of(skill)?, q_and, all);
    }

    let mut socketed: Vec<&StatFilter> = Vec::new();
    for stat in stats {
        if stat.not != Some(true) && !is_internal(first_id(stat)?) {
            socketed.push(stat);
        }
    }
    let option = |s: &StatFilter| -> R<f64> {
        s.option.as_ref().map(|o| o.value).ok_or_else(|| FilterError(format!("no option: {}", s.stat_ref)))
    };
    let mut optional = Vec::new();
    let mut required = Vec::new();
    for &stat in &socketed {
        if stat.disabled {
            continue;
        }
        let o = option(stat)?;
        if is_search_mode(o, SearchMode::Optional) {
            optional.push(stat);
        }
        if is_search_mode(o, SearchMode::Required) {
            required.push(stat);
        }
    }

    let mut local_not_mode = false;
    for stat in stats {
        if first_id(stat)? == Some("item.mercenary_6link") {
            local_not_mode = true;
        }
    }
    let local_not_stats: Vec<&StatFilter> =
        if local_not_mode { stats.iter().filter(|s| s.not == Some(true) && !s.disabled).collect() } else { Vec::new() };

    for stat in stats {
        if skill.disabled || required.len() == 5 {
            break;
        }

        if stat.not == Some(true) {
            if local_not_mode {
                continue;
            }
            // Only when enabled, so the web UI isn't cluttered for players who
            // open the search in a browser to check its filters.
            if !stat.disabled {
                q_not.extend(every_trade_id_to_query(&Bare::of(stat)?));
            }
        } else if first_id(stat)? == Some("item.mercenary_6link") {
            all.extend(mercenary_6link_groups(stat, &skill_ids, &local_not_stats, &required, &optional)?);
        }
    }

    if optional.len() >= 2 {
        // Mixed N-1 and AND group
        let mut all_of = vec![skill_ids];
        for stat in &required {
            all_of.push(trade_ids(stat)?.to_vec());
        }
        let mut some_of = Vec::new();
        for stat in &optional {
            some_of.push(trade_ids(stat)?.to_vec());
        }
        all.push(weighted_group(&all_of, usize_to_f64(optional.len()) - 1.0, some_of));
    } else {
        // AND group; not weighted, for a better trade site experience. A skill
        // without checked support gems is a simple AND filter.
        let mut filters = every_trade_id_to_query(&Bare::of(skill)?);
        for &stat in &socketed {
            let disabled = if is_search_mode(option(stat)?, SearchMode::Optional) { true } else { stat.disabled };
            filters.extend(every_trade_id_to_query(&Bare { option: None, disabled, ..Bare::of(stat)? }));
        }
        all.push(StatGroup {
            kind: "mercenary",
            value: (!skill.disabled && !required.is_empty())
                .then_some(Range { min: Some(1.0 + usize_to_f64(required.len())), max: None }),
            disabled: Some(skill.disabled || required.is_empty()),
            filters,
        });
    }
    Ok(())
}

/// `mercenary`'s `item.mercenary_6link` branch: the weighted 6-link and tier-3
/// gem groups.
fn mercenary_6link_groups(
    stat: &StatFilter,
    skill_ids: &[String],
    local_not_stats: &[&StatFilter],
    required: &[&StatFilter],
    optional: &[&StatFilter],
) -> R<Vec<StatGroup>> {
    let force_enabled = stat.disabled && !local_not_stats.is_empty();
    if stat.disabled && !force_enabled {
        return Ok(Vec::new());
    }

    let mut possible_supports: Vec<Vec<&Stat>> = Vec::new();
    for source in &stat.sources {
        let family: Vec<&Stat> = match source {
            FilterSource::Family(family) => family.iter().collect(),
            FilterSource::Stat(s) => s.modifier.stats.iter().map(|p| &p.stat).collect(),
        };
        let first = family.first().ok_or_else(|| FilterError("empty support family".to_owned()))?;
        let canonical = first.mercenary.as_ref().and_then(|m| m.canonical.as_deref());
        let excluded =
            local_not_stats.iter().any(|n| Some(n.stat_ref.as_str()) == canonical || n.stat_ref == first.stat_ref);
        if !excluded {
            possible_supports.push(family);
        }
    }
    let mut tier3_count = stat.roll.as_ref().and_then(|r| r.min).map_or(0.0, |m| m.clamp(0.0, 5.0));

    let max_tier = |s: &StatFilter| s.mercenary.as_ref().is_some_and(|m| m.max_tier == Some(true));
    let required_max = usize_to_f64(required.iter().filter(|s| max_tier(s)).count());
    let optional_surplus =
        if optional.len() >= 2 { usize_to_f64(optional.iter().filter(|s| max_tier(s)).count()) - 1.0 } else { 0.0 };
    if force_enabled || required_max + optional_surplus >= tier3_count {
        tier3_count = 0.0;
    }

    let pseudo = |s: &Stat| s.trade.ids.get("pseudo").cloned().unwrap_or_default();
    let mut groups = Vec::new();
    if tier3_count < 5.0 {
        // 6-Link group
        let ids = possible_supports
            .iter()
            .map(|family| {
                let mut family = family.clone();
                if family.len() >= 3 {
                    let synthetic = family
                        .first()
                        .and_then(|s| s.mercenary.as_ref())
                        .is_some_and(|m| m.synthetic_family == Some(true));
                    let min_tier = if synthetic { 3 } else { 2 };
                    family.retain(|s| s.mercenary.as_ref().and_then(|m| m.tier).is_some_and(|t| t >= min_tier));
                }
                family.iter().flat_map(|s| pseudo(s)).collect()
            })
            .collect();
        groups.push(weighted_group(&[skill_ids.to_vec()], 5.0, ids));
    }

    if tier3_count > 0.0 {
        // Tier-3 gems group: the last gem of a family counts as tier 3;
        // a "not" filter can override that ("Knockback (Tier: 1)").
        let ids = possible_supports.iter().map(|family| family.last().map(|s| pseudo(s)).unwrap_or_default()).collect();
        groups.push(weighted_group(&[skill_ids.to_vec()], tier3_count, ids));
    }
    Ok(groups)
}

/// `{ type: 'mercenary', disabled: false, ...weightedGroupToQuery({ allOf, someOf }) }`
fn weighted_group(all_of: &[Vec<String>], some_of_min: f64, some_of_ids: Vec<Vec<String>>) -> StatGroup {
    // The largest possible surplus from `someOf`; `allOf` must outweigh it.
    let surplus = (usize_to_f64(some_of_ids.len()) - some_of_min).max(0.0);
    let weight = surplus + 1.0;
    let all_min = usize_to_f64(all_of.len()) * weight;
    let total_min = some_of_min + all_min;

    let mut flat_ids: Vec<String> = Vec::new();
    for family in all_of {
        for id in family {
            for _ in 0..f64_to_usize(weight) {
                flat_ids.push(id.clone());
            }
        }
    }
    flat_ids.extend(some_of_ids.into_iter().flatten());

    StatGroup {
        kind: "mercenary",
        value: Some(Range { min: Some(total_min), max: None }),
        filters: flat_ids.into_iter().map(|id| StatQuery { id, value: None, disabled: None }).collect(),
        disabled: Some(false),
    }
}

/// `appendAndFilter`
fn append_and_filter(stat: &Bare, and_group: &mut Vec<StatQuery>, all: &mut Vec<StatGroup>) {
    if let [id] = stat.trade_id.as_slice() {
        and_group.push(trade_id_to_query(id, stat));
    } else {
        all.push(StatGroup {
            kind: "count",
            value: Some(Range { min: Some(1.0), max: None }),
            disabled: Some(stat.disabled),
            filters: every_trade_id_to_query(stat),
        });
    }
}

/// `everyTradeIdToQuery`
fn every_trade_id_to_query(stat: &Bare) -> Vec<StatQuery> {
    stat.trade_id.iter().map(|id| trade_id_to_query(id, stat)).collect()
}

/// `tradeIdToQuery`: `{div_by_100}`, `{empty}` and `{empty_if_100}` prefixes
/// adjust the roll, then go.
fn trade_id_to_query(id: &str, stat: &Bare) -> StatQuery {
    let mut roll = stat.roll;
    let divisor = if id.starts_with("{div_by_100}") { 100.0 } else { 1.0 };
    #[expect(clippy::float_cmp, reason = "upstream compares the roll to exactly 100 with ===")]
    if id.starts_with("{empty}") || (id.starts_with("{empty_if_100}") && roll.is_some_and(|r| r.value == 100.0)) {
        roll = None;
    }
    let id = id.strip_prefix('{').map_or(id, |rest| {
        let Some(end) = rest.find('}') else { return id };
        rest.get(end.saturating_add(1)..).unwrap_or(id)
    });

    let (min, max) = get_min_max(roll, divisor);
    StatQuery {
        id: id.to_owned(),
        value: Some(StatValue { min, max, option: stat.option }),
        disabled: Some(stat.disabled),
    }
}

/// `getMinMax`
fn get_min_max(roll: Option<&FilterRoll>, divisor: f64) -> (Option<f64>, Option<f64>) {
    let Some(roll) = roll else { return (None, None) };
    let invert = roll.trade_invert == Some(true);
    let sign = if invert { -1.0 } else { 1.0 };
    let a = roll.min.map(|m| m * sign / divisor);
    let b = roll.max.map(|m| m * sign / divisor);
    if invert { (b, a) } else { (a, b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trade_id_prefixes_adjust_the_roll() {
        let roll = FilterRoll {
            value: 100.0,
            min: Some(100.0),
            max: None,
            default: crate::filters::MinMax { min: 100.0, max: 100.0 },
            bounds: None,
            trade_invert: None,
            dp: false,
            is_negated: false,
            goodness: None,
        };
        let bare =
            |id: &str| Bare { trade_id: vec![id.into()], roll: Some(&roll), option: None, disabled: false, not: false };
        let q = trade_id_to_query("{div_by_100}explicit.stat_1", &bare("x"));
        assert_eq!((q.id.as_str(), q.value.unwrap().min), ("explicit.stat_1", Some(1.0)));
        let q = trade_id_to_query("{empty_if_100}explicit.stat_2", &bare("x"));
        assert_eq!(q.value.unwrap().min, None);
    }

    #[test]
    fn weighted_groups_outweigh_the_optional_ids() {
        let g = weighted_group(&[vec!["a".into()]], 1.0, vec![vec!["b".into()], vec!["c".into()]]);
        assert_eq!(g.value.unwrap().min, Some(3.0));
        assert_eq!(g.filters.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["a", "a", "b", "c"]);
    }
}
