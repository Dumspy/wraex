//! `filters/create-stat-filters.ts`: the stat filters of a preset, from the
//! item's summed stats.

use wraex_core::Rarity;
use wraex_core::filters::{Rounding, percent_roll, percent_roll_delta, round_roll};

use super::pseudo::anointments::{apply_anointment_rules, decode_oils};
use super::pseudo::item_property::{
    BASE_PCTL_AFFECTED_IDS, filter_base_percentile, filter_equipment_props, filter_memory_strands,
};
use super::pseudo::maps::{chart_props, map_props, valdo_bad_mods};
use super::pseudo::{
    atzoatl_rules, filter_pseudo, flasks::apply_flask_hybrid_mod, heist::apply_heist_rules, reflection_rules,
    timeless_jewel::filter_timeless_jewel_keystones,
};
use super::{
    FilterError, FilterGroup, FilterOrGroup, FilterRoll, FilterSource, FilterTag, ItemHasEmptyModifier, MinMax, R,
    StatFilter, ValueFilter, missing,
};
use crate::data::{Data, stat_better};
use crate::parser::advanced_mod_desc::Generation;
use crate::parser::calc_q20::{QUALITY_CHANGING_ENCHANT, get_prop_quality};
use crate::parser::item::Item;
use crate::parser::meta::{ItemCategory, JEWELLERY};
use crate::parser::modifiers::{
    ModifierMechanic, ModifierType, StatCalculated, TotalMode, stat_sources_total, translate_stat_with_roll,
};
use crate::parser::stat_translations::RollGeneration;

use ItemCategory as C;

/// `FiltersCreationContext`
#[derive(Debug)]
pub struct Ctx<'a> {
    pub d: &'a Data,
    pub item: &'a Item,
    pub search_in_range: f64,
    pub filters: Vec<StatFilter>,
    pub groups: Vec<FilterGroup>,
    pub stats_by_type: Vec<StatCalculated>,
}

/// `createExactStatFilters`'s mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Props,
    Bulk,
}

fn is(item: &Item, category: ItemCategory) -> bool {
    item.category.as_ref() == Some(&category)
}

fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// `filter.tradeId[0]`: throws when the stat has no trade id for its type.
pub(crate) fn trade_id0(filter: &StatFilter) -> R<Option<&str>> {
    let ids = filter.trade_id.as_ref().ok_or_else(|| FilterError(format!("no trade id: {}", filter.stat_ref)))?;
    Ok(ids.first().map(String::as_str))
}

/// `filter.roll!`
fn roll_mut(filter: &mut StatFilter) -> R<&mut FilterRoll> {
    let stat_ref = filter.stat_ref.clone();
    filter.roll.as_mut().ok_or_else(|| FilterError(format!("no roll: {stat_ref}")))
}

