//! `filters/create-unique-filters.ts`: uniques with their own preset (Mageblood).

use super::create_item_filters::{CreateOptions, create_filters};
use super::create_presets::Presets;
use super::create_stat_filters::{Ctx, calculated_stat_to_filter};
use super::pseudo::item_property::{Prop, prop_to_filter};
use super::{FilterError, FilterPreset, MinMax, R, StatFilter, missing};
use crate::data::Data;
use crate::parser::item::Item;
use crate::parser::modifiers::{ModifierType, StatCalculated, StatRoll};

/// `PRESET_UNIQUES`
pub const PRESET_UNIQUES: [&str; 1] = ["Mageblood"];

/// `createUniquePresets`
pub fn create_unique_presets(d: &Data, item: &Item, opts: &CreateOptions, search_stat_range: f64) -> R<Presets> {
    if item.ref_name_is("Mageblood") {
        let exact = CreateOptions { exact: true, ..opts.clone() };
        return Ok(Presets {
            active: "filters.preset_exact".to_owned(),
            presets: vec![FilterPreset {
                id: "filters.preset_exact".to_owned(),
                filters: create_filters(d, item, &exact)?,
                stats: create_mageblood_filters(d, item, &item.stats_by_type, search_stat_range)?,
            }],
        });
    }
    Err(FilterError("item doesn't have unique presets".to_owned()))
}

/// `createMagebloodFilters`: the implicits, the duplicate count, and each
/// "Legacy of #" without its roll.
fn create_mageblood_filters(
    d: &Data,
    item: &Item,
    stats_by_type: &[StatCalculated],
    search_stat_range: f64,
) -> R<Vec<StatFilter>> {
    let ctx = Ctx {
        d,
        item,
        search_in_range: search_stat_range.min(2.0),
        filters: Vec::new(),
        stats_by_type: stats_by_type.to_vec(),
    };
    let mut filters = Vec::new();

    // All but the "Legacy of #" stats.
    for calc in &ctx.stats_by_type {
        if calc.kind != ModifierType::Explicit || calc.stat.stat_ref.starts_with("All Mage's Legacies") {
            filters.push(calculated_stat_to_filter(d, calc, ctx.search_in_range, item, true)?);
        }
    }

    let legacies: Vec<&StatCalculated> = ctx
        .stats_by_type
        .iter()
        .filter(|s| s.kind == ModifierType::Explicit && s.stat.stat_ref.starts_with("Legacy"))
        .collect();

    let duplicates = 4.0 - wraex_core::num::usize_to_f64(legacies.len());
    let prop = Prop {
        disabled: Some(false),
        hidden: Some("dont_hide_me"),
        ..Prop::new(
            "Duplicates: #",
            "item.duplicates",
            StatRoll { value: duplicates, min: duplicates, max: duplicates, option: None },
            legacies.iter().flat_map(|s| s.sources.iter().cloned()).collect(),
        )
    };
    let mut dup_filter = prop_to_filter(prop, &ctx)?;
    let roll = dup_filter.roll.as_mut().ok_or_else(|| missing("duplicates roll"))?;
    roll.bounds = Some(MinMax { min: 0.0, max: 3.0 });
    roll.min = Some(duplicates);
    filters.push(dup_filter);

    for calc in legacies {
        // A copy, so the item's own stat stays as it is.
        let mut copied = calc.clone();
        let mut source = copied.sources.first().cloned().ok_or_else(|| missing("sources[0]"))?;
        source.contributes =
            Some(StatRoll { value: 0.0, min: 0.0, max: 0.0, option: source.contributes.and_then(|c| c.option) });
        copied.sources = vec![source];
        let mut filter = calculated_stat_to_filter(d, &copied, ctx.search_in_range, item, true)?;
        filter.roll = None;
        filters.push(filter);
    }

    for filter in &mut filters {
        if filter.hidden.is_none() {
            filter.disabled = false;
        }
        if filter.hidden == Some("dont_hide_me") {
            filter.hidden = None;
        }
    }

    Ok(filters)
}
