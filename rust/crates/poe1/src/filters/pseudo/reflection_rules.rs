//! `filters/pseudo/reflection-rules.ts`: which Mirrored Tablet reflections are
//! worth searching for.

use super::super::{FilterError, R, StatFilter};

struct FilterRule {
    stat_ref: &'static str,
    disabled: Option<bool>,
    any_difficulty: bool,
}

const fn rule(stat_ref: &'static str) -> FilterRule {
    FilterRule { stat_ref, disabled: None, any_difficulty: false }
}

const fn enabled(stat_ref: &'static str) -> FilterRule {
    FilterRule { disabled: Some(false), ..rule(stat_ref) }
}

const RULES: [FilterRule; 15] = [
    rule("Reflection of the Breachlord (Difficulty #)"),
    FilterRule { any_difficulty: true, ..rule("Reflection of Delirium (Difficulty #)") },
    rule("Reflection of Tyranny (Difficulty #)"),
    rule("Reflection of the Trove (Difficulty #)"),
    rule("Reflection of Thralldom (Difficulty #)"),
    rule("Reflection of Phaaryl (Difficulty #)"),
    rule("Reflection of Perverted Faith (Difficulty #)"),
    enabled("Reflection of Kalandra (Difficulty #)"),
    rule("Reflection of Experimentation (Difficulty #)"),
    enabled("Reflection of the Sun (Difficulty #)"),
    rule("Reflection of the Monolith (Difficulty #)"),
    rule("Reflection of the Nightmare (Difficulty #)"),
    rule("Reflection of Azurite (Difficulty #)"),
    enabled("Reflection of Paradise (Difficulty #)"),
    enabled("Reflection of Angling (Difficulty #)"),
];

/// `applyRules`
pub fn apply_rules(filters: &mut Vec<StatFilter>) -> R<()> {
    let mut i = 0;
    while i < filters.len() {
        let Some(filter) = filters.get_mut(i) else { break };
        match RULES.iter().find(|r| r.stat_ref == filter.stat_ref) {
            None => {
                let difficulty =
                    filter.roll.as_ref().ok_or_else(|| FilterError(format!("no roll: {}", filter.stat_ref)))?.value;
                if difficulty < 8.0 {
                    filters.remove(i);
                    continue;
                }
                filter.hidden = Some("low_tier_reflection");
            }
            Some(rule) => {
                if rule.any_difficulty {
                    filter.roll = None;
                }
                if let Some(disabled) = rule.disabled {
                    filter.disabled = disabled;
                }
            }
        }
        i = i.saturating_add(1);
    }
    Ok(())
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    RULES.iter().map(|r| r.stat_ref).collect()
}