/// `createExactStatFilters`
pub fn create_exact_stat_filters(
    d: &Data,
    item: &Item,
    stats_by_type: &[StatCalculated],
    search_stat_range: f64,
    mode: Option<Mode>,
) -> R<Vec<StatFilter>> {
    if item.info().area.as_ref().is_some_and(|a| a.blighted == Some(true)) || is(item, C::Invitation) {
        return Ok(Vec::new());
    }

    let mut keep_by_type = vec![
        ModifierType::Pseudo,
        ModifierType::Fractured,
        ModifierType::Enchant,
        ModifierType::Necropolis,
        ModifierType::Imbued,
    ];

    if item.influences.is_empty()
        && item.is_fractured != Some(true)
        && !is(item, C::Tincture)
        && !is(item, C::Idol)
        && !is(item, C::Chart)
    {
        keep_by_type.push(ModifierType::Implicit);
    }

    let excluded = [C::ClusterJewel, C::Map, C::HeistContract, C::HeistBlueprint, C::Chart, C::Sentinel];
    let magic_explicits =
        item.rarity == Some(Rarity::Magic) && !item.category.as_ref().is_some_and(|c| excluded.contains(c));
    if magic_explicits || (item.rarity == Some(Rarity::Rare) && is(item, C::Idol)) {
        keep_by_type.push(ModifierType::Explicit);
    }

    if is(item, C::Flask) {
        keep_by_type.push(ModifierType::Crafted);
    }

    let mut ctx = Ctx {
        d,
        item,
        search_in_range: if mode != Some(Mode::Props) { search_stat_range.min(2.0) } else { search_stat_range },
        filters: Vec::new(),
        groups: Vec::new(),
        stats_by_type: stats_by_type
            .iter()
            .filter(|calc| {
                keep_by_type.contains(&calc.kind)
                    || calc.sources.iter().any(|s| s.modifier.info.mechanic == Some(ModifierMechanic::RareOnly))
            })
            .cloned()
            .collect(),
    };

    filter_base_percentile(&mut ctx);
    filter_memory_strands(&mut ctx, None);
    map_props(mode == Some(Mode::Bulk), &mut ctx)?;
    chart_props(mode == Some(Mode::Bulk), &mut ctx);
    valdo_bad_mods(&mut ctx)?;

    let stats = std::mem::take(&mut ctx.stats_by_type);
    for calc in &stats {
        let filter = calculated_stat_to_filter(calc, ctx.search_in_range, item)?;
        ctx.filters.push(filter);
    }
    ctx.stats_by_type = stats;

    if item.ref_name_is("Chronicle of Atzoatl") {
        atzoatl_rules::apply_rules(&mut ctx.filters)?;
        return Ok(ctx.filters);
    }
    if item.ref_name_is("Mirrored Tablet") {
        reflection_rules::apply_rules(&mut ctx.filters)?;
        return Ok(ctx.filters);
    }

    for filter in &mut ctx.filters {
        if filter.not == Some(true) {
            continue;
        }

        filter.hidden = None;

        if filter.tag == FilterTag::EXPLICIT {
            let top_tier = filter.stat_sources().any(|s| s.modifier.info.tier.is_some_and(|t| t <= 2.0));
            filter.disabled = !top_tier;
        } else if filter.tag != FilterTag::Property && filter.tag != FilterTag::PSEUDO {
            filter.disabled = false;
        }

        if filter.stat_ref == "# uses remaining" {
            let roll = roll_mut(filter)?;
            roll.min = Some(roll.value);
            roll.default.min = roll.value;
            roll.default.max = roll.value;
        }
    }

    if is(item, C::ClusterJewel) {
        apply_cluster_jewel_rules(&mut ctx.filters, true)?;
    }
    // Not `else if` upstream either.
    if is(item, C::HeistContract) || is(item, C::HeistBlueprint) {
        apply_heist_rules(&mut ctx)?;
    } else if is(item, C::Flask) {
        apply_flask_rules(&mut ctx.filters);
        apply_flask_hybrid_mod(&mut ctx)?;
    } else if is(item, C::MemoryLine) || is(item, C::SanctumRelic) || is(item, C::Charm) {
        enable_all_filters(&mut ctx.filters);
    } else if is(item, C::Idol) {
        enable_good_rolled_filters(&mut ctx.filters, 0.66);
    }

    Ok(ctx.filters)
}

