//! `filters/pseudo/anointments.ts`: the emotions behind an anointment.

use super::super::StatFilter;
use crate::parser::item::Item;
use crate::parser::modifiers::{ModifierType, StatCalculated};

const EMOTIONS: [&str; 13] = [
    "Diluted Liquid Ire",
    "Diluted Liquid Guilt",
    "Diluted Liquid Greed",
    "Liquid Paranoia",
    "Liquid Envy",
    "Liquid Disgust",
    "Liquid Despair",
    "Concentrated Liquid Fear",
    "Concentrated Liquid Suffering",
    "Concentrated Liquid Isolation",
    "Potent Liquid Melancholy",
    "Potent Liquid Ferocity",
    "Potent Liquid Contempt",
];

const HARDCODED_HIGH_VALUE_OILS: [&str; 3] =
    ["Concentrated Liquid Isolation", "Potent Liquid Ferocity", "Potent Liquid Contempt"];

/// `decodeOils`: an amulet enchant's emotions, else a ring's. Unlike PoE1, not
/// sorted. An index without an emotion reads as `undefined`, written `null`.
pub fn decode_oils(calc: &StatCalculated) -> Option<Vec<Option<&'static str>>> {
    if calc.kind != ModifierType::Enchant {
        return None;
    }

    let first = calc.sources.first();
    let mut encoded = first.and_then(|s| s.stat.translation.oils.clone()).filter(|e| !e.is_empty());
    if encoded.is_none()
        && let Some(anointments) = &calc.stat.anointments
    {
        encoded = if anointments.len() == 1 {
            Some(anointments[0].oils.clone())
        } else {
            let roll = first.and_then(|s| s.stat.roll.as_ref()).map(|r| r.value);
            anointments.iter().find(|a| Some(a.roll) == roll).map(|a| a.oils.clone())
        };
    }
    let encoded = encoded.filter(|e| !e.is_empty())?;

    Some(
        encoded
            .split(',')
            .map(wraex_core::js::number)
            .map(|i| if i.fract() == 0.0 && i >= 0.0 { EMOTIONS.get(i as usize).copied() } else { None })
            .collect(),
    )
}

/// `applyAnointmentRules`
pub fn apply_anointment_rules(filters: &mut [StatFilter], item: &Item) {
    let Some(anointment) = filters.iter_mut().find(|f| f.oils.is_some()) else { return };

    if item.talisman_tier.is_some_and(|t| t != 0.0 && !t.is_nan()) {
        anointment.disabled = false;
    } else if item.is_modifiable() {
        let oils = anointment.oils.as_deref().unwrap_or_default();
        if !oils.iter().any(|o| o.is_some_and(|o| HARDCODED_HIGH_VALUE_OILS.contains(&o))) {
            anointment.hidden = Some("filters.hide_anointment");
            anointment.disabled = true;
        }
    }
}
