//! `filters/pseudo/timeless-jewel.ts`: a timeless jewel's seed as a "count one"
//! group of every keystone variant.

use super::super::create_stat_filters::{Ctx, calculated_stat_to_filter};
use super::super::{FilterGroup, FilterTag, GroupKind, R, missing};
use super::utils::{find_and_resolve_by_ref, meta_count_one_filter};
use crate::data::stat_better;
use crate::parser::advanced_mod_desc::ParsedModifier;
use crate::parser::modifiers::sum_stats_by_mod_type;
use crate::parser::stat_translations::ParsedStat;

/// `filterTimelessJewelKeystones`
pub fn filter_timeless_jewel_keystones(ctx: &mut Ctx) -> R<()> {
    let Some(seed_index) = ctx
        .stats_by_type
        .iter()
        .position(|calc| calc.stat.better == stat_better::NOT_COMPARABLE && calc.stat.mod_family.is_some())
    else {
        return Ok(());
    };
    let Some(tree_seed) = ctx.stats_by_type.get(seed_index).cloned() else { return Ok(()) };

    let mut group =
        FilterGroup { group: GroupKind::One, expanded: true, meta: meta_count_one_filter(false), stats: Vec::new() };

    let first_source = tree_seed.sources.first().ok_or_else(|| missing("sources[0]"))?;
    for family_stat_ref in tree_seed.stat.mod_family.as_deref().unwrap_or_default() {
        let is_seed = *family_stat_ref == tree_seed.stat.stat_ref;
        let calc = if is_seed {
            tree_seed.clone()
        } else {
            let family_stat =
                find_and_resolve_by_ref(ctx.d, family_stat_ref, tree_seed.kind, ctx.item.category.as_ref(), None)?;
            let translation = family_stat.matchers.first().cloned().ok_or_else(|| missing("matchers[0]"))?;
            let modifier = ParsedModifier {
                info: first_source.modifier.info.clone(),
                stats: vec![ParsedStat { translation, stat: family_stat, roll: first_source.stat.roll.clone() }],
            };
            sum_stats_by_mod_type(&[modifier]).into_iter().next().ok_or_else(|| missing("family stat"))?
        };
        let mut filter = calculated_stat_to_filter(&calc, ctx.search_in_range, ctx.item)?;
        if !is_seed {
            filter.tag = FilterTag::PSEUDO;
            filter.disabled = true;
        }
        group.stats.push(filter);
    }

    ctx.stats_by_type.remove(seed_index);
    ctx.groups.push(group);
    Ok(())
}
