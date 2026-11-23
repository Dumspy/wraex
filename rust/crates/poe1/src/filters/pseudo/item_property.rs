//! `filters/pseudo/item-property.ts`: armour and weapon properties (at 20%
//! quality), base percentile and memory strands as filters.

use indexmap::IndexMap;

use super::super::create_stat_filters::{Ctx, calculated_stat_to_filter};
use super::super::{FilterSource, FilterTag, R, StatFilter};
use crate::data::{Stat, StatMatcher, StatTrade, stat_better};
use crate::parser::advanced_mod_desc::{ModifierInfo, ParsedModifier};
use crate::parser::calc_q20::{PropStats, calc_prop_bounds, prop_at_20_quality, quality_stats};
use crate::parser::item::Item;
use crate::parser::meta::{ARMOUR, ItemCategory, WEAPON};
use crate::parser::modifiers::{ModifierType, StatCalculated, StatRoll, StatSource};
use crate::parser::stat_translations::{ParsedStat, StatRollInfo};

pub const BASE_PCTL_AFFECTED_IDS: [&str; 4] = ["item.armour", "item.evasion_rating", "item.energy_shield", "item.ward"];

/// `propToFilter`'s options.
#[derive(Debug, Clone)]
pub struct Prop {
    pub stat_ref: &'static str,
    pub trade_id: &'static str,
    pub roll: Option<StatRoll>,
    pub sources: Vec<FilterSource>,
    pub dp: bool,
    pub disabled: Option<bool>,
    pub hidden: Option<&'static str>,
}

impl Prop {
    pub fn new(stat_ref: &'static str, trade_id: &'static str, roll: Option<StatRoll>) -> Self {
        Self { stat_ref, trade_id, roll, sources: Vec::new(), dp: false, disabled: None, hidden: None }
    }
}

/// A roll from 0 to `max`.
pub fn up_to(value: f64, max: f64) -> Option<StatRoll> {
    Some(StatRoll { value, min: 0.0, max })
}

/// `Number.MAX_SAFE_INTEGER`
pub const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

fn in_set(item: &Item, set: &[ItemCategory]) -> bool {
    item.category.as_ref().is_some_and(|c| set.contains(c))
}

/// `filterEquipmentProps`
pub fn filter_equipment_props(ctx: &mut Ctx) {
    if in_set(ctx.item, ARMOUR) {
        armour_props(ctx);
    }
    if in_set(ctx.item, WEAPON) {
        weapon_props(ctx);
    }
}

/// `filterBasePercentile`
pub fn filter_base_percentile(ctx: &mut Ctx) {
    if let Some(pct) = ctx.item.base_percentile {
        let prop = Prop {
            disabled: Some(pct < 50.0),
            ..Prop::new("Base Percentile: #%", "item.base_percentile", up_to(pct, 100.0))
        };
        let filter = prop_to_filter(prop, ctx.item, ctx.search_in_range);
        ctx.filters.push(filter);
    }
}

/// `filterMemoryStrands`
pub fn filter_memory_strands(ctx: &mut Ctx, hidden: Option<&'static str>) {
    if let Some(strands) = ctx.item.memory_strands {
        let prop = Prop {
            disabled: Some(strands < 60.0 || hidden.is_some()),
            hidden,
            ..Prop::new("Memory Strands: #", "item.memory_strands", up_to(strands, 100.0))
        };
        let filter = prop_to_filter(prop, ctx.item, ctx.search_in_range);
        ctx.filters.push(filter);
    }
}

fn truthy(x: Option<f64>) -> Option<f64> {
    x.filter(|v| *v != 0.0 && !v.is_nan())
}

const CHANCE_TO_BLOCK: &str = "+#% Chance to Block";

/// `ARMOUR_STATS`
fn is_armour_stat(stat_ref: &str) -> bool {
    use quality_stats::{ARMOUR as AR, ENERGY_SHIELD as ES, EVASION as EV, WARD};
    [AR, EV, ES, WARD].iter().any(|p| p.flat.contains(&stat_ref) || p.incr.contains(&stat_ref))
        || stat_ref == CHANCE_TO_BLOCK
}

