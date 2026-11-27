//! `filters/create-stat-filters.ts`: the stat filters of a preset, from the
//! item's summed stats.

use wraex_core::Rarity;
use wraex_core::filters::{Rounding, percent_roll, percent_roll_delta, round_roll};

use super::common::{explicit_modifier_count, max_useful_item_level};
use super::pseudo::anointments::{apply_anointment_rules, decode_oils};
use super::pseudo::item_property::{filter_base_percentile, filter_item_prop};
use super::pseudo::{atzoatl_rules, filter_pseudo, missing_fractured_rules, reflection_rules};
use super::{
    AdditionalInfo, EmptyModifierCounts, FilterError, FilterOption, FilterRoll, FilterTag, ItemHasEmptyModifier,
    MinMax, R, StatFilter, missing,
};
use crate::data::{Data, stat_better};
use crate::parser::advanced_mod_desc::Generation;
use crate::parser::get_max_sockets;
use crate::parser::item::Item;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::{
    ModifierType, StatCalculated, StatRoll, TotalMode, stat_sources_total, translate_stat_with_roll,
};

use ItemCategory as C;

/// `FiltersCreationContext`
#[derive(Debug)]
pub struct Ctx<'a> {
    pub d: &'a Data,
    pub item: &'a Item,
    pub search_in_range: f64,
    pub filters: Vec<StatFilter>,
    pub stats_by_type: Vec<StatCalculated>,
}

/// The stat filter options of `createPresets`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StatOptions {
    pub search_stat_range: f64,
    pub default_all_selected: bool,
}

fn is(item: &Item, category: &ItemCategory) -> bool {
    item.category.as_ref() == Some(category)
}

const fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

const fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) }
}

/// `filter.roll!`
pub(crate) fn roll_mut(filter: &mut StatFilter) -> R<&mut FilterRoll> {
    let stat_ref = filter.stat_ref.clone();
    filter.roll.as_mut().ok_or_else(|| FilterError(format!("no roll: {stat_ref}")))
}

