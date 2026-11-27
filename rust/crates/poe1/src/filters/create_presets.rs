//! `filters/create-presets.ts`: the presets a price check offers, and which
//! one starts active.

use serde::{Deserialize, Serialize};
use wraex_core::Rarity;

use super::create_item_filters::{CreateOptions, create_filters};
use super::create_stat_filters::{Mode, create_exact_stat_filters, init_ui_mod_filters};
use super::pseudo::mercenary::create_mercenary_filters;
use super::{FilterOrGroup, FilterPreset, R, StatFilter, missing};
use crate::data::Data;
use crate::parser::item::Item;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::{ModifierType, sum_stats_by_mod_type};

use ItemCategory as C;

const ROMAN_NUMERALS: [&str; 5] = ["I", "II", "III", "IV", "V"];

/// `CreatePresetsOptions`: [`CreateOptions`] without `exact`, plus the stat
/// range in percent.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePresetsOptions {
    #[serde(flatten)]
    pub filters: CreateOptions,
    pub search_stat_range: f64,
}

/// `{ presets, active }`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Presets {
    pub active: String,
    pub presets: Vec<FilterPreset>,
}

fn stats(filters: Vec<StatFilter>) -> Vec<FilterOrGroup> {
    filters.into_iter().map(FilterOrGroup::Stat).collect()
}

/// `createPresets`'s Expedition Logbook branch: one preset per area mod.
fn logbook_presets(d: &Data, item: &Item, exact: &CreateOptions, range: f64) -> R<Presets> {
    let areas = item.logbook_area_mods.as_ref().ok_or_else(|| missing("logbookAreaMods"))?;
    let mut presets = Vec::new();
    for (idx, area) in areas.iter().enumerate() {
        presets.push(FilterPreset {
            // `ROMAN_NUMERALS[idx]` is `undefined` past five areas.
            id: ROMAN_NUMERALS.get(idx).copied().unwrap_or("undefined").to_owned(),
            filters: create_filters(d, item, exact)?,
            stats: stats(create_exact_stat_filters(d, item, &sum_stats_by_mod_type(area), range, None)?),
        });
    }
    Ok(Presets { active: ROMAN_NUMERALS[0].to_owned(), presets })
}

/// `createPresets`
pub fn create_presets(d: &Data, item: &Item, opts: &CreatePresetsOptions) -> R<Presets> {
    let exact = CreateOptions { exact: true, ..opts.filters.clone() };
    let range = opts.search_stat_range;
    let is = |c: ItemCategory| item.category.as_ref() == Some(&c);
    let rarity = item.rarity;
    let is_unique = rarity == Some(Rarity::Unique);

    if item.ref_name_is("Expedition Logbook") {
        return logbook_presets(d, item, &exact, range);
    } else if item.ref_name_is("Mercenary Warrant") {
        return Ok(Presets {
            active: "filters.preset_exact".to_owned(),
            presets: vec![FilterPreset {
                id: "filters.preset_exact".to_owned(),
                filters: create_filters(d, item, &exact)?,
                stats: create_mercenary_filters(d, item)?,
            }],
        });
    }

    let not_unique_in = |c: ItemCategory| is(c) && !is_unique;
    if (item.info().craftable.is_none() && !is_unique)
        || item.is_unidentified
        || rarity == Some(Rarity::Normal)
        || not_unique_in(C::Flask)
        || not_unique_in(C::Tincture)
        || not_unique_in(C::SanctumRelic)
        || not_unique_in(C::Idol)
        || is(C::Charm)
        || is(C::Map)
        || is(C::MemoryLine)
        || is(C::Invitation)
        || is(C::HeistContract)
        || is(C::HeistBlueprint)
        || is(C::Chart)
        || is(C::Sentinel)
    {
        if !is_unique && ((is(C::Map) && item.map_completion_reward.is_none()) || is(C::Chart)) {
            let bulk_preset = FilterPreset {
                id: "filters.preset_bulk".to_owned(),
                filters: create_filters(d, item, &exact)?,
                stats: stats(create_exact_stat_filters(d, item, &item.stats_by_type, range, Some(Mode::Bulk))?),
            };
            let blighted = item.info().area.as_ref().is_some_and(|a| a.blighted == Some(true));
            if rarity == Some(Rarity::Rare) && !item.is_unidentified && !blighted {
                let props_preset = FilterPreset {
                    id: "filters.preset_pseudo".to_owned(),
                    filters: create_filters(d, item, &exact)?,
                    stats: stats(create_exact_stat_filters(d, item, &item.stats_by_type, range, Some(Mode::Props))?),
                };
                let special = if is(C::Chart) {
                    let area = item.map_area.as_ref().ok_or_else(|| missing("mapArea"))?;
                    area.area.as_ref().ok_or_else(|| missing("mapArea.area"))?.special == Some(true)
                } else {
                    false
                };
                let active = if special { bulk_preset.id.clone() } else { props_preset.id.clone() };
                return Ok(Presets { active, presets: vec![props_preset, bulk_preset] });
            }
            return Ok(Presets { active: bulk_preset.id.clone(), presets: vec![bulk_preset] });
        }

        return Ok(Presets {
            active: "filters.preset_exact".to_owned(),
            presets: vec![FilterPreset {
                id: "filters.preset_exact".to_owned(),
                filters: create_filters(d, item, &exact)?,
                stats: stats(create_exact_stat_filters(d, item, &item.stats_by_type, range, None)?),
            }],
        });
    }

    let pseudo_preset = FilterPreset {
        id: "filters.preset_pseudo".to_owned(),
        filters: create_filters(d, item, &CreateOptions { exact: false, ..opts.filters.clone() })?,
        stats: init_ui_mod_filters(d, item, range)?,
    };

    let likely_finished_item = is_unique
        || item.stats_by_type.iter().any(|calc| calc.kind == ModifierType::Crafted)
        // Quality above 20 can sell bases; below 20 drops sometimes.
        || (item.quality == Some(20.0) && item.memory_strands.is_none_or(|s| s == 0.0 || s.is_nan()))
        || item.is_corrupted.is_some()
        || item.is_mirrored == Some(true);

    // `item.itemLevel! >= 82`: `undefined >= 82` is false.
    let has_crafting_value = item.is_synthesised == Some(true)
        || item.is_fractured == Some(true)
        || !item.influences.is_empty()
        || is(C::ClusterJewel)
        || (is(C::Jewel) && rarity == Some(Rarity::Magic))
        || (!is(C::Jewel) && !is(C::AbyssJewel) && item.item_level.is_some_and(|l| l >= 82.0));

    if likely_finished_item || !has_crafting_value {
        return Ok(Presets { active: pseudo_preset.id.clone(), presets: vec![pseudo_preset] });
    }

    let base_item_preset = FilterPreset {
        id: "filters.preset_base_item".to_owned(),
        filters: create_filters(d, item, &exact)?,
        stats: stats(create_exact_stat_filters(d, item, &item.stats_by_type, range, None)?),
    };

    Ok(Presets { active: pseudo_preset.id.clone(), presets: vec![pseudo_preset, base_item_preset] })
}
