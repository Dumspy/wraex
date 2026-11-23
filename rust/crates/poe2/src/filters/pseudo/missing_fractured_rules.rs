//! `filters/pseudo/missing-fractured-rules.ts`: a fractured item whose text
//! marks no mod as fractured searches its explicits as fractured.

use super::super::{FilterTag, StatFilter};

/// `applyRules`
pub fn apply_rules(filters: &mut Vec<StatFilter>, explicit_stats: Vec<StatFilter>) {
    if filters.iter().any(|f| f.tag == FilterTag::FRACTURED) {
        return;
    }
    filters.extend(
        explicit_stats
            .into_iter()
            .filter(|f| f.tag == FilterTag::EXPLICIT)
            .map(|f| StatFilter { tag: FilterTag::FRACTURED, ..f }),
    );
}
