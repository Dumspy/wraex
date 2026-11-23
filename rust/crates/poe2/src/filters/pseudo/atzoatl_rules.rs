//! `filters/pseudo/atzoatl-rules.ts`. Every rule is commented out upstream, so
//! what is left disables all filters (and reads each room's option).

use super::super::{R, StatFilter};

/// `applyRules` with an empty `RULES`.
pub fn apply_rules(filters: &mut [StatFilter]) -> R<()> {
    for filter in filters {
        filter.disabled = true;
    }
    Ok(())
}