fn armour_props(ctx: &mut Ctx) {
    let item = ctx.item;
    let base_info = item.unique_base.as_ref().unwrap_or(item.info());
    let bounds = |b: Option<[u32; 2]>| b.map(|[min, max]| [f64::from(min), f64::from(max)]);
    let armour = base_info.armour.as_ref();
    let single = is_single_attr_armour(item);

    let props = [
        (item.armour_ar, quality_stats::ARMOUR, armour.and_then(|a| a.ar), "Armour: #", "item.armour"),
        (item.armour_ev, quality_stats::EVASION, armour.and_then(|a| a.ev), "Evasion Rating: #", "item.evasion_rating"),
        (
            item.armour_es,
            quality_stats::ENERGY_SHIELD,
            armour.and_then(|a| a.es),
            "Energy Shield: #",
            "item.energy_shield",
        ),
        (item.armour_ward, quality_stats::WARD, armour.and_then(|a| a.ward), "Ward: #", "item.ward"),
    ];
    for (value, stats, base, stat_ref, trade_id) in props {
        let Some(value) = truthy(value) else { continue };
        let total_q20 = prop_at_20_quality(value, stats, bounds(base), item);
        let prop = Prop {
            sources: total_q20.sources.into_iter().map(FilterSource::Stat).collect(),
            disabled: Some(!single),
            ..Prop::new(stat_ref, trade_id, Some(total_q20.roll))
        };
        let filter = prop_to_filter(prop, item, ctx.search_in_range);
        ctx.filters.push(filter);
    }

    if let Some(block) = truthy(item.armour_block) {
        let block = calc_prop_bounds(block, PropStats { flat: &[CHANCE_TO_BLOCK], incr: &[] }, item);
        let prop = Prop {
            sources: block.sources.into_iter().map(FilterSource::Stat).collect(),
            disabled: Some(true),
            ..Prop::new("Block: #%", "item.block", Some(block.roll))
        };
        let filter = prop_to_filter(prop, item, ctx.search_in_range);
        ctx.filters.push(filter);
    }

    if [item.armour_ar, item.armour_ev, item.armour_es, item.armour_ward, item.armour_block]
        .into_iter()
        .any(|v| truthy(v).is_some())
    {
        ctx.stats_by_type.retain(|m| !is_armour_stat(&m.stat.stat_ref));
    }
}

const ATTACK_SPEED: &str = "#% increased Attack Speed";
const CRIT_CHANCE: &str = "#% increased Critical Strike Chance";
const ELEMENTAL_FLAT: [&str; 3] =
    ["Adds # to # Lightning Damage", "Adds # to # Cold Damage", "Adds # to # Fire Damage"];

/// `WEAPON_STATS`. Not `Adds # to # Chaos Damage`.
fn is_weapon_stat(stat_ref: &str) -> bool {
    let phys = quality_stats::PHYSICAL_DAMAGE;
    phys.flat.contains(&stat_ref)
        || phys.incr.contains(&stat_ref)
        || stat_ref == ATTACK_SPEED
        || stat_ref == CRIT_CHANCE
        || ELEMENTAL_FLAT.contains(&stat_ref)
}

fn weapon_props(ctx: &mut Ctx) {
    let item = ctx.item;
    let range = ctx.search_in_range;

    let attack_speed =
        calc_prop_bounds(item.weapon_as.unwrap_or(0.0), PropStats { incr: &[ATTACK_SPEED], flat: &[] }, item);
    let phys_q20 = prop_at_20_quality(item.weapon_physical.unwrap_or(0.0), quality_stats::PHYSICAL_DAMAGE, None, item);
    let aps = attack_speed.roll;
    let pdps_q20 = StatRoll {
        value: phys_q20.roll.value * aps.value,
        min: phys_q20.roll.min * aps.min,
        max: phys_q20.roll.max * aps.max,
    };

    let ele_dmg =
        calc_prop_bounds(item.weapon_elemental.unwrap_or(0.0), PropStats { flat: &ELEMENTAL_FLAT, incr: &[] }, item);
    let edps = StatRoll {
        value: ele_dmg.roll.value * aps.value,
        min: ele_dmg.roll.min * aps.min,
        max: ele_dmg.roll.max * aps.max,
    };
    let dps =
        StatRoll { value: pdps_q20.value + edps.value, min: pdps_q20.min + edps.min, max: pdps_q20.max + edps.max };

    if truthy(item.weapon_elemental).is_some() {
        if truthy(item.weapon_physical).is_some() {
            let prop = Prop { disabled: Some(false), ..Prop::new("Total DPS: #", "item.total_dps", Some(dps)) };
            ctx.filters.push(prop_to_filter(prop, item, range));
        }

        let minor = edps.value / dps.value < 0.67;
        let prop = Prop {
            disabled: Some(minor),
            hidden: minor.then_some("filters.hide_ele_dps"),
            ..Prop::new("Elemental DPS: #", "item.elemental_dps", Some(edps))
        };
        ctx.filters.push(prop_to_filter(prop, item, range));
    }

    if truthy(item.weapon_physical).is_some() {
        let minor = pdps_q20.value / dps.value < 0.67;
        let prop = Prop {
            disabled: Some(!is_pdps_important(item) || minor),
            hidden: minor.then_some("filters.hide_phys_dps"),
            ..Prop::new("Physical DPS: #", "item.physical_dps", Some(pdps_q20))
        };
        ctx.filters.push(prop_to_filter(prop, item, range));
    }

    let prop = Prop {
        sources: attack_speed.sources.into_iter().map(FilterSource::Stat).collect(),
        dp: true,
        disabled: Some(true),
        ..Prop::new("Attacks per Second: #", "item.aps", Some(aps))
    };
    ctx.filters.push(prop_to_filter(prop, item, range));

    if let Some(crit) = truthy(item.weapon_crit) {
        let crit = calc_prop_bounds(crit, PropStats { incr: &[CRIT_CHANCE], flat: &[] }, item);
        let prop = Prop {
            sources: crit.sources.into_iter().map(FilterSource::Stat).collect(),
            dp: true,
            disabled: Some(true),
            ..Prop::new("Critical Strike Chance: #%", "item.crit", Some(crit.roll))
        };
        ctx.filters.push(prop_to_filter(prop, item, range));
    }

    if [item.weapon_as, item.weapon_crit, item.weapon_elemental, item.weapon_physical]
        .into_iter()
        .any(|v| truthy(v).is_some())
    {
        ctx.stats_by_type.retain(|m| !is_weapon_stat(&m.stat.stat_ref));
    }
}