/// `initUiModFilters`
pub fn init_ui_mod_filters(d: &Data, item: &Item, search_stat_range: f64) -> R<Vec<FilterOrGroup>> {
    let mut ctx = Ctx {
        d,
        item,
        search_in_range: if item.rarity == Some(Rarity::Normal) { 100.0 } else { search_stat_range },
        filters: Vec::new(),
        groups: Vec::new(),
        stats_by_type: item
            .stats_by_type
            .iter()
            .map(|calc| {
                if calc.kind == ModifierType::Fractured && calc.stat.trade.ids.contains_key("explicit") {
                    StatCalculated { kind: ModifierType::Explicit, ..calc.clone() }
                } else {
                    calc.clone()
                }
            })
            .collect(),
    };

    filter_equipment_props(&mut ctx);
    if item.rarity == Some(Rarity::Unique) {
        filter_base_percentile(&mut ctx);
    }
    filter_memory_strands(&mut ctx, Some("hide_memory_strands"));
    if !item.ref_name_is("Split Personality") {
        filter_pseudo(&mut ctx)?;
    }
    if item.unique_base.as_ref().is_some_and(|b| b.ref_name == "Timeless Jewel") {
        filter_timeless_jewel_keystones(&mut ctx)?;
    }

    if item.is_corrupted.is_none() && item.is_mirrored != Some(true) {
        ctx.stats_by_type.retain(|m| m.kind != ModifierType::Fractured);
        ctx.stats_by_type.extend(item.stats_by_type.iter().filter(|m| m.kind == ModifierType::Fractured).cloned());
    }

    if item.is_veiled == Some(true) {
        ctx.stats_by_type.retain(|m| m.kind != ModifierType::Veiled);
    }

    let stats = std::mem::take(&mut ctx.stats_by_type);
    for calc in &stats {
        let filter = calculated_stat_to_filter(calc, ctx.search_in_range, item)?;
        ctx.filters.push(filter);
    }
    ctx.stats_by_type = stats;

    if item.is_veiled == Some(true) {
        for filter in &mut ctx.filters {
            filter.disabled = true;
        }
    }

    final_filter_tweaks(&mut ctx)?;

    let mut out: Vec<FilterOrGroup> = ctx.filters.into_iter().map(FilterOrGroup::Stat).collect();
    out.extend(ctx.groups.into_iter().map(FilterOrGroup::Group));
    Ok(out)
}

