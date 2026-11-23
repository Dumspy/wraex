//! `filters/pseudo/flasks.ts`

use wraex_core::Rarity;

use super::super::R;
use super::super::create_stat_filters::Ctx;
use super::utils::{find_and_resolve_by_ref, stat_to_not_filter};
use crate::parser::modifiers::ModifierType;

const INCR_CHARGE_RECOVERY: &str = "#% increased Charge Recovery";
const INCR_EFFECT: &str = "#% increased effect";

/// `applyFlaskHybridMod`: <https://github.com/SnosMe/awakened-poe-trade/issues/758>
pub fn apply_flask_hybrid_mod(ctx: &mut Ctx) -> R<()> {
    let applicable = ctx.item.rarity == Some(Rarity::Magic)
        && ctx.filters.iter().any(|f| f.stat_ref == INCR_CHARGE_RECOVERY)
        && !ctx.filters.iter().any(|f| f.stat_ref == INCR_EFFECT);
    if !applicable {
        return Ok(());
    }

    let effect_stat =
        find_and_resolve_by_ref(ctx.d, INCR_EFFECT, ModifierType::Explicit, ctx.item.category.as_ref(), None)?;
    let filter = stat_to_not_filter(&effect_stat, ModifierType::Explicit, true, false);
    let insert_after = ctx.filters.iter().position(|f| f.stat_ref == INCR_CHARGE_RECOVERY).expect("checked above");
    ctx.filters.insert(insert_after + 1, filter);
    Ok(())
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    vec![INCR_CHARGE_RECOVERY, INCR_EFFECT]
}