/// `isSingleAttrArmour`
fn is_single_attr_armour(item: &Item) -> bool {
    [item.armour_ar, item.armour_ev, item.armour_es, item.armour_ward].iter().filter(|v| v.is_some()).count() == 1
}

/// `isPdpsImportant`
fn is_pdps_important(item: &Item) -> bool {
    use ItemCategory as C;
    matches!(
        item.category,
        Some(C::OneHandedAxe | C::TwoHandedAxe | C::OneHandedSword | C::TwoHandedSword | C::Bow | C::Warstaff)
    )
}

/// `propToFilter`
pub fn prop_to_filter(prop: Prop, item: &Item, search_in_range: f64) -> StatFilter {
    let mut ids = IndexMap::new();
    ids.insert("pseudo".to_owned(), vec![prop.trade_id.to_owned()]);
    let stat = Stat {
        stat_ref: prop.stat_ref.to_owned(),
        dp: None,
        matchers: vec![StatMatcher { string: prop.stat_ref.to_owned(), advanced: None, negate: None, value: None }],
        better: stat_better::POSITIVE_ROLL,
        mod_family: None,
        reminder_text: None,
        from_area_mods: None,
        anointments: None,
        mercenary: None,
        jewellery_quality: None,
        trade: StatTrade { inverted: None, option: None, ids },
    };
    let sources = prop.sources;
    let mut filter = no_source_pseudo_to_filter(
        NoSourcePseudo { pseudo: stat, roll: prop.roll, dp: prop.dp, disabled: prop.disabled, hidden: prop.hidden },
        item,
        search_in_range,
    )
    .expect("a stat without a trade option converts");
    filter.tag = FilterTag::Property;
    filter.sources = sources;
    filter
}

/// `noSourcePseudoToFilter`'s options.
#[derive(Debug, Clone)]
pub struct NoSourcePseudo {
    pub pseudo: Stat,
    pub roll: Option<StatRoll>,
    pub dp: bool,
    pub disabled: Option<bool>,
    pub hidden: Option<&'static str>,
}

/// `noSourcePseudoToFilter`
pub fn no_source_pseudo_to_filter(opts: NoSourcePseudo, item: &Item, search_in_range: f64) -> R<StatFilter> {
    let roll = opts.roll.map(|r| StatRollInfo {
        unscalable: false,
        generation: None,
        dp: opts.dp,
        value: r.value,
        min: r.min,
        max: r.max,
    });
    let source = StatSource {
        modifier: ParsedModifier { info: ModifierInfo::new(ModifierType::Pseudo), stats: Vec::new() },
        stat: ParsedStat { stat: opts.pseudo.clone(), translation: opts.pseudo.matchers[0].clone(), roll },
        contributes: opts.roll,
    };
    let calc = StatCalculated { stat: opts.pseudo, kind: ModifierType::Pseudo, sources: vec![source] };
    let mut filter = calculated_stat_to_filter(&calc, search_in_range, item)?;

    filter.sources = Vec::new();
    if let Some(disabled) = opts.disabled {
        filter.disabled = disabled;
    }
    if let Some(hidden) = opts.hidden {
        filter.hidden = Some(hidden);
    }
    Ok(filter)
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    let mut refs = vec![CHANCE_TO_BLOCK, ATTACK_SPEED, CRIT_CHANCE];
    refs.extend(ELEMENTAL_FLAT);
    refs
}