/// `calculatedStatToFilter`
pub fn calculated_stat_to_filter(calc: &StatCalculated, mut percent: f64, item: &Item) -> R<StatFilter> {
    let StatCalculated { stat, sources, kind } = calc;
    let kind = *kind;
    let trade_id = stat.trade.ids.get(kind.as_str()).cloned();
    let first = sources.first();

    let mut option_filter = None;
    if stat.trade.option == Some(true) {
        let first = first.ok_or_else(|| missing("sources[0]"))?;
        let value = first.contributes.ok_or_else(|| missing("contributes"))?.value;
        option_filter = Some(StatFilter {
            trade_id: trade_id.clone(),
            stat_ref: stat.stat_ref.clone(),
            text: first.stat.translation.string.clone(),
            tag: if kind == ModifierType::Enchant { FilterTag::ENCHANT } else { FilterTag::Variant },
            oils: None,
            mercenary: None,
            reminder_text: first.stat.stat.reminder_text.clone(),
            sources: sources.iter().cloned().map(FilterSource::Stat).collect(),
            not: None,
            roll: None,
            option: Some(ValueFilter { value }),
            hidden: None,
            disabled: false,
        });
    }

    let total_mode = if item.ref_name_is("Mirrored Tablet") { TotalMode::Max } else { TotalMode::Sum };
    let roll = stat_sources_total(sources, total_mode);
    let translation = translate_stat_with_roll(calc, roll.as_ref());

    let mut filter = match option_filter {
        Some(filter) => filter,
        None => StatFilter {
            trade_id,
            stat_ref: stat.stat_ref.clone(),
            text: translation.string.clone(),
            tag: FilterTag::Type(kind),
            oils: decode_oils(calc),
            mercenary: None,
            reminder_text: sources.iter().find_map(|s| s.stat.stat.reminder_text.clone()),
            sources: sources.iter().cloned().map(FilterSource::Stat).collect(),
            not: None,
            roll: None,
            option: None,
            hidden: None,
            disabled: true,
        },
    };

    if stat.better == stat_better::NOT_COMPARABLE {
        if kind != ModifierType::Enchant {
            filter.tag = FilterTag::Variant;
        }
        if filter.oils.is_none() {
            filter.disabled = false;
        }
    }

    if kind == ModifierType::Implicit || kind == ModifierType::Explicit {
        if kind == ModifierType::Explicit
            && let Some(unique) = &item.info().unique
        {
            if let Some(fixed_stats) = &unique.fixed_stats {
                if !fixed_stats.contains(&filter.stat_ref) {
                    filter.tag = FilterTag::Variant;
                }
            } else if sources
                .iter()
                .any(|s| matches!(s.modifier.info.generation, Some(Generation::Prefix | Generation::Suffix)))
            {
                // Recognized unveiled mods are overridden below; this is a fallback.
                filter.tag = FilterTag::Variant;
            }
        }

        if let Some(mechanic) = sources.iter().find_map(|s| s.modifier.info.mechanic) {
            filter.tag = FilterTag::Mechanic(mechanic);
        }
    }

    if let (Some(roll), None) = (roll, &filter.option) {
        let rarity = item.rarity;
        if (rarity == Some(Rarity::Magic)
            && (item.is_unmodifiable == Some(true) || item.is_corrupted.is_some() || item.is_mirrored == Some(true)))
            || item.ref_name_is("Viscera")
        {
            percent = 0.0;
        } else if rarity == Some(Rarity::Unique)
            || (rarity == Some(Rarity::Magic) && is(item, C::Jewel))
            || sources
                .iter()
                .any(|s| s.modifier.info.tier == Some(1.0) && s.modifier.info.kind == ModifierType::Fractured)
        {
            let perfect_roll = (stat.better == stat_better::POSITIVE_ROLL && roll.value >= roll.max)
                || (stat.better == stat_better::NEGATIVE_ROLL && roll.value <= roll.min);
            if perfect_roll {
                percent = 0.0;
            }
        }

        let mut goodness = None;
        if stat.better != stat_better::NOT_COMPARABLE {
            let fixed = roll.min == roll.max;
            if fixed {
                goodness = Some(1.0);
            } else {
                let mut g = (roll.value - roll.min) / (roll.max - roll.min);
                if stat.better == stat_better::NEGATIVE_ROLL {
                    g = 1.0 - g;
                }
                goodness = Some(g);
            }
        }

        let dp = stat.dp == Some(true)
            || sources
                .iter()
                .any(|s| s.stat.stat.stat_ref == stat.stat_ref && s.stat.roll.as_ref().is_some_and(|r| r.dp));

        let bounds = MinMax {
            min: percent_roll(roll.min, -0.0, Rounding::Floor, dp),
            max: percent_roll(roll.max, 0.0, Rounding::Ceil, dp),
        };

        let mut default = if stat.better == stat_better::NOT_COMPARABLE {
            MinMax { min: roll.value, max: roll.value }
        } else if rarity == Some(Rarity::Unique) {
            let delta = roll.max - roll.min;
            MinMax {
                min: percent_roll_delta(roll.value, delta, -percent, Rounding::Floor, dp),
                max: percent_roll_delta(roll.value, delta, percent, Rounding::Ceil, dp),
            }
        } else {
            MinMax {
                min: percent_roll(roll.value, -percent, Rounding::Floor, dp),
                max: percent_roll(roll.value, percent, Rounding::Ceil, dp),
            }
        };
        default.min = js_max(default.min, bounds.min);
        default.max = js_min(default.max, bounds.max);
        let ranged = roll.min != roll.max;
        let mut filter_roll = FilterRoll {
            value: round_roll(roll.value, dp),
            min: None,
            max: None,
            default,
            bounds: (ranged && stat.better != stat_better::NOT_COMPARABLE).then_some(bounds),
            trade_invert: stat.trade.inverted,
            dp,
            is_negated: false,
            goodness,
        };

        filter_fill_min_max(&mut filter_roll, stat.better);

        if translation.negate {
            filter_adjustment_for_negate(&mut filter_roll);
        }
        filter.roll = Some(filter_roll);
    }

    Ok(filter)
}

