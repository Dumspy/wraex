//! `ref/ee2/renderer/src/parser/augment-builder.ts`: the item editor's view of
//! runes and soul cores in an item's sockets.

use wraex_core::js;

use super::item::{EditorItem, Item};
use super::meta::{ARMOUR, ItemCategory, is_martial_weapon};
use crate::data::{AugmentLineData, BaseType, Data, Namespace};

/// `selectAugmentEffectByItemCategory`
pub fn select_augment_effect_by_item_category(
    category: &ItemCategory,
    augment: &BaseType,
) -> Option<Vec<AugmentLineData>> {
    let lines = augment.augment.as_ref()?;
    Some(lines.iter().filter(|l| l.categories.iter().any(|c| c == category.as_str())).cloned().collect())
}

/// `buildEditorItems`: augments without an effect for `category` are left out.
pub fn build_editor_items(augments: &[BaseType], category: &ItemCategory, existing: bool) -> Vec<EditorItem> {
    augments
        .iter()
        .filter_map(|augment| {
            let effect = select_augment_effect_by_item_category(category, augment).filter(|e| !e.is_empty())?;
            Some(build_editor_item(augment, effect, existing))
        })
        .collect()
}

/// `replaceHashWithValues` (`Parser.ts`): each value replaces the next `#`.
pub fn replace_hash_with_values(template: &str, values: &[f64]) -> String {
    let mut result = template.to_owned();
    for value in values {
        result = result.replacen('#', &js::number_to_string(*value), 1);
    }
    result
}

/// `zipHashAndValues`: the augment's lines with their values filled in.
fn zip_hash_and_values(strings: &[String], values: &[Vec<f64>]) -> String {
    let Some(first) = values.first() else {
        // `values[0].values()` on an empty list throws upstream; no data has one.
        return strings.join("\n");
    };
    if !values.iter().all(|v| v == first) {
        return "ve(shouldn't see this)".to_owned();
    }
    let mut generator = first.iter().copied();
    strings
        .iter()
        .map(|s| {
            let count = s.matches('#').count();
            let values: Vec<f64> = (0..count).map(|_| generator.next().unwrap_or(0.0)).collect();
            replace_hash_with_values(s, &values)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_editor_item(augment: &BaseType, stats: Vec<AugmentLineData>, existing: bool) -> EditorItem {
    let strings: Vec<String> = stats.iter().map(|s| s.string.clone()).collect();
    let values: Vec<Vec<f64>> = stats.iter().map(|s| s.values.clone()).collect();
    EditorItem {
        existing: existing.then_some(true),
        socket_bound: None,
        name: augment.name.clone(),
        ref_name: augment.ref_name.clone(),
        icon: augment.icon.clone(),
        display_string: zip_hash_and_values(&strings, &values),
        stats,
        base_item: augment.clone(),
    }
}

/// The price-check widget's `savedAugments` setting: augments to put into empty
/// sockets, by category key (`ItemCategory` value, `martialWeapon`,
/// `casterWeapon`, `armour`, `all`).
pub type SavedAugments = indexmap::IndexMap<String, Vec<Option<String>>>;

/// `getSavedAugments`
pub fn get_saved_augments(d: &Data, item: &Item, lookup: &SavedAugments) -> Vec<Option<BaseType>> {
    if item.augment_sockets.is_none() {
        return Vec::new();
    }
    let non_empty = |key: &str| lookup.get(key).filter(|l| !l.is_empty());
    let category = item.category.as_ref();
    let ref_names = category
        .and_then(|c| non_empty(c.as_str()))
        .or_else(|| category.filter(|c| is_martial_weapon(c)).and_then(|_| non_empty("martialWeapon")))
        .or_else(|| {
            category
                .filter(|c| matches!(c, ItemCategory::Wand | ItemCategory::Staff))
                .and_then(|_| non_empty("casterWeapon"))
        })
        .or_else(|| category.filter(|c| ARMOUR.contains(c)).and_then(|_| non_empty("armour")))
        .or_else(|| non_empty("all"));
    ref_names
        .map(|names| {
            names
                .iter()
                .map(|r| r.as_ref().and_then(|r| d.item_by_ref(Namespace::Item, r)).and_then(|b| b.into_iter().next()))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fills_hashes_in_order() {
        assert_eq!(replace_hash_with_values("Adds # to # Fire", &[3.0, 7.5]), "Adds 3 to 7.5 Fire");
        let strings = vec!["#% increased Damage".to_owned(), "+# to Life".to_owned()];
        assert_eq!(
            zip_hash_and_values(&strings, &[vec![20.0, 5.0], vec![20.0, 5.0]]),
            "20% increased Damage\n+5 to Life"
        );
        assert_eq!(zip_hash_and_values(&strings, &[vec![20.0], vec![21.0]]), "ve(shouldn't see this)");
    }
}
