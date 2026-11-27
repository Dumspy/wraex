//! `filters/pseudo/anointments.ts`: the oils behind an anointment.

use super::super::StatFilter;
use crate::parser::item::Item;
use crate::parser::modifiers::{ModifierType, StatCalculated};
use wraex_core::num::f64_to_usize;

const OILS: [&str; 14] = [
    "Prismatic Oil",
    "Clear Oil",
    "Sepia Oil",
    "Amber Oil",
    "Verdant Oil",
    "Teal Oil",
    "Azure Oil",
    "Indigo Oil",
    "Violet Oil",
    "Crimson Oil",
    "Black Oil",
    "Opalescent Oil",
    "Silver Oil",
    "Golden Oil",
];

/// `decodeOils`: the oils, most valuable first. An index without an oil reads
/// as `undefined` upstream, written as `null`; the data has none.
pub fn decode_oils(calc: &StatCalculated) -> Option<Vec<Option<&'static str>>> {
    if calc.kind != ModifierType::Enchant {
        return None;
    }

    let anointments = calc.stat.anointments.as_ref()?;
    let encoded = if anointments.len() == 1 {
        anointments.first().map(|a| &a.oils)
    } else {
        let roll = calc.sources.first().and_then(|s| s.stat.roll.as_ref()).map(|r| r.value);
        anointments.iter().find(|a| Some(a.roll) == roll).map(|a| &a.oils)
    };
    let encoded = encoded.filter(|e| !e.is_empty())?;

    let mut indexes: Vec<f64> = encoded.split(',').map(wraex_core::js::number).collect();
    // [Golden Oil ... Clear Oil]
    indexes.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    Some(
        indexes
            .into_iter()
            .map(|i| if i.fract() == 0.0 && i >= 0.0 { OILS.get(f64_to_usize(i)).copied() } else { None })
            .collect(),
    )
}

/// `applyAnointmentRules`
pub fn apply_anointment_rules(filters: &mut [StatFilter], item: &Item) {
    let Some(anointment) = filters.iter_mut().find(|f| f.oils.is_some()) else { return };

    if item.talisman_tier.is_some_and(|t| t != 0.0 && !t.is_nan()) {
        anointment.disabled = false;
    } else if item.is_corrupted.is_none() && item.is_mirrored != Some(true) {
        let first = anointment.oils.as_ref().and_then(|o| o.first().copied().flatten());
        if first != Some("Golden Oil") && first != Some("Silver Oil") {
            anointment.hidden = Some("filters.hide_anointment");
            anointment.disabled = true;
        }
    }
}
