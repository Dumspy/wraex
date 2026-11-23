//! `filters/pseudo/item-property.ts`: armour, weapon and waystone properties (at
//! 20% quality) and base percentile as filters.

use indexmap::IndexMap;

use super::super::create_stat_filters::{Ctx, calculated_stat_to_filter, short_roll_to_filter};
use super::super::{
    AdditionalInfo, FilterOption, FilterTag, INTERNAL_TRADE_IDS, ItemIsElementalModifier, R, StatFilter, missing,
};
use crate::data::{Stat, StatMatcher, StatTrade, stat_better};
use crate::parser::advanced_mod_desc::{ModifierInfo, ParsedModifier};
use crate::parser::calc_q20::{PropStats, calc_prop_bounds, prop_at_20_quality, quality_stats};
use crate::parser::item::Item;
use crate::parser::meta::{ARMOUR, ItemCategory, is_weapon};
use crate::parser::modifiers::{ModifierType, StatCalculated, StatRoll, StatSource};
use crate::parser::stat_translations::{ParsedStat, StatRollInfo};

/// `propToFilter`'s options.
#[derive(Debug, Clone)]
pub struct Prop {
    pub stat_ref: &'static str,
    pub trade_id: &'static str,
    pub roll: StatRoll,
    pub sources: Vec<StatSource>,
    pub dp: bool,
    pub disabled: Option<bool>,
    pub hidden: Option<&'static str>,
    pub option: Option<FilterOption>,
}

impl Prop {
    pub fn new(stat_ref: &'static str, trade_id: &'static str, roll: StatRoll, sources: Vec<StatSource>) -> Self {
        Self { stat_ref, trade_id, roll, sources, dp: false, disabled: None, hidden: None, option: None }
    }
}

fn truthy(x: Option<f64>) -> Option<f64> {
    x.filter(|v| *v != 0.0 && !v.is_nan())
}

/// `filterItemProp`
pub fn filter_item_prop(ctx: &mut Ctx) -> R<()> {
    let category = ctx.item.category.as_ref();
    if category.is_some_and(|c| ARMOUR.contains(c)) {
        armour_props(ctx)?;
    }
    if category.is_some_and(is_weapon) {
        weapon_props(ctx)?;
    }
    if category == Some(&ItemCategory::Map) {
        map_props(ctx)?;
    }
    Ok(())
}

/// `filterBasePercentile`
pub fn filter_base_percentile(ctx: &mut Ctx) -> R<()> {
    if let Some(pct) = ctx.item.base_percentile {
        let roll = StatRoll { value: pct, min: 0.0, max: 100.0, option: None };
        let prop = Prop {
            disabled: Some(pct < 50.0),
            ..Prop::new("Base Percentile: #%", "item.base_percentile", roll, Vec::new())
        };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }
    Ok(())
}

const BLOCK: &str = "#% increased Block chance";

/// `ARMOUR_STATS`
pub fn is_armour_stat(stat_ref: &str) -> bool {
    use quality_stats::{ARMOUR as AR, ENERGY_SHIELD as ES, EVASION as EV, WARD};
    [AR, EV, ES, WARD].iter().any(|p| p.flat.contains(&stat_ref) || p.incr.contains(&stat_ref)) || stat_ref == BLOCK
}

