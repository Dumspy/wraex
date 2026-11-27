//! `ref/apt/renderer/src/web/map-check`: the map check's stats
//! (`prepare-map-stats.ts`, `MapCheck.vue`) and the per-profile decisions
//! the user marks them with (`common.ts`).

use serde::{Deserialize, Serialize};
use wraex_core::Rarity;
use wraex_core::filters::round_roll;

use crate::data::Data;
use crate::parser::item::Item;
use crate::parser::meta::ItemCategory;
use crate::parser::modifiers::{ModifierType, StatCalculated, TotalMode, stat_sources_total, translate_stat_with_roll};

/// `PreparedStat`
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreparedStat {
    pub matcher: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub roll: Option<f64>,
}

/// `prepareMapStat`: the stat's text and its rounded roll, negated for a
/// negated translation.
pub fn prepare_map_stat(calc: &StatCalculated) -> PreparedStat {
    let roll = stat_sources_total(&calc.sources, TotalMode::Sum);
    let translation = translate_stat_with_roll(calc, roll.as_ref());
    let mut rounded = roll.map(|r| round_roll(r.value, translation.dp.unwrap_or(false)));
    if translation.negate
        && let Some(r) = &mut rounded
    {
        *r = -*r;
    }
    PreparedStat { matcher: translation.string, roll: rounded }
}

/// `MapCheck.vue` `mapStats`: the explicit stats, prepared.
pub fn map_stats(item: &Item) -> Vec<PreparedStat> {
    item.stats_by_type.iter().filter(|c| c.kind == ModifierType::Explicit).map(prepare_map_stat).collect()
}

/// `WidgetItemCheck.vue` `isMapLike`: the items the item check shows the map
/// check for.
pub fn is_map_like(item: &Item) -> bool {
    use ItemCategory as C;
    let is = |c: C| item.category.as_ref() == Some(&c);
    (is(C::Map) && item.rarity != Some(Rarity::Unique))
        || is(C::HeistContract)
        || is(C::HeistBlueprint)
        || is(C::Invitation)
        || is(C::Chart)
        || item.ref_name_is("Expedition Logbook")
}

/// `MapCheckConfig`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MapCheckConfig {
    pub profile: u32,
    pub show_new_stats: bool,
    pub selected_stats: Vec<MapCheckStat>,
}

/// `MapCheckStat`: a stat text and its decisions, one character per profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MapCheckStat {
    pub matcher: String,
    pub decision: String,
}

/// `StatTag`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatTag {
    Outdated = 1,
    HeistExclusive,
    UberMapExclusive,
}

/// The decision of one profile (`decisionSet[profileName - 1]`).
fn decision_at(decision_set: &str, profile: u32) -> Option<char> {
    decision_set.chars().nth(usize::try_from(profile.checked_sub(1)?).ok()?)
}

/// `decisionHasColor`. Three profiles are stored as `"---"`: `-` undecided,
/// `d` deadly, `w` warning, `g` good, `s` seen.
pub fn decision_has_color(decision_set: &str, profile: u32) -> bool {
    // `undefined !== '-'`: a missing profile counts as colored.
    let d = decision_at(decision_set, profile);
    d != Some('-') && d != Some('s')
}

/// `nextDecision`: deadly, warning, good, seen, then deadly again.
pub fn next_decision(current: &str) -> &'static str {
    match current {
        "d" => "w",
        "w" => "g",
        "g" => "s",
        _ => "d",
    }
}

/// `decisionCreate`: `update_set` with one profile's decision replaced.
pub fn decision_create(value: &str, profile: u32, update_set: &str) -> String {
    let mut split: Vec<String> = update_set.chars().map(String::from).collect();
    let idx = usize::try_from(profile.saturating_sub(1)).unwrap_or(0);
    // Assigning past the end of a JavaScript array leaves holes, which `join`
    // writes as empty strings.
    if split.len() <= idx {
        split.resize(idx.saturating_add(1), String::new());
    }
    if let Some(slot) = split.get_mut(idx) {
        value.clone_into(slot);
    }
    split.concat()
}

/// `isOutdated`: a decided stat whose text no longer matches a stat.
pub fn is_outdated(d: &Data, profile: u32, stat: &MapCheckStat) -> bool {
    decision_has_color(&stat.decision, profile) && d.stat_by_match_str_v2(&stat.matcher).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    #[test]
    fn decisions_cycle_per_profile() {
        assert!(!decision_has_color("---", 1));
        assert!(decision_has_color("-d-", 2));
        assert!(!decision_has_color("--s", 3));
        assert_eq!(next_decision("d"), "w");
        assert_eq!(next_decision("s"), "d");
        assert_eq!(next_decision("-"), "d");
        assert_eq!(decision_create("w", 2, "---"), "-w-");
        assert_eq!(decision_create("g", 3, "d"), "dg");
    }

    #[test]
    fn outdated_stats_no_longer_match() {
        let d = en();
        let stat = |matcher: &str| MapCheckStat { matcher: matcher.into(), decision: "d--".into() };
        assert!(!is_outdated(d, 1, &stat("#% increased Monster Movement Speed")));
        assert!(is_outdated(d, 1, &stat("No such map stat #")));
        assert!(!is_outdated(d, 2, &stat("No such map stat #")));
    }
}