/// `hideNotVariableStat`
fn hide_not_variable_stat(filter: &mut StatFilter, item: &Item) -> R<()> {
    if item.rarity != Some(Rarity::Unique) || (filter.tag != FilterTag::EXPLICIT && filter.tag != FilterTag::Property) {
        return Ok(());
    }

    // Always show a stat increased by a Catalyst.
    let has_quality = item.quality.is_some_and(|q| q != 0.0 && !q.is_nan());
    let in_jewellery = item.category.as_ref().is_some_and(|c| JEWELLERY.contains(c));
    if has_quality
        && in_jewellery
        && filter.stat_sources().any(|s| {
            s.modifier.info.roll_incr.is_some_and(|i| i != 0.0 && !i.is_nan())
                && s.stat.roll.as_ref().is_some_and(|r| !r.unscalable)
        })
    {
        return Ok(());
    }

    // Always show a scalable stat on a corrupted item.
    let has_roll_and_scalable = filter.stat_sources().any(|s| s.stat.roll.as_ref().is_some_and(|r| !r.unscalable));
    if item.is_corrupted.is_some() && has_roll_and_scalable {
        let is_vvo_corrupted = item.new_mods.iter().any(|m| {
            m.stats.iter().any(|s| s.roll.as_ref().is_some_and(|r| r.generation == Some(RollGeneration::Volatile)))
        });
        if !is_vvo_corrupted {
            // A stat not reduced, compared to other VVO-corrupted items, can matter.
            filter.disabled = false;
        }
        return Ok(());
    }

    match &mut filter.roll {
        None => {
            filter.hidden = Some("filters.hide_const_roll");
            filter.disabled = true;
        }
        Some(roll) if roll.bounds.is_none() => {
            roll.min = None;
            roll.max = None;
            filter.hidden = Some("filters.hide_const_roll");
            filter.disabled = true;
        }
        Some(_) => {
            let affected = trade_id0(filter)?.is_some_and(|id| BASE_PCTL_AFFECTED_IDS.contains(&id));
            let constant = filter.stat_sources().all(|s| {
                let roll = s.stat.roll.as_ref();
                roll.map(|r| r.min) == roll.map(|r| r.max)
            });
            if affected && constant && get_prop_quality(item) < 21.0 {
                let roll = roll_mut(filter)?;
                roll.min = None;
                roll.max = None;
                filter.hidden = Some("hide_variable_by_base_percentile_only");
                filter.disabled = true;
            }
        }
    }

    if item.is_foulborn == Some(true) && filter.tag == FilterTag::EXPLICIT {
        // A mod not replaced with a foulborn one can matter.
        filter.disabled = false;
    }
    Ok(())
}

/// `filterFillMinMax`
fn filter_fill_min_max(roll: &mut FilterRoll, better: i8) {
    match better {
        stat_better::POSITIVE_ROLL => roll.min = Some(roll.default.min),
        stat_better::NEGATIVE_ROLL => roll.max = Some(roll.default.max),
        stat_better::NOT_COMPARABLE => {
            roll.min = Some(roll.default.min);
            roll.max = Some(roll.default.max);
        }
        _ => {}
    }
}

/// `filterAdjustmentForNegate`
fn filter_adjustment_for_negate(roll: &mut FilterRoll) {
    roll.trade_invert = Some(roll.trade_invert != Some(true));
    roll.is_negated = true;
    let swap = roll.clone();

    if let (Some(swap_bounds), Some(bounds)) = (swap.bounds, &mut roll.bounds) {
        bounds.min = -swap_bounds.max;
        bounds.max = -swap_bounds.min;
    }

    roll.default.min = -swap.default.max;
    roll.default.max = -swap.default.min;

    roll.value = -swap.value;
    roll.min = swap.max.map(|m| -m);
    roll.max = swap.min.map(|m| -m);
}

