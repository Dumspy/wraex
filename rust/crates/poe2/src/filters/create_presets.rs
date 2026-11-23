//! `filters/create-presets.ts`: the presets a price check offers, and which
//! one starts active.

use serde::{Deserialize, Serialize};
use wraex_core::Rarity;

use super::common::{has_crafting_value, likely_finished_item};
use super::create_item_filters::{CreateOptions, create_filters};
use super::create_stat_filters::{StatOptions, create_exact_stat_filters, init_ui_mod_filters};
use super::create_unique_filters::{PRESET_UNIQUES, create_unique_presets};
use super::{FilterPreset, R, missing};
use crate::data::Data;
use crate::parser::item::Item;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::sum_stats_by_mod_type;

use ItemCategory as C;

const ROMAN_NUMERALS: [&str; 5] = ["I", "II", "III", "IV", "V"];

/// `createPresets`' options: [`CreateOptions`] without `exact`, plus the stat
/// range in percent and whether every filter starts enabled.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePresetsOptions {
    #[serde(flatten)]
    pub filters: CreateOptions,
    pub search_stat_range: f64,
    pub default_all_selected: bool,
}

/// `{ presets, active }`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Presets {
    pub active: String,
    pub presets: Vec<FilterPreset>,
}

/// `createPresets`
pub fn create_presets(d: &Data, item: &Item, opts: &CreatePresetsOptions) -> R<Presets> {
    let exact = CreateOptions { exact: true, ..opts.filters.clone() };
    let stat_opts =
        StatOptions { search_stat_range: opts.search_stat_range, default_all_selected: opts.default_all_selected };
    let is = |c: ItemCategory| item.category.as_ref() == Some(&c);
    let is_unique = item.rarity == Some(Rarity::Unique);

    // Logbooks aren't real anymore.
    if item.ref_name_is("logbook here") {
        let areas = item.logbook_area_mods.as_ref().ok_or_else(|| missing("logbookAreaMods"))?;
        let mut presets = Vec::new();
        for (idx, area) in areas.iter().enumerate() {
            presets.push(FilterPreset {
                id: ROMAN_NUMERALS.get(idx).copied().unwrap_or("undefined").to_owned(),
                filters: create_filters(d, item, &exact)?,
                stats: create_exact_stat_filters(d, item, &sum_stats_by_mod_type(area), stat_opts)?,
            });
        }
        return Ok(Presets { active: ROMAN_NUMERALS[0].to_owned(), presets });
    }

    let exact_only = [
        C::Flask,
        C::Relic,
        C::Tincture,
        C::MemoryLine,
        C::Invitation,
        C::HeistContract,
        C::HeistBlueprint,
        C::Sentinel,
        C::Tablet,
        C::Wombgift,
    ];
    let has_trials = item.trials.as_ref().and_then(|t| t.number_of_trials).is_some_and(|n| n != 0.0 && !n.is_nan());
    if item.is_unidentified
        || item.rarity == Some(Rarity::Normal)
        || (item.info().craftable.is_none() && !is_unique)
        || (item.category.as_ref().is_some_and(|c| exact_only.contains(c)) && !is_unique)
        || (is(C::Currency) && has_trials)
    {
        return Ok(Presets {
            active: "filters.preset_exact".to_owned(),
            presets: vec![FilterPreset {
                id: "filters.preset_exact".to_owned(),
                filters: create_filters(d, item, &exact)?,
                stats: create_exact_stat_filters(d, item, &item.stats_by_type, stat_opts)?,
            }],
        });
    }

    if PRESET_UNIQUES.iter().any(|u| item.ref_name_is(u)) {
        return create_unique_presets(d, item, &opts.filters, opts.search_stat_range);
    }

    // TODO (upstream): pseudo change here
    let pseudo_preset = FilterPreset {
        id: "filters.preset_pseudo".to_owned(),
        filters: create_filters(d, item, &CreateOptions { exact: false, ..opts.filters.clone() })?,
        stats: init_ui_mod_filters(d, item, stat_opts)?,
    };

    if likely_finished_item(item) || !has_crafting_value(item) {
        return Ok(Presets { active: pseudo_preset.id.clone(), presets: vec![pseudo_preset] });
    }

    let base_item_preset = FilterPreset {
        id: "filters.preset_base_item".to_owned(),
        filters: create_filters(d, item, &exact)?,
        stats: create_exact_stat_filters(d, item, &item.stats_by_type, stat_opts)?,
    };

    Ok(Presets { active: pseudo_preset.id.clone(), presets: vec![pseudo_preset, base_item_preset] })
}