/// `createExactStatFilters`
#[expect(
    clippy::too_many_lines,
    reason = "line-for-line port of upstream `createExactStatFilters` (filters/create-stat-filters.ts)"
)]
pub fn create_exact_stat_filters(
    d: &Data,
    item: &Item,
    stats_by_type: &[StatCalculated],
    opts: StatOptions,
) -> R<Vec<StatFilter>> {
    let search_in_range = if is(item, &C::Tablet) { 0.0 } else { opts.search_stat_range.min(2.0) };

    if item.map_blighted.is_some() || is(item, &C::Invitation) {
        return Ok(Vec::new());
    }
    if item.is_unidentified && item.rarity == Some(Rarity::Unique) && item.is_synthesised != Some(true) {
        // Implicits are wanted here.
        let mut out = Vec::new();
        for calc in stats_by_type.iter().filter(|c| c.kind == ModifierType::Implicit) {
            let mut filter = calculated_stat_to_filter(d, calc, search_in_range, item, true)?;
            filter.disabled = false;
            out.push(filter);
        }
        return Ok(out);
    }

    let mut keep_by_type = vec![
        ModifierType::Pseudo,
        ModifierType::Fractured,
        ModifierType::Desecrated,
        ModifierType::Crafted,
        ModifierType::Enchant,
        ModifierType::Necropolis,
        ModifierType::Sanctum,
        ModifierType::Skill,
    ];

    if item.influences.is_empty() && item.is_fractured != Some(true) && !is(item, &C::Tincture) {
        keep_by_type.push(ModifierType::Implicit);
    }

    let few_explicits = item.rarity == Some(Rarity::Magic)
        || (item.rarity == Some(Rarity::Rare) && explicit_modifier_count(item).total < 5);
    let excluded = [C::ClusterJewel, C::Map, C::HeistContract, C::HeistBlueprint, C::Sentinel];
    if few_explicits && !item.category.as_ref().is_some_and(|c| excluded.contains(c)) {
        keep_by_type.push(ModifierType::Explicit);
    }

    if is(item, &C::Flask) {
        keep_by_type.push(ModifierType::Crafted);
    }

    let mut ctx = Ctx {
        d,
        item,
        search_in_range,
        filters: Vec::new(),
        stats_by_type: stats_by_type.iter().filter(|c| keep_by_type.contains(&c.kind)).cloned().collect(),
    };

    // "uses remaining" pseudo for tablets.
    if is(item, &C::Tablet) {
        filter_pseudo(&mut ctx)?;
    }

    for calc in &ctx.stats_by_type {
        let filter = calculated_stat_to_filter(d, calc, ctx.search_in_range, item, true)?;
        ctx.filters.push(filter);
    }

    if item.ref_name_is("Chronicle of Atzoatl") {
        atzoatl_rules::apply_rules(&mut ctx.filters)?;
        return Ok(ctx.filters);
    }
    if item.ref_name_is("Mirrored Tablet") {
        reflection_rules::apply_rules(&mut ctx.filters)?;
        return Ok(ctx.filters);
    }

    let fractured = item.is_fractured == Some(true);
    for filter in &mut ctx.filters {
        // Show every mod for exact.
        filter.hidden = None;

        // Only fractured explicits; no implicits on tablets.
        if (fractured && filter.tag == FilterTag::EXPLICIT)
            || (is(item, &C::Tablet) && filter.tag == FilterTag::IMPLICIT)
        {
            continue;
        }

        if is(item, &C::Tablet) {
            filter.disabled = false;
        } else if filter.tag == FilterTag::EXPLICIT {
            let top_tier = filter.sources.iter().any(|s| s.modifier.info.tier.is_some_and(|t| t <= 2.0));
            filter.disabled = !top_tier;
        } else if filter.tag != FilterTag::Property {
            filter.disabled = false;
        }

        if filter.stat_ref == "# uses remaining" {
            let roll = roll_mut(filter)?;
            roll.min = Some(roll.value);
            roll.default.min = roll.value;
            roll.default.max = roll.value;
        }
    }

    // Fractured, but no mod actually is: a fixed game bug, cheap to keep.
    if fractured && !ctx.filters.iter().any(|f| f.tag == FilterTag::FRACTURED) {
        let mut explicit_stats = Vec::new();
        for calc in stats_by_type.iter().filter(|c| c.kind == ModifierType::Explicit) {
            explicit_stats.push(calculated_stat_to_filter(d, calc, ctx.search_in_range, item, true)?);
        }
        missing_fractured_rules::apply_rules(&mut ctx.filters, explicit_stats);
    }

    if let Some(empty) = show_has_empty_modifier(&ctx) {
        ctx.filters.push(empty_modifier_filter(empty, false, None));
    }

    if is(item, &C::ClusterJewel) {
        apply_cluster_jewel_rules(&mut ctx.filters)?;
    } else if is(item, &C::Flask) {
        apply_flask_rules(&mut ctx.filters);
    } else if is(item, &C::MemoryLine) || is(item, &C::SanctumRelic) || is(item, &C::Charm) || is(item, &C::Relic) {
        enable_all_filters(&mut ctx.filters);
    }

    if opts.default_all_selected {
        enable_all_filters(&mut ctx.filters);
    }

    Ok(ctx.filters)
}

/// The `# Empty Modifier` filter.
fn empty_modifier_filter(empty: EmptyModifier, disabled: bool, hidden: Option<&'static str>) -> StatFilter {
    let roll = empty.counts.0.get(empty.empty.value()).copied().unwrap_or(0.0);
    StatFilter {
        option: Some(FilterOption { value: Some(wraex_core::num::usize_to_f64(empty.empty.value())) }),
        additional_info: Some(AdditionalInfo { elemental_info: None, empty_modifier_info: Some(empty.counts) }),
        roll: Some(FilterRoll {
            value: roll,
            min: Some(roll),
            max: None,
            default: MinMax { min: roll, max: roll },
            bounds: None,
            trade_invert: None,
            dp: false,
            is_negated: false,
            goodness: None,
        }),
        hidden,
        ..StatFilter::plain("item.has_empty_modifier", "# Empty Modifier", FilterTag::PSEUDO, disabled)
    }
}

