//! `filters/pseudo/heist.ts`: contract jobs and targets, blueprint wings.

use wraex_core::Rarity;

use super::super::create_stat_filters::Ctx;
use super::super::{FilterError, FilterTag, R, missing};
use super::item_property::{MAX_SAFE_INTEGER, Prop, prop_to_filter, up_to};
use super::utils::{NotFilterOptions, stat_to_not_filter};
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::ModifierType;

const ENCHANT_MODS: &str = "# Enchant Modifiers";

/// `applyHeistRules`
pub fn apply_heist_rules(ctx: &mut Ctx) -> R<()> {
    apply_contract_rules(ctx)?;
    apply_blueprint_rules(ctx)
}

fn apply_contract_rules(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;
    if item.category != Some(ItemCategory::HeistContract) || item.rarity == Some(Rarity::Unique) {
        return Ok(());
    }

    let contract = item.heist_contract.as_ref();
    if let (Some(job), Some(level)) =
        (contract.and_then(|c| c.required_job), contract.and_then(|c| c.job_level).filter(|l| *l != 0.0 && !l.is_nan()))
    {
        let internal_id = match job {
            "Lockpicking" => "item.heist_job_lockpicking",
            "Brute Force" => "item.heist_job_bruteforce",
            "Perception" => "item.heist_job_perception",
            "Demolition" => "item.heist_job_demolition",
            "Counter-Thaumaturgy" => "item.heist_job_counterthaumaturgy",
            "Trap Disarmament" => "item.heist_job_trapdisarmament",
            "Agility" => "item.heist_job_agility",
            "Deception" => "item.heist_job_deception",
            "Engineering" => "item.heist_job_engineering",
            other => return Err(FilterError(format!("unknown heist job: {other}"))),
        };
        let prop =
            Prop { disabled: Some(false), ..Prop::new(internal_id, internal_id, up_to(level, MAX_SAFE_INTEGER)) };
        ctx.filters.push(prop_to_filter(prop, item, 0.0));
    }

    if contract.and_then(|c| c.target_value) == Some("Priceless") {
        let prop =
            Prop { disabled: Some(false), ..Prop::new("Heist Target: Priceless", "item.heist_target_priceless", None) };
        ctx.filters.push(prop_to_filter(prop, item, ctx.search_in_range));
    }
    Ok(())
}

fn apply_blueprint_rules(ctx: &mut Ctx) -> R<()> {
    let item = ctx.item;
    if item.category != Some(ItemCategory::HeistBlueprint) || item.rarity == Some(Rarity::Unique) {
        return Ok(());
    }

    if !ctx.filters.iter().any(|f| f.tag == FilterTag::ENCHANT) {
        let stat = ctx.d.pseudo_stat_by_ref(ENCHANT_MODS).ok_or_else(|| missing(ENCHANT_MODS))?;
        ctx.filters.push(stat_to_not_filter(NotFilterOptions {
            stat: &stat,
            kind: ModifierType::Pseudo,
            negate_string: false,
            disabled: false,
        }));
    }

    let blueprint = item.heist_blueprint.as_ref().ok_or_else(|| missing("heistBlueprint"))?;
    let revealed = blueprint.wings_revealed.ok_or_else(|| missing("wingsRevealed"))?;
    let total = blueprint.wings_total.ok_or_else(|| missing("wingsTotal"))?;

    let prop = Prop {
        disabled: Some(false),
        ..Prop::new("Wings Revealed: #", "item.heist_wings_revealed", up_to(revealed, MAX_SAFE_INTEGER))
    };
    ctx.filters.push(prop_to_filter(prop, item, 0.0));

    let prop = Prop {
        disabled: Some(false),
        ..Prop::new("Total Wings: #", "item.heist_wings_total", up_to(total, MAX_SAFE_INTEGER))
    };
    ctx.filters.push(prop_to_filter(prop, item, 0.0));
    Ok(())
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    vec![ENCHANT_MODS]
}
