//! `filters/pseudo/utils.ts`: stats looked up by ref, and `not`/count groups.

use super::super::{FilterError, FilterTag, R, StatFilter};
use crate::data::{Data, MergeKind, Stat, StatGroupResolver, StatOrGroup};
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::ModifierType;
use crate::parser::stat_translations::resolve_translation;

/// `ResolveHint`: which stat of a percent- or flag-merge group is meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveHint {
    PercentMerge { pick: MergeKind },
    FlagMerge { pick: MergeKind },
}

fn try_find_and_resolve_by_ref(
    d: &Data,
    name: &str,
    mod_type: ModifierType,
    item_category: Option<&ItemCategory>,
    hint: Option<ResolveHint>,
) -> R<Option<Stat>> {
    let Some(stat_or_group) = d.stat_by_ref_v2(name) else { return Ok(None) };

    let stat = match stat_or_group {
        StatOrGroup::Stat(stat) => Some(stat),
        StatOrGroup::Group(group) => match (&group.resolve, hint) {
            (StatGroupResolver::Select { .. } | StatGroupResolver::TrivialMerge, _) => {
                #[expect(
                    clippy::map_err_ignore,
                    reason = "upstream throws `Unexpected stat shape: {name}`; its error is not part of the message"
                )]
                let stat = resolve_translation(group, "", mod_type, item_category, None)
                    .map_err(|_| FilterError(format!("Unexpected stat shape: {name}")))?;
                stat
            }
            (StatGroupResolver::PercentMerge { kind }, Some(ResolveHint::PercentMerge { pick })) => {
                // NOTE (upstream): the 'value' stat can actually be a flag
                let k = if pick == MergeKind::Percent { MergeKind::Percent } else { MergeKind::Value };
                kind.iter().position(|x| *x == k).and_then(|i| group.stats.get(i).cloned())
            }
            (StatGroupResolver::FlagMerge { kind }, Some(ResolveHint::FlagMerge { pick })) => {
                let k = if pick == MergeKind::Flag { MergeKind::Flag } else { MergeKind::Value };
                kind.iter().position(|x| *x == k).and_then(|i| group.stats.get(i).cloned())
            }
            _ => return Ok(None),
        },
    };
    Ok(stat.filter(|s| s.trade.ids.contains_key(mod_type.as_str())))
}

/// `findAndResolveByRef`: throws `Unexpected stat shape` without a match.
pub fn find_and_resolve_by_ref(
    d: &Data,
    name: &str,
    mod_type: ModifierType,
    item_category: Option<&ItemCategory>,
    hint: Option<ResolveHint>,
) -> R<Stat> {
    try_find_and_resolve_by_ref(d, name, mod_type, item_category, hint)?
        .ok_or_else(|| FilterError(format!("Unexpected stat shape: {name}")))
}

/// `statToNotFilter`'s options.
#[derive(Debug, Clone, Copy)]
pub struct NotFilterOptions<'a> {
    pub stat: &'a Stat,
    pub kind: ModifierType,
    pub negate_string: bool,
    pub disabled: bool,
}

/// `statToNotFilter`
pub fn stat_to_not_filter(opts: NotFilterOptions<'_>) -> StatFilter {
    let matcher = opts
        .stat
        .matchers
        .iter()
        .find(|m| m.negate.unwrap_or(false) == opts.negate_string)
        .or_else(|| opts.stat.matchers.first());
    StatFilter {
        trade_id: opts.stat.trade.ids.get(opts.kind.as_str()).cloned(),
        stat_ref: opts.stat.stat_ref.clone(),
        text: matcher.map(|m| m.string.clone()).unwrap_or_default(),
        tag: FilterTag::Type(opts.kind),
        oils: None,
        mercenary: None,
        reminder_text: None,
        sources: Vec::new(),
        not: Some(true),
        roll: None,
        option: None,
        hidden: None,
        disabled: opts.disabled,
    }
}

fn meta_filter(trade_id: &str, text: &str, disabled: bool) -> StatFilter {
    StatFilter {
        trade_id: Some(vec![trade_id.to_owned()]),
        stat_ref: text.to_owned(),
        text: text.to_owned(),
        tag: FilterTag::FilterGroup,
        oils: None,
        mercenary: None,
        reminder_text: None,
        sources: Vec::new(),
        not: None,
        roll: None,
        option: None,
        hidden: None,
        disabled,
    }
}

/// `metaNotFilter`
pub fn meta_not_filter(disabled: bool) -> StatFilter {
    meta_filter("item.not_group", "Not", disabled)
}

/// `metaCountOneFilter`
pub fn meta_count_one_filter(disabled: bool) -> StatFilter {
    meta_filter("item.count_one_group", "Count (1)", disabled)
}