/// `initUiModFilters`
pub fn init_ui_mod_filters(d: &Data, item: &Item, opts: StatOptions) -> R<Vec<StatFilter>> {
    let explicit_like =
        |k: ModifierType| matches!(k, ModifierType::Fractured | ModifierType::Desecrated | ModifierType::Crafted);
    let mut ctx = Ctx {
        d,
        item,
        search_in_range: if item.rarity == Some(Rarity::Normal) { 100.0 } else { opts.search_stat_range },
        filters: Vec::new(),
        stats_by_type: item
            .stats_by_type
            .iter()
            .map(|calc| {
                let has_explicit = calc.stat.trade.ids.as_ref().is_some_and(|ids| ids.contains_key("explicit"));
                if explicit_like(calc.kind) && has_explicit {
                    StatCalculated { kind: ModifierType::Explicit, ..calc.clone() }
                } else {
                    calc.clone()
                }
            })
            .collect(),
    };

    if !item.ref_name_is("Split Personality") {
        filter_item_prop(&mut ctx)?;
        // TODO (upstream): other options; don't include trade site uniques with random augments
        let max_sockets = get_max_sockets(item);
        if item.rarity != Some(Rarity::Unique) || max_sockets == 0.0 || max_sockets.is_nan() {
            filter_pseudo(&mut ctx)?;
        }
        if item.ref_name_is("Emperor's Vigilance") {
            filter_base_percentile(&mut ctx)?;
        }
    }

    if item.is_modifiable() {
        ctx.stats_by_type.retain(|m| !explicit_like(m.kind));
        // Sources are merged, so keep only the ones of the stat's own type: the
        // filters below then hold only their own rolls.
        ctx.stats_by_type.extend(item.stats_by_type.iter().filter(|m| explicit_like(m.kind)).map(|m| StatCalculated {
            sources: m.sources.iter().filter(|s| s.modifier.info.kind == m.kind).cloned().collect(),
            ..m.clone()
        }));
    }

    if item.is_veiled == Some(true) {
        ctx.stats_by_type.retain(|m| m.kind != ModifierType::Veiled);
    }

    for calc in &ctx.stats_by_type {
        let filter = calculated_stat_to_filter(d, calc, ctx.search_in_range, item, true)?;
        ctx.filters.push(filter);
    }

    if item.is_veiled == Some(true) {
        for filter in &mut ctx.filters {
            filter.disabled = true;
        }
    }

    final_filter_tweaks(&mut ctx)?;

    if opts.default_all_selected {
        enable_all_filters(&mut ctx.filters);
    }

    Ok(ctx.filters)
}

/// `shortRollToFilter`. Only elemental rolls use it, so: no decimals, not
/// negated, higher is better.
pub fn short_roll_to_filter(roll: StatRoll, percent: f64, item: &Item) -> FilterRoll {
    let dp = false;
    let bounds = MinMax {
        min: percent_roll(roll.min, -0.0, Rounding::Floor, dp),
        max: percent_roll(roll.max, 0.0, Rounding::Ceil, dp),
    };
    let mut default = MinMax {
        min: percent_roll(roll.value, -percent, Rounding::Floor, dp),
        max: percent_roll(roll.value, percent, Rounding::Ceil, dp),
    };
    default.min = js_max(default.min, bounds.min);
    default.max = js_min(default.max, bounds.max);

    #[expect(clippy::float_cmp, reason = "JS `!==` on the rolled values")]
    let mut filter_roll = FilterRoll {
        value: round_roll(roll.value, dp),
        min: None,
        max: None,
        default,
        bounds: (item.rarity == Some(Rarity::Unique) && roll.min != roll.max).then_some(bounds),
        trade_invert: Some(false),
        dp,
        is_negated: false,
        goodness: None,
    };
    filter_fill_min_max(&mut filter_roll, stat_better::POSITIVE_ROLL);
    filter_roll
}