fn armour_props(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;

    let props = [
        (item.armour_ar, quality_stats::ARMOUR, "Armour: #", "item.armour"),
        (item.armour_ev, quality_stats::EVASION, "Evasion Rating: #", "item.evasion_rating"),
        (item.armour_es, quality_stats::ENERGY_SHIELD, "Energy Shield: #", "item.energy_shield"),
    ];
    for (value, stats, stat_ref, trade_id) in props {
        let Some(value) = truthy(value) else { continue };
        let total_q20 = prop_at_20_quality(value, stats, item);
        // `!isSingleAttrArmour(item)`, which is always true in PoE2.
        let prop = Prop { disabled: Some(false), ..Prop::new(stat_ref, trade_id, total_q20.roll, total_q20.sources) };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if let Some(block) = truthy(item.armour_block) {
        let block = calc_prop_bounds(block, PropStats { flat: &[], incr: &[BLOCK] }, item, false);
        let prop = Prop { disabled: Some(true), ..Prop::new("Block: #%", "item.block", block.roll, block.sources) };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if let Some(ward) = truthy(item.armour_rw) {
        let ward = calc_prop_bounds(ward, quality_stats::WARD, item, false);
        let prop =
            Prop { disabled: Some(true), ..Prop::new("Runic Ward: #", "item.runic_ward", ward.roll, ward.sources) };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if [item.armour_ar, item.armour_ev, item.armour_es, item.armour_rw, item.armour_block]
        .into_iter()
        .any(|v| truthy(v).is_some())
    {
        ctx.stats_by_type.retain(|m| !is_armour_stat(&m.stat.stat_ref));
    }
    Ok(())
}

const ATTACK_SPEED: &str = "#% increased Attack Speed";
const CRIT_FLAT: &str = "#% to Critical Hit Chance";
const CRIT_INCR: &str = "#% increased Critical Hit Chance";
const SPIRIT_INCR: &str = "#% increased Spirit";
const FIRE: &str = "Adds # to # Fire Damage";
const COLD: &str = "Adds # to # Cold Damage";
const LIGHTNING: &str = "Adds # to # Lightning Damage";

/// `WEAPON_STATS`. Not `Adds # to # Chaos Damage`.
pub fn is_weapon_stat(stat_ref: &str) -> bool {
    let phys = quality_stats::PHYSICAL_DAMAGE;
    phys.flat.contains(&stat_ref)
        || phys.incr.contains(&stat_ref)
        || [ATTACK_SPEED, CRIT_FLAT, LIGHTNING, COLD, FIRE, SPIRIT_INCR].contains(&stat_ref)
}

fn times(a: StatRoll, b: StatRoll) -> StatRoll {
    StatRoll { value: a.value * b.value, min: a.min * b.min, max: a.max * b.max, option: None }
}

fn weapon_props(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;

    let attack_speed =
        calc_prop_bounds(item.weapon_as.unwrap_or(0.0), PropStats { incr: &[ATTACK_SPEED], flat: &[] }, item, false);
    let phys_q20 = prop_at_20_quality(item.weapon_physical.unwrap_or(0.0), quality_stats::PHYSICAL_DAMAGE, item);
    let aps = attack_speed.roll;
    let pdps_q20 = times(phys_q20.roll, aps);

    let flat = |total: Option<f64>, flat: &'static [&'static str]| {
        calc_prop_bounds(total.unwrap_or(0.0), PropStats { flat, incr: &[] }, item, false)
    };
    let ele_dmg = flat(item.weapon_elemental, &[LIGHTNING, COLD, FIRE]);
    let fire_dmg = flat(item.weapon_fire, &[FIRE]);
    let cold_dmg = flat(item.weapon_cold, &[COLD]);
    let lightning_dmg = flat(item.weapon_lightning, &[LIGHTNING]);

    let edps = times(ele_dmg.roll, aps);
    let dps = StatRoll {
        value: pdps_q20.value + edps.value,
        min: pdps_q20.min + edps.min,
        max: pdps_q20.max + edps.max,
        option: None,
    };
    let fire_dps = times(fire_dmg.roll, aps);
    let cold_dps = times(cold_dmg.roll, aps);
    let lightning_dps = times(lightning_dmg.roll, aps);

    if truthy(item.weapon_elemental).is_some() {
        let mut elemental_info = IndexMap::new();
        for (i, roll) in [(13, fire_dps), (14, cold_dps), (15, lightning_dps)] {
            if roll.value != 0.0 {
                elemental_info.insert(INTERNAL_TRADE_IDS[i], short_roll_to_filter(roll, ctx.search_in_range, item));
            }
        }

        let mut total_sources = ele_dmg.sources.clone();
        total_sources.extend(phys_q20.sources.iter().cloned());
        let prop = Prop { disabled: Some(false), ..Prop::new("Total DPS: #", "item.total_dps", dps, total_sources) };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);

        let minor = edps.value / dps.value < 0.15;
        let prop = Prop {
            disabled: Some(minor),
            hidden: minor.then_some("filters.hide_ele_dps"),
            option: Some(FilterOption { value: Some(f64::from(ItemIsElementalModifier::Any as u8)) }),
            ..Prop::new("Elemental DPS: #", "item.elemental_dps", edps, ele_dmg.sources)
        };
        let mut elemental_filter = prop_to_filter(prop, ctx)?;

        if !elemental_info.is_empty() {
            let roll = elemental_filter.roll.clone().ok_or_else(|| missing("elemental roll"))?;
            elemental_info.insert(INTERNAL_TRADE_IDS[12], roll);
            elemental_filter.additional_info =
                Some(AdditionalInfo { elemental_info: Some(elemental_info), empty_modifier_info: None });
        }

        ctx.filters.push(elemental_filter);
    }

    if truthy(item.weapon_physical).is_some() {
        let minor = pdps_q20.value / dps.value < 0.67;
        let prop = Prop {
            disabled: Some(!is_pdps_important(item) || minor),
            hidden: minor.then_some("filters.hide_phys_dps"),
            ..Prop::new("Physical DPS: #", "item.physical_dps", pdps_q20, phys_q20.sources)
        };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if truthy(item.weapon_as).is_some() {
        let prop = Prop {
            dp: true,
            disabled: Some(true),
            ..Prop::new("Attacks per Second: #", "item.aps", aps, attack_speed.sources)
        };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if let Some(crit) = truthy(item.weapon_crit) {
        let crit = calc_prop_bounds(crit, PropStats { incr: &[CRIT_INCR], flat: &[CRIT_FLAT] }, item, false);
        let prop = Prop {
            dp: true,
            disabled: Some(true),
            ..Prop::new("Critical Hit Chance: #%", "item.crit", crit.roll, crit.sources)
        };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if let Some(reload) = truthy(item.weapon_reload) {
        // Inverted: a lower reload time is better.
        let reload = calc_prop_bounds(reload, PropStats { incr: &[ATTACK_SPEED], flat: &[] }, item, true);
        let prop = Prop {
            dp: true,
            disabled: Some(true),
            ..Prop::new("Reload Time: #", "item.reload_time", reload.roll, reload.sources)
        };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if let Some(spirit) = truthy(item.weapon_spirit) {
        let spirit = calc_prop_bounds(spirit, PropStats { flat: &[], incr: &[SPIRIT_INCR] }, item, false);
        let prop =
            Prop { disabled: Some(false), ..Prop::new("Spirit: #%", "item.spirit", spirit.roll, spirit.sources) };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }

    if [item.weapon_as, item.weapon_crit, item.weapon_elemental, item.weapon_physical, item.weapon_spirit]
        .into_iter()
        .any(|v| truthy(v).is_some())
    {
        ctx.stats_by_type.retain(|m| !is_weapon_stat(&m.stat.stat_ref));
    }
    Ok(())
}

fn map_props(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;
    let none = PropStats { flat: &[], incr: &[] };
    let props: [(Option<f64>, &'static str, &'static str, Option<&'static str>); 9] = [
        (item.map_revives, "Revives Available: #", "item.map_revives", Some("filters.hide_revives")),
        (item.map_pack_size, "Monster Pack Size: #", "item.map_pack_size", None),
        // Not in 0.5.2.
        (item.map_magic_monsters, "Magic Monsters: #", "item.map_magic_monsters", None),
        // Not in 0.5.2.
        (item.map_rare_monsters, "Rare Monsters: #", "item.map_rare_monsters", None),
        (item.map_drop_chance, "Waystone Drop Chance: #%", "item.map_drop_chance", None),
        (item.map_item_rarity, "Map Item Rarity: #%", "item.map_item_rarity", None),
        // Not in 0.5.2.
        (item.map_gold, "Gold Found: #%", "item.map_gold", None),
        // Yes, rare monsters' trade id.
        (item.map_monster_rarity, "Monster Rarity: #%", "item.map_rare_monsters", None),
        // Yes, magic monsters' trade id.
        (item.map_effectiveness, "Monster Effectiveness: #%", "item.map_magic_monsters", None),
    ];
    for (value, stat_ref, trade_id, hidden) in props {
        let Some(value) = truthy(value) else { continue };
        let bounds = calc_prop_bounds(value, none, item, false);
        let prop = Prop { disabled: Some(true), hidden, ..Prop::new(stat_ref, trade_id, bounds.roll, bounds.sources) };
        let filter = prop_to_filter(prop, ctx)?;
        ctx.filters.push(filter);
    }
    Ok(())
}

/// `isPdpsImportant`
fn is_pdps_important(item: &Item) -> bool {
    use ItemCategory as C;
    matches!(
        item.category,
        Some(
            C::OneHandedAxe
                | C::TwoHandedAxe
                | C::OneHandedSword
                | C::TwoHandedSword
                | C::OneHandedMace
                | C::TwoHandedMace
                | C::Bow
                | C::Warstaff
                | C::Crossbow
                | C::Spear
                | C::Flail
        )
    )
}

/// The stat `propToFilter` builds: `{ ref, matchers: [{ string: ref }], trade:
/// { ids: { pseudo: [tradeId] } }, better: 1 }`.
pub fn prop_stat(stat_ref: &str, trade_id: &str) -> Stat {
    let mut ids = IndexMap::new();
    ids.insert("pseudo".to_owned(), vec![trade_id.to_owned()]);
    Stat {
        stat_ref: stat_ref.to_owned(),
        id: None,
        dp: None,
        matchers: vec![StatMatcher {
            string: stat_ref.to_owned(),
            advanced: None,
            negate: None,
            value: None,
            oils: None,
        }],
        better: stat_better::POSITIVE_ROLL,
        from_area_mods: None,
        from_uber_area_mods: None,
        from_heist_area_mods: None,
        anointments: None,
        trade: StatTrade { inverted: None, option: None, count: None, ids: Some(ids) },
    }
}

/// `propToFilter`
pub fn prop_to_filter(prop: Prop, ctx: &Ctx) -> R<StatFilter> {
    let stat = prop_stat(prop.stat_ref, prop.trade_id);
    let r = prop.roll;
    let source = StatSource {
        modifier: ParsedModifier { info: ModifierInfo::new(ModifierType::Pseudo), stats: Vec::new() },
        stat: ParsedStat {
            translation: stat.matchers[0].clone(),
            stat: stat.clone(),
            roll: Some(StatRollInfo {
                unscalable: false,
                legacy: None,
                dp: prop.dp,
                value: r.value,
                min: r.min,
                max: r.max,
                option: r.option,
            }),
            from_added_augment: None,
        },
        contributes: Some(r),
    };
    let calc = StatCalculated { stat, kind: ModifierType::Pseudo, sources: vec![source] };
    let mut filter = calculated_stat_to_filter(ctx.d, &calc, ctx.search_in_range, ctx.item, true)?;

    filter.tag = FilterTag::Property;
    filter.sources = prop.sources;
    if let Some(disabled) = prop.disabled {
        filter.disabled = disabled;
    }
    if let Some(hidden) = prop.hidden {
        filter.hidden = Some(hidden);
    }
    if let Some(option) = prop.option {
        filter.option = Some(option);
    }
    Ok(filter)
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    vec![BLOCK, ATTACK_SPEED, CRIT_FLAT, LIGHTNING, COLD, FIRE, SPIRIT_INCR]
}