/// `finalFilterTweaks`
fn final_filter_tweaks(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;

    if is(item, C::ClusterJewel) && item.rarity != Some(Rarity::Unique) {
        apply_cluster_jewel_rules(&mut ctx.filters, false)?;
    } else if is(item, C::Flask) {
        apply_flask_rules(&mut ctx.filters);
        apply_flask_hybrid_mod(ctx)?;
    }

    if let Some(has_empty_modifier) = show_has_empty_modifier(item) {
        ctx.filters.push(StatFilter {
            trade_id: Some(vec!["item.has_empty_modifier".to_owned()]),
            stat_ref: "1 Empty or Crafted Modifier".to_owned(),
            text: "1 Empty or Crafted Modifier".to_owned(),
            tag: FilterTag::PSEUDO,
            oils: None,
            mercenary: None,
            reminder_text: None,
            sources: Vec::new(),
            not: None,
            roll: None,
            option: Some(ValueFilter { value: f64::from(has_empty_modifier as u8) }),
            hidden: Some("filters.hide_empty_mod"),
            disabled: true,
        });
    }

    if is(item, C::Amulet) || is(item, C::Ring) {
        apply_anointment_rules(&mut ctx.filters, item);
    }

    for filter in &mut ctx.filters {
        hide_not_variable_stat(filter, item)?;

        if filter.tag == FilterTag::ENCHANT {
            if QUALITY_CHANGING_ENCHANT.contains(&filter.stat_ref.as_str()) {
                filter.hidden = Some("hide_enchant_meta_stat");
            }
        } else if filter.tag == FilterTag::FRACTURED {
            let calc = item
                .stats_by_type
                .iter()
                .find(|m| m.stat.stat_ref == filter.stat_ref)
                .ok_or_else(|| missing("fractured stat"))?;
            if calc.stat.trade.ids.contains_key("explicit") {
                // Hide only if the fractured mod has a matching explicit variant.
                filter.hidden = Some("filters.hide_for_crafting");
            }
        } else if filter.stat_sources().next().is_some_and(|s| s.stat.stat.jewellery_quality.is_some()) {
            filter.hidden = Some("hide_jewellery_quality");
        } else if filter.tag == FilterTag::IMPLICIT {
            if item.rarity == Some(Rarity::Unique) && item.is_corrupted.is_none() && !is(item, C::Jewel) {
                // Hide not Corrupted, Vestigial implicits etc. that pseudo stats did not consume.
                filter.hidden = Some("hide_unique_base_implicit");
                filter.disabled = true;
            }
        } else if matches!(
            filter.tag,
            FilterTag::Mechanic(ModifierMechanic::Foulborn | ModifierMechanic::Vestigial) | FilterTag::Variant
        ) {
            filter.disabled = false;
        }
    }

    let mut base_percentile = None;
    for (i, filter) in ctx.filters.iter().enumerate() {
        if trade_id0(filter)? == Some("item.base_percentile") {
            base_percentile = Some(i);
            break;
        }
    }
    if let (Some(i), Some(Rarity::Unique)) = (base_percentile, item.rarity) {
        let mut has_visible_armour_prop = false;
        for filter in &ctx.filters {
            if trade_id0(filter)?.is_some_and(|id| BASE_PCTL_AFFECTED_IDS.contains(&id)) && filter.hidden.is_none() {
                has_visible_armour_prop = true;
                break;
            }
        }
        if has_visible_armour_prop {
            ctx.filters[i].hidden = Some("hide_redundant");
            ctx.filters[i].disabled = true;
        }
    }

    if item.rarity == Some(Rarity::Unique) {
        let count_visible = ctx.filters.iter().filter(|f| f.hidden.is_none()).count();
        if count_visible <= 3 {
            enable_all_filters(&mut ctx.filters);
        }
    }
    Ok(())
}