/// `calculatedStatToFilter`
#[expect(
    clippy::too_many_lines,
    reason = "line-for-line port of upstream `calculatedStatToFilter` (filters/create-stat-filters.ts)"
)]
pub fn calculated_stat_to_filter(
    d: &Data,
    calc: &StatCalculated,
    mut percent: f64,
    item: &Item,
    disabled: bool,
) -> R<StatFilter> {
    let StatCalculated { stat, sources, kind } = calc;
    let kind = *kind;

    let total_mode = if item.ref_name_is("Mirrored Tablet") { TotalMode::Max } else { TotalMode::Sum };
    let roll = stat_sources_total(sources, total_mode);
    let translation = translate_stat_with_roll(calc, roll.as_ref());

    let ids = stat.trade.ids.as_ref().ok_or_else(|| FilterError(format!("no trade ids: {}", stat.stat_ref)))?;
    let id_kind = if kind == ModifierType::AddedAugment { ModifierType::Augment } else { kind };
    let trade_id = ids.get(id_kind.as_str()).cloned();

    let mut filter = if stat.trade.option == Some(true) {
        let first = sources.first().ok_or_else(|| missing("sources[0]"))?;
        let same = roll.map(|r| r.option) == roll.map(|r| Some(r.value));
        let option = first.contributes.ok_or_else(|| missing("contributes"))?.option;
        StatFilter {
            trade_id,
            stat_ref: stat.stat_ref.clone(),
            text: if roll.is_none() || same {
                first.stat.translation.string.clone()
            } else {
                translation.string.clone()
            },
            tag: if kind == ModifierType::Enchant { FilterTag::ENCHANT } else { FilterTag::Variant },
            oils: decode_oils(calc),
            sources: sources.clone(),
            roll: None,
            option: Some(FilterOption { value: option }),
            hidden: None,
            disabled: false,
            additional_info: None,
            editor_added: None,
        }
    } else {
        StatFilter {
            trade_id,
            stat_ref: stat.stat_ref.clone(),
            text: translation.string.clone(),
            tag: FilterTag::Type(kind),
            oils: decode_oils(calc),
            sources: sources.clone(),
            roll: None,
            option: None,
            hidden: None,
            disabled,
            additional_info: None,
            editor_added: None,
        }
    };

    let has_generation = |g: Generation| sources.iter().any(|s| s.modifier.info.generation == Some(g));
    let unique = item.info().unique.as_ref();
    match kind {
        ModifierType::Implicit => {
            if has_generation(Generation::Eldritch) {
                filter.tag = FilterTag::Eldritch;
            } else if item.is_synthesised == Some(true) {
                filter.tag = FilterTag::Synthesised;
            } else if let Some(unique) = unique
                && unique.base.starts_with("Runemastered")
                && let Some(fixed_stats) = &unique.fixed_stats
                && !fixed_stats.contains(&filter.stat_ref)
            {
                filter.tag = FilterTag::Variant;
            }
        }
        ModifierType::Explicit => {
            if item.rarity == Some(Rarity::Unique) && has_generation(Generation::Mutated) {
                filter.tag = FilterTag::Mutated;
            }

            let named_in = |key: &str| {
                let list = d.list(key);
                sources.iter().any(|s| s.modifier.info.name.as_ref().is_some_and(|n| list.contains(n)))
            };
            if let Some(fixed_stats) = unique.and_then(|u| u.fixed_stats.as_ref()) {
                if !fixed_stats.contains(&filter.stat_ref) {
                    filter.tag = FilterTag::Variant;
                }
            } else if named_in("DELVE_MODS") {
                filter.tag = FilterTag::Delve;
            } else if named_in("VEILED_MODS") {
                // Can't drop from the ground, so not shown (`FilterTag.Unveiled`).
            } else if named_in("INCURSION_MODS") {
                filter.tag = FilterTag::Incursion;
            }
        }
        ModifierType::Enchant if has_generation(Generation::Corrupted) => filter.tag = FilterTag::Corrupted,
        _ => {}
    }

    if let Some(roll) = roll
        && (filter.option.is_none() || roll.option != Some(roll.value))
    {
        let mut goodness = None;
        if stat.better != stat_better::NOT_COMPARABLE {
            #[expect(clippy::float_cmp, reason = "JS `===` on the rolled values")]
            if roll.min == roll.max {
                goodness = Some(1.0);
            } else {
                let mut g = (roll.value - roll.min) / (roll.max - roll.min);
                if stat.better == stat_better::NEGATIVE_ROLL {
                    g = 1.0 - g;
                }
                goodness = Some(g);
            }
        }

        if (item.rarity == Some(Rarity::Magic) && (item.is_unmodifiable == Some(true) || !item.is_modifiable()))
            || stat.stat_ref == "Has # Charm Slot"
        {
            percent = 0.0;
        } else if item.rarity == Some(Rarity::Unique)
            || (item.rarity == Some(Rarity::Magic) && (is(item, &C::Jewel) || is(item, &C::Tablet)))
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

        let mut dp = stat.dp == Some(true);
        if !dp {
            for s in sources.iter().filter(|s| s.stat.stat.stat_ref == stat.stat_ref) {
                // `s.stat.roll!.dp`
                if s.stat.roll.as_ref().ok_or_else(|| missing("stat roll"))?.dp {
                    dp = true;
                    break;
                }
            }
        }

        let bounds = MinMax {
            min: percent_roll(roll.min, -0.0, Rounding::Floor, dp),
            max: percent_roll(roll.max, 0.0, Rounding::Ceil, dp),
        };

        let mut default = if stat.better == stat_better::NOT_COMPARABLE {
            MinMax { min: roll.value, max: roll.value }
        } else if item.rarity == Some(Rarity::Unique) {
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

        #[expect(clippy::float_cmp, reason = "JS `!==` on the rolled values")]
        let mut filter_roll = FilterRoll {
            value: round_roll(roll.value, dp),
            min: None,
            max: None,
            default,
            bounds: (item.rarity == Some(Rarity::Unique)
                && roll.min != roll.max
                && stat.better != stat_better::NOT_COMPARABLE)
                .then_some(bounds),
            trade_invert: stat.trade.inverted,
            dp,
            is_negated: false,
            goodness,
        };

        let reload = ids.get("pseudo").is_some_and(|p| matches!(p.as_slice(), [id] if id == "item.reload_time"));
        let used_better = if reload { stat_better::NEGATIVE_ROLL } else { stat.better };
        filter_fill_min_max(&mut filter_roll, used_better);

        if translation.negate {
            filter_adjustment_for_negate(&mut filter_roll);
        }
        filter.roll = Some(filter_roll);
    }

    hide_not_variable_stat(&mut filter, item);

    Ok(filter)
}

/// `hideNotVariableStat`
fn hide_not_variable_stat(filter: &mut StatFilter, item: &Item) {
    if item.rarity != Some(Rarity::Unique) {
        return;
    }
    if filter.tag == FilterTag::IMPLICIT && is(item, &C::Jewel) {
        return;
    }
    let tag = filter.tag;
    if (tag != FilterTag::IMPLICIT && tag != FilterTag::EXPLICIT && tag != FilterTag::PSEUDO)
        || (tag == FilterTag::PSEUDO && filter.sources.iter().any(|s| s.modifier.info.kind == ModifierType::Enchant))
    {
        return;
    }

    if filter.stat_ref == "# uses remaining" {
        return;
    }

    match &mut filter.roll {
        None => filter.hidden = Some("filters.hide_const_roll"),
        Some(roll) if roll.bounds.is_none() => {
            roll.min = None;
            roll.max = None;
            filter.hidden = Some("filters.hide_const_roll");
        }
        Some(_) => {}
    }
}

/// `filterFillMinMax`
const fn filter_fill_min_max(roll: &mut FilterRoll, better: i8) {
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
pub fn final_filter_tweaks(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;

    if is(item, &C::ClusterJewel) && item.rarity != Some(Rarity::Unique) {
        apply_cluster_jewel_rules(&mut ctx.filters)?;
    } else if is(item, &C::Flask) {
        apply_flask_rules(&mut ctx.filters);
    }

    if item.rarity == Some(Rarity::Unique)
        && !item.ref_name_is("Morior Invictus")
        && !item.ref_name_is("Darkness Enthroned")
    {
        hide_all_augments(&mut ctx.filters);
    }

    if let Some(empty) = show_has_empty_modifier(ctx) {
        ctx.filters.push(empty_modifier_filter(empty, true, Some("filters.hide_empty_mod")));
    }

    if is(item, &C::Amulet) || is(item, &C::Ring) {
        apply_anointment_rules(&mut ctx.filters, item);
    }

    let possible_catalyst = item.quality_type.as_ref().filter(|q| !q.is_empty()).and_then(|quality_type| {
        let catalysts = &ctx.d.catalyst_types();
        let group = if is(item, &C::Jewel) {
            Some(&catalysts.refined)
        } else if is(item, &C::Ring) || is(item, &C::Amulet) {
            Some(&catalysts.normal)
        } else {
            None
        };
        group?.iter().find(|c| c.tags.get(1) == Some(quality_type)).cloned()
    });

    for filter in &mut ctx.filters {
        if filter.tag == FilterTag::FRACTURED || filter.tag == FilterTag::DESECRATED || filter.tag == FilterTag::CRAFTED
        {
            let calc = item
                .stats_by_type
                .iter()
                .find(|m| m.stat.stat_ref == filter.stat_ref)
                .ok_or_else(|| missing("stat of a fractured filter"))?;
            let ids = calc.stat.trade.ids.as_ref().ok_or_else(|| missing("trade ids"))?;
            if ids.contains_key("explicit") {
                // Hide only if the mod has a matching explicit variant.
                filter.hidden = Some("filters.hide_for_crafting");
            }
        } else if filter.tag == FilterTag::SKILL
            && let Some(roll) = &filter.roll
        {
            // TODO (upstream): enable ilvl for these if over 80
            filter.disabled = roll.value < 19.0;
            if filter.disabled {
                filter.hidden = Some("filters.hide_not_max_level");
            }
        }
        if is(item, &C::Map) && filter.tag != FilterTag::Property && filter.tag != FilterTag::DESECRATED {
            filter.disabled = true;
            filter.hidden = Some("filters.hide_for_map");
        }
        // Never hide uses remaining on tablets, even unique ones.
        if is(item, &C::Tablet) && filter.stat_ref == "# uses remaining" {
            filter.hidden = None;
        }
        if let Some(added) = filter.sources.iter().find_map(|s| s.stat.from_added_augment.as_ref()) {
            filter.editor_added = Some(added.clone());
        }
        if filter.sources.iter().any(|s| s.modifier.info.added_incr == Some(true))
            && let Some(catalyst) = &possible_catalyst
        {
            filter.editor_added = Some(catalyst.clone());
        }
    }

    if item.rarity == Some(Rarity::Magic)
        && item.is_modifiable()
        && let Some(level) = item.item_level.filter(|l| *l != 0.0 && !l.is_nan())
        && max_useful_item_level(item.category.as_ref()) - 3.0 > level
    {
        ctx.filters.push(StatFilter {
            hidden: Some("filters.hide_low_ilvl"),
            ..StatFilter::plain("item.rarity_magic", "Rarity: Magic", FilterTag::PSEUDO, true)
        });
    }

    if item.rarity == Some(Rarity::Unique) || is(item, &C::Relic) {
        let count_visible = ctx.filters.iter().filter(|f| f.hidden.is_none()).count();
        if count_visible <= 3 {
            enable_all_filters(&mut ctx.filters);
        }
    }
    Ok(())
}

/// `applyClusterJewelRules`.
/// <https://www.poewiki.net/wiki/Cluster_Jewel#Optimal_passive_skill_amounts>
fn apply_cluster_jewel_rules(filters: &mut [StatFilter]) -> R<()> {
    for filter in filters {
        if filter.stat_ref == "# Added Passive Skills are Jewel Sockets" {
            filter.hidden = Some("filters.hide_const_roll");
            filter.disabled = true;
        }

        if filter.stat_ref == "Adds # Passive Skills" {
            filter.disabled = false;
            let roll = roll_mut(filter)?;
            let value = roll.value;
            #[expect(clippy::float_cmp, reason = "JS `===` on the rolled value")]
            if value == 4.0 {
                // 4 is [_, 5]
                roll.max = Some(5.0);
            } else if value == 5.0 {
                // 5 is [5, 5]
                roll.min = Some(roll.default.min);
            } else if [3.0, 6.0, 10.0, 11.0, 12.0].contains(&value) {
                // 3, 6, 10, 11, 12 are [n, _]
                roll.min = Some(roll.default.min);
                roll.max = None;
            }
            // else 2, 8, 9 are [_, n]
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

/// `hideAllAugments`
fn hide_all_augments(filters: &mut [StatFilter]) {
    for filter in filters {
        if filter.tag == FilterTag::AUGMENT || filter.tag == FilterTag::ADDED_AUGMENT {
            filter.disabled = true;
            if filter.stat_ref == "Destroys all Augment Sockets on the item to create a Jewel Socket" {
                continue;
            }
            filter.hidden = Some("filters.hide_const_roll");
        }
    }
}

/// `showHasEmptyModifier`'s result.
#[derive(Debug, Clone, Copy, PartialEq)]
struct EmptyModifier {
    empty: ItemHasEmptyModifier,
    counts: EmptyModifierCounts,
}

/// `showHasEmptyModifier`
fn show_has_empty_modifier(ctx: &Ctx) -> Option<EmptyModifier> {
    let item = ctx.item;
    if !item.is_modifiable() || is(item, &C::Map) {
        return None;
    }
    if item.rarity != Some(Rarity::Rare) && item.rarity != Some(Rarity::Magic) {
        return None;
    }

    let count = explicit_modifier_count(item);
    let (prefixes, suffixes, total) = (
        wraex_core::num::usize_to_f64(count.prefixes),
        wraex_core::num::usize_to_f64(count.suffixes),
        wraex_core::num::usize_to_f64(count.total),
    );
    let max = item_max_modifiers_by_slot(item, &ctx.filters);
    let [any, max_prefix, max_suffix] = max;

    #[expect(clippy::float_cmp, reason = "JS `!==` on the modifier counts and rolls")]
    if total != any && total != 0.0 {
        let empty = if suffixes == max_suffix {
            ItemHasEmptyModifier::Prefix
        } else if prefixes == max_prefix {
            ItemHasEmptyModifier::Suffix
        } else {
            ItemHasEmptyModifier::Any
        };
        let counts = EmptyModifierCounts([any - total, max_prefix - prefixes, max_suffix - suffixes]);
        return Some(EmptyModifier { empty, counts });
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

/// `itemBaseMaxModifiersOfType`
const fn item_base_max_modifiers_of_type(category: Option<&ItemCategory>, rarity: Option<Rarity>) -> f64 {
    match rarity {
        Some(Rarity::Normal) => 0.0,
        Some(Rarity::Magic) => 1.0,
        _ => match category {
            Some(C::Jewel | C::Tablet | C::Relic | C::SanctumRelic) => 2.0,
            _ => 3.0,
        },
    }
}

/// `itemMaxModifiersBySlot`: `[any, prefix, suffix]`.
fn item_max_modifiers_by_slot(item: &Item, filters: &[StatFilter]) -> [f64; 3] {
    let base = item_base_max_modifiers_of_type(item.category.as_ref(), item.rarity);

    let mut max = [2.0 * base, base, base];
    for filter in filters {
        let value = filter.roll.as_ref().map_or(0.0, |r| r.value);
        if filter.stat_ref == "# Prefix Modifier allowed" {
            if let Some(slot) = max.get_mut(ItemHasEmptyModifier::Prefix.value()) {
                *slot += value;
            }
        } else if filter.stat_ref == "# Suffix Modifier allowed"
            && let Some(slot) = max.get_mut(ItemHasEmptyModifier::Suffix.value())
        {
            *slot += value;
        }
    }

    for slot in [ItemHasEmptyModifier::Prefix, ItemHasEmptyModifier::Suffix] {
        if let Some(value) = max.get_mut(slot.value())
            && *value < 0.0
        {
            *value = 0.0;
        }
    }
    let prefix = max.get(ItemHasEmptyModifier::Prefix.value()).copied().unwrap_or(0.0);
    let suffix = max.get(ItemHasEmptyModifier::Suffix.value()).copied().unwrap_or(0.0);
    if let Some(any) = max.get_mut(ItemHasEmptyModifier::Any.value()) {
        *any = prefix + suffix;
    }
    max
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::item::Item;

    fn plain_filter(stat_ref: &str, value: f64) -> StatFilter {
        let roll = FilterRoll {
            value,
            min: Some(value),
            max: Some(value),
            default: MinMax { min: value, max: value },
            bounds: None,
            trade_invert: None,
            dp: false,
            is_negated: false,
            goodness: None,
        };
        StatFilter { roll: Some(roll), ..StatFilter::plain(stat_ref, stat_ref, FilterTag::EXPLICIT, false) }
    }

    fn item_of(rarity: Rarity) -> Item {
        let mut item = Item::test_item();
        item.rarity = Some(rarity);
        item
    }

    /// `specs/web/price-check/filters/create-stat-filters.test.ts` (`item mod count
    /// tests`): the base counts per rarity.
    #[test]
    fn max_modifiers_by_slot_follows_the_rarity() {
        for (rarity, expected) in [(Rarity::Normal, 0.0), (Rarity::Magic, 1.0), (Rarity::Rare, 3.0)] {
            assert_eq!(item_max_modifiers_by_slot(&item_of(rarity), &[]), [expected * 2.0, expected, expected]);
        }
    }

    /// `specs/web/price-check/filters/create-stat-filters.test.ts` (`item mod count
    /// tests`): stats that allow more (or fewer) prefixes and suffixes.
    #[test]
    #[expect(
        clippy::type_complexity,
        reason = "one row per `create-stat-filters.test.ts` case: rarity, allowed-modifier stats, expected prefix and suffix counts"
    )]
    fn max_modifiers_by_slot_reads_the_allowed_modifier_stats() {
        let cases: [(Rarity, &[(&str, f64)], f64, f64); 20] = [
            (Rarity::Rare, &[], 3.0, 3.0),
            (Rarity::Rare, &[("# to all Attributes", 1.0)], 3.0, 3.0),
            (Rarity::Magic, &[("# Suffix Modifier allowed", 2.0)], 1.0, 3.0),
            (Rarity::Magic, &[("# Prefix Modifier allowed", -1.0)], 0.0, 1.0),
            (Rarity::Rare, &[("# Suffix Modifier allowed", 2.0)], 3.0, 5.0),
            (Rarity::Rare, &[("# Suffix Modifier allowed", 1.0)], 3.0, 4.0),
            (Rarity::Rare, &[("# Suffix Modifier allowed", -1.0)], 3.0, 2.0),
            (Rarity::Rare, &[("# Suffix Modifier allowed", -2.0)], 3.0, 1.0),
            (Rarity::Rare, &[("# Suffix Modifier allowed", -3.0)], 3.0, 0.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", 2.0)], 5.0, 3.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", 1.0)], 4.0, 3.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", -1.0)], 2.0, 3.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", -2.0)], 1.0, 3.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", -3.0)], 0.0, 3.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", 1.0), ("# Suffix Modifier allowed", 1.0)], 4.0, 4.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", 2.0), ("# Suffix Modifier allowed", -1.0)], 5.0, 2.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", -1.0), ("# Suffix Modifier allowed", 2.0)], 2.0, 5.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", -2.0), ("# Suffix Modifier allowed", -2.0)], 1.0, 1.0),
            (Rarity::Rare, &[("# Prefix Modifier allowed", -3.0), ("# Suffix Modifier allowed", 2.0)], 0.0, 5.0),
            (Rarity::Rare, &[("# Suffix Modifier allowed", 2.0), ("# Suffix Modifier allowed", 2.0)], 3.0, 7.0),
        ];
        for (rarity, stats, prefixes, suffixes) in cases {
            let filters: Vec<StatFilter> =
                stats.iter().map(|(stat_ref, value)| plain_filter(stat_ref, *value)).collect();
            assert_eq!(
                item_max_modifiers_by_slot(&item_of(rarity), &filters),
                [prefixes + suffixes, prefixes, suffixes],
                "{stats:?}"
            );
        }
    }
}
