//! `filters/pseudo/maps.ts`: map and chart properties, and Valdo's lethal mods.

use wraex_core::Rarity;
use wraex_core::num::usize_to_f64;

use super::super::create_stat_filters::Ctx;
use super::super::{FilterError, R};
use super::item_property::{MAX_SAFE_INTEGER, NoSourcePseudo, Prop, no_source_pseudo_to_filter, prop_to_filter, up_to};
use super::utils::{NotFilterOptions, find_and_resolve_by_ref, stat_to_not_filter};
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::ModifierType;

const MORE_THREADS: &str = "More Scarabs: #%";
const MORE_MAPS: &str = "More Maps: #%";
const MORE_DIVINATION_CARDS: &str = "More Divination Cards: #%";
const MORE_CURRENCY: &str = "More Currency: #%";
const EXPLICIT_MODIFIERS: &str = "# Modifiers";

const VALDO_LETHAL_STATS: [&str; 1] = ["Players who Die in area are sent to the Void"];

fn truthy(x: Option<f64>) -> Option<f64> {
    x.filter(|v| *v != 0.0 && !v.is_nan())
}

/// `areaProps`'s `disabled` option.
#[derive(Debug, Clone, Copy, Default)]
struct AreaPropsDisabled {
    /// Atlas drops reuse this one.
    item_quantity: bool,
    item_rarity: bool,
    pack_size: bool,
}

/// `areaProps`: which of quantity, rarity and pack size start disabled.
fn area_props(disabled: AreaPropsDisabled, ctx: &mut Ctx) {
    let item = ctx.item;
    let props = [
        (item.area_item_quantity, "Item Quantity: +#%", "item.map_item_quantity", disabled.item_quantity),
        // Reuses quantity's.
        (item.area_atlas_drops, "Atlas Drops: +#%", "item.map_atlas_drops", disabled.item_quantity),
        (item.area_item_rarity, "Item Rarity: +#%", "item.map_item_rarity", disabled.item_rarity),
        (item.area_pack_size, "Monster Pack Size: +#%", "item.map_pack_size", disabled.pack_size),
    ];
    for (value, stat_ref, trade_id, start_disabled) in props {
        let Some(value) = truthy(value) else { continue };
        let prop =
            Prop { disabled: Some(start_disabled), ..Prop::new(stat_ref, trade_id, up_to(value, MAX_SAFE_INTEGER)) };
        ctx.filters.push(prop_to_filter(prop, item, ctx.search_in_range));
    }
}

/// `mapProps`
pub fn map_props(bulk: bool, ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;
    if item.category != Some(ItemCategory::Map)
        || item.map_completion_reward.is_some()
        || item.rarity == Some(Rarity::Unique)
    {
        return Ok(());
    }

    let push_pseudo = |ctx: &mut Ctx, stat_ref: &str, value: f64, max: f64, disabled: bool, range: f64| -> R<()> {
        let stat =
            ctx.d.pseudo_stat_by_ref(stat_ref).ok_or_else(|| FilterError(format!("no pseudo stat: {stat_ref}")))?;
        let opts =
            NoSourcePseudo { pseudo: stat, roll: up_to(value, max), dp: false, disabled: Some(disabled), hidden: None };
        let filter = no_source_pseudo_to_filter(opts, ctx.item, range)?;
        ctx.filters.push(filter);
        Ok(())
    };
    let range = ctx.search_in_range;

    if !bulk {
        let has_valuable_drops = [item.map_more_threads, item.map_more_currency, item.map_more_div_cards]
            .into_iter()
            .any(|v| truthy(v).is_some());

        area_props(AreaPropsDisabled { item_rarity: has_valuable_drops, ..Default::default() }, ctx);

        let more = [
            (item.map_more_maps, MORE_MAPS, true),
            (item.map_more_threads, MORE_THREADS, false),
            (item.map_more_currency, MORE_CURRENCY, false),
            (item.map_more_div_cards, MORE_DIVINATION_CARDS, false),
        ];
        for (value, stat_ref, disabled) in more {
            if let Some(value) = truthy(value) {
                push_pseudo(ctx, stat_ref, value, MAX_SAFE_INTEGER, disabled, range)?;
            }
        }
    }

    let explicit_mods = item
        .new_mods
        .iter()
        .filter(|m| m.info.kind == ModifierType::Explicit || m.info.kind == ModifierType::Fractured)
        .count();
    if explicit_mods >= 8 {
        push_pseudo(ctx, EXPLICIT_MODIFIERS, usize_to_f64(explicit_mods), 9.0, false, 0.0)?;
    }
    Ok(())
}

/// `chartProps`
pub fn chart_props(bulk: bool, ctx: &mut Ctx) {
    let item = ctx.item;
    if bulk || item.category != Some(ItemCategory::Chart) {
        return;
    }

    area_props(AreaPropsDisabled::default(), ctx);

    if let Some(sulphur) = truthy(item.chart_sulphur) {
        let prop = Prop {
            disabled: Some(false),
            ..Prop::new("Dead Man's Sulphur: +#%", "item.chart_sulphur", up_to(sulphur, MAX_SAFE_INTEGER))
        };
        ctx.filters.push(prop_to_filter(prop, item, ctx.search_in_range));
    }
}

/// `valdoBadMods`
pub fn valdo_bad_mods(ctx: &mut Ctx) -> R<()> {
    if ctx.item.map_completion_reward.is_none() {
        return Ok(());
    }

    for lethal_stat_ref in VALDO_LETHAL_STATS {
        if ctx.item.stats_by_type.iter().any(|calc| calc.stat.stat_ref == lethal_stat_ref) {
            continue;
        }

        let lethal_stat =
            find_and_resolve_by_ref(ctx.d, lethal_stat_ref, ModifierType::Explicit, ctx.item.category.as_ref(), None)?;
        ctx.filters.push(stat_to_not_filter(NotFilterOptions {
            stat: &lethal_stat,
            kind: ModifierType::Explicit,
            negate_string: false,
            disabled: false,
        }));
    }
    Ok(())
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    let mut refs = vec![MORE_THREADS, MORE_MAPS, MORE_DIVINATION_CARDS, MORE_CURRENCY, EXPLICIT_MODIFIERS];
    refs.extend(VALDO_LETHAL_STATS);
    refs
}