/// `applyClusterJewelRules`.
/// <https://www.poewiki.net/wiki/Cluster_Jewel#Optimal_passive_skill_amounts>
fn apply_cluster_jewel_rules(filters: &mut [StatFilter], exact: bool) -> R<()> {
    for filter in filters {
        if filter.stat_ref == "# Added Passive Skills are Jewel Sockets" {
            filter.hidden = Some("filters.hide_const_roll");
            filter.disabled = true;
        }

        if filter.stat_ref == "Adds # Passive Skills" {
            filter.disabled = false;
            let roll = roll_mut(filter)?;
            let value = roll.value;
            if value == 4.0 {
                // 4 is [_, 5]
                roll.max = Some(5.0);
            } else if value == 5.0 && exact {
                // 5 is [5, 5] (and [_, 5] for a rare jewel)
                roll.min = Some(roll.default.min);
            } else if [3.0, 6.0, 10.0, 11.0, 12.0].contains(&value) {
                // 3, 6, 10, 11, 12 are [n, _]
                roll.min = Some(roll.default.min);
                roll.max = None;
            }
            // else 2, 5 (rare), 8, 9 are [_, n]
        }
    }
    Ok(())
}

/// `applyFlaskRules`
fn apply_flask_rules(filters: &mut [StatFilter]) {
    let used_enkindling = filters.iter().any(|f| f.stat_ref == "Gains no Charges during Flask Effect");
    for filter in filters {
        if filter.tag == FilterTag::ENCHANT && !used_enkindling {
            filter.hidden = Some("hide_harvest_and_instilling");
            filter.disabled = true;
        }
    }
}

/// `showHasEmptyModifier`.
// TODO (upstream): +1 Prefix Modifier allowed, -1 Suffix Modifier allowed
fn show_has_empty_modifier(item: &Item) -> Option<ItemHasEmptyModifier> {
    if item.rarity != Some(Rarity::Rare) || item.is_corrupted.is_some() || item.is_mirrored == Some(true) {
        return None;
    }

    let random_mods: Vec<_> = item
        .new_mods
        .iter()
        .filter(|m| {
            matches!(
                m.info.kind,
                ModifierType::Explicit | ModifierType::Fractured | ModifierType::Veiled | ModifierType::Crafted
            )
        })
        .collect();
    let crafted_mod = random_mods.iter().find(|m| m.info.kind == ModifierType::Crafted);

    if (random_mods.len() == 5 && crafted_mod.is_none()) || (random_mods.len() == 6 && crafted_mod.is_some()) {
        let count = |g: Generation| random_mods.iter().filter(|m| m.info.generation == Some(g)).count() as i32;
        let (mut prefixes, mut suffixes) = (count(Generation::Prefix), count(Generation::Suffix));

        if let Some(crafted) = crafted_mod {
            if crafted.info.generation == Some(Generation::Prefix) {
                prefixes -= 1;
            } else {
                suffixes -= 1;
            }
        }

        if prefixes == 2 {
            return Some(ItemHasEmptyModifier::Prefix);
        }
        if suffixes == 2 {
            return Some(ItemHasEmptyModifier::Suffix);
        }
    }
    None
}

/// `enableAllFilters`
pub(crate) fn enable_all_filters(filters: &mut [StatFilter]) {
    for filter in filters {
        if filter.hidden.is_none() {
            filter.disabled = false;
        }
    }
}

/// `enableGoodRolledFilters`
fn enable_good_rolled_filters(filters: &mut [StatFilter], above_pct: f64) {
    for filter in filters {
        if filter.hidden.is_some() {
            continue;
        }
        match filter.roll.as_ref().and_then(|r| r.goodness) {
            None => filter.disabled = false,
            Some(goodness) if goodness >= above_pct => filter.disabled = false,
            Some(_) => {}
        }
    }
}
