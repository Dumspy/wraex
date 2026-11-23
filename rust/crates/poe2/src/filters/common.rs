//! `filters/common.ts`: whether an item is finished or a crafting base, and its
//! explicit modifier count.

use wraex_core::Rarity;

use crate::parser::item::Item;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::{EXPLICIT_MOD_TYPES, ModifierType};

/// `maxUsefulItemLevel`
pub fn max_useful_item_level(category: Option<&ItemCategory>) -> f64 {
    match category {
        Some(ItemCategory::Wand | ItemCategory::Staff) => 81.0,
        Some(ItemCategory::Relic) => 80.0,
        Some(ItemCategory::Tablet | ItemCategory::Jewel | ItemCategory::Map) => 1.0,
        _ => 82.0,
    }
}

fn truthy(x: Option<f64>) -> bool {
    x.is_some_and(|v| v != 0.0 && !v.is_nan())
}

/// `likelyFinishedItem`
pub fn likely_finished_item(item: &Item) -> bool {
    item.rarity == Some(Rarity::Unique)
        || item.stats_by_type.iter().any(|calc| calc.kind == ModifierType::Crafted)
        // Quality above 20 can sell bases; below 20 drops sometimes.
        || (item.quality == Some(20.0) && item.quality_type.as_deref().is_none_or(str::is_empty))
        || !item.is_modifiable()
}

/// `hasCraftingValue`
pub fn has_crafting_value(item: &Item) -> bool {
    let is = |c: ItemCategory| item.category.as_ref() == Some(&c);
    item.is_modifiable()
        // Synthesised and influenced items are not in PoE2 yet.
        && (item.is_synthesised == Some(true)
            || item.is_fractured == Some(true)
            || !item.influences.is_empty()
            // Clusters (deprecated)
            || is(ItemCategory::ClusterJewel)
            || (is(ItemCategory::Jewel) && item.rarity == Some(Rarity::Magic))
            // High item level (minus 15: low ones still seem to sell). `undefined >= x` is false.
            || item.item_level.is_some_and(|l| l >= max_useful_item_level(item.category.as_ref()) - 15.0)
            // Exceptional item
            || item.augment_sockets.as_ref().is_some_and(|s| s.current > s.normal)
            || (truthy(item.quality) && item.quality.is_some_and(|q| q > 20.0)))
}

/// `explicitModifierCount`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModifierCount {
    pub prefixes: usize,
    pub suffixes: usize,
    pub total: usize,
}

/// `explicitModifierCount`
pub fn explicit_modifier_count(item: &Item) -> ModifierCount {
    use crate::parser::advanced_mod_desc::Generation;
    let random_mods: Vec<_> = item.new_mods.iter().filter(|m| EXPLICIT_MOD_TYPES.contains(&m.info.kind)).collect();
    let count = |g: Generation| random_mods.iter().filter(|m| m.info.generation == Some(g)).count();
    let (prefixes, suffixes) = (count(Generation::Prefix), count(Generation::Suffix));
    ModifierCount { prefixes, suffixes, total: prefixes + suffixes }
}
