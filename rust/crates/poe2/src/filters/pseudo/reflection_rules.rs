//! `filters/pseudo/reflection-rules.ts`. Every rule is commented out upstream:
//! reflections below difficulty 8 go, the rest are hidden.

use super::super::{FilterError, R, StatFilter};

/// `applyRules` with an empty `RULES`.
pub fn apply_rules(filters: &mut Vec<StatFilter>) -> R<()> {
    let mut i = 0;
    while i < filters.len() {
        let Some(filter) = filters.get(i) else { break };
        let difficulty =
            filter.roll.as_ref().ok_or_else(|| FilterError(format!("no roll: {}", filter.stat_ref)))?.value;
        if difficulty < 8.0 {
            filters.remove(i);
            continue;
        }
        if let Some(filter) = filters.get_mut(i) {
            filter.hidden = Some("low_tier_reflection");
        }
        i = i.saturating_add(1);
    }
    Ok(())
}
