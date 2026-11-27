//! `filters/pseudo/atzoatl-rules.ts`: which Chronicle of Atzoatl rooms are
//! worth searching for.

use super::super::{R, StatFilter, js_equal, missing};
use crate::parser::item::IncursionRoom;

const HIDDEN_GREEDY: &str = "filters.hide_greedy";

#[derive(Clone, Copy)]
struct FilterRule {
    stat_ref: &'static str,
    disabled: Option<bool>,
    remove: bool,
    hidden: Option<&'static str>,
    explosives_room: bool,
}

const fn keep(stat_ref: &'static str) -> FilterRule {
    FilterRule { stat_ref, disabled: None, remove: false, hidden: None, explosives_room: false }
}

const fn remove(stat_ref: &'static str) -> FilterRule {
    FilterRule { remove: true, ..keep(stat_ref) }
}

const fn explosives(stat_ref: &'static str) -> FilterRule {
    FilterRule { remove: true, explosives_room: true, ..keep(stat_ref) }
}

const fn enabled(stat_ref: &'static str) -> FilterRule {
    FilterRule { disabled: Some(false), ..keep(stat_ref) }
}

const fn greedy(stat_ref: &'static str) -> FilterRule {
    FilterRule { hidden: Some(HIDDEN_GREEDY), ..keep(stat_ref) }
}

const RULES: [FilterRule; 84] = [
    remove("Has Room: Banquet Hall"),
    remove("Has Room: Antechamber"),
    remove("Has Room: Passageways"),
    remove("Has Room: Cloister"),
    remove("Has Room: Tunnels"),
    remove("Has Room: Cellar"),
    remove("Has Room: Chasm"),
    remove("Has Room: Halls"),
    remove("Has Room: Tombs"),
    remove("Has Room: Pits"),
    remove("Has Room: Shrine of Empowerment (Tier 1)"),
    remove("Has Room: Sanctum of Unity (Tier 2)"),
    remove("Has Room: Temple Nexus (Tier 3)"),
    explosives("Has Room: Explosives Room (Tier 1)"),
    explosives("Has Room: Demolition Lab (Tier 2)"),
    explosives("Has Room: Shrine of Unmaking (Tier 3)"),
    // TierApex
    enabled("Has Room: Apex of Atzoatl"),
    enabled("Has Room: Cradle of Creation (Tier 3)"),
    // Tier0 (corrupt items)
    remove("Has Room: Corruption Chamber (Tier 1)"),
    remove("Has Room: Catalyst of Corruption (Tier 2)"),
    enabled("Has Room: Locus of Corruption (Tier 3)"),
    // Tier0 (corrupt gems)
    remove("Has Room: Gemcutter's Workshop (Tier 1)"),
    remove("Has Room: Department of Thaumaturgy (Tier 2)"),
    enabled("Has Room: Doryani's Institute (Tier 3)"),
    // Tier0 (sacrifice a unique item)
    remove("Has Room: Sacrificial Chamber (Tier 1)"),
    remove("Has Room: Hall of Offerings (Tier 2)"),
    enabled("Has Room: Apex of Ascension (Tier 3)"),
    // Tier1 (chests full of currency)
    greedy("Has Room: Vault (Tier 1)"),
    keep("Has Room: Treasury (Tier 2)"),
    enabled("Has Room: Wealth of the Vaal (Tier 3)"),
    // Tier1 (corrupt maps)
    greedy("Has Room: Surveyor's Study (Tier 1)"),
    keep("Has Room: Office of Cartography (Tier 2)"),
    enabled("Has Room: Atlas of Worlds (Tier 3)"),
    // Tier1 (Timeless Monolith)
    greedy("Has Room: Hall of Mettle (Tier 1)"),
    keep("Has Room: Hall of Heroes (Tier 2)"),
    keep("Has Room: Hall of Legends (Tier 3)"),
    // Tier2 (chests full of items)
    remove("Has Room: Storage Room (Tier 1)"),
    greedy("Has Room: Warehouses (Tier 2)"),
    keep("Has Room: Museum of Artefacts (Tier 3)"),
    // Tier2 (monster pack size)
    remove("Has Room: Guardhouse (Tier 1)"),
    greedy("Has Room: Barracks (Tier 2)"),
    keep("Has Room: Hall of War (Tier 3)"),
    // Tier2 (item yield of the Temple)
    remove("Has Room: Workshop (Tier 1)"),
    greedy("Has Room: Engineering Department (Tier 2)"),
    keep("Has Room: Factory (Tier 3)"),
    // Tier2 (Tormented Spirits)
    remove("Has Room: Torment Cells (Tier 1)"),
    greedy("Has Room: Torture Cages (Tier 2)"),
    keep("Has Room: Sadist's Den (Tier 3)"),
    // Tier2 (Contains Strongboxes)
    remove("Has Room: Strongbox Chamber (Tier 1)"),
    greedy("Has Room: Hall of Locks (Tier 2)"),
    keep("Has Room: Court of Sealed Death (Tier 3)"),
    // Tier2 (rarity of items dropped)
    remove("Has Room: Jeweller's Workshop (Tier 1)"),
    greedy("Has Room: Jewellery Forge (Tier 2)"),
    keep("Has Room: Glittering Halls (Tier 3)"),
    // Tier2 (Unstable Breaches)
    remove("Has Room: Anomaly Research Lab (Tier 1)"),
    greedy("Has Room: Breach Containment Chamber (Tier 2)"),
    keep("Has Room: House of the Others (Tier 3)"),
    // Tier3
    remove("Has Room: Tempest Generator (Tier 1)"),
    remove("Has Room: Hurricane Engine (Tier 2)"),
    greedy("Has Room: Storm of Corruption (Tier 3)"),
    // Tier3
    remove("Has Room: Armourer's Workshop (Tier 1)"),
    remove("Has Room: Armoury (Tier 2)"),
    greedy("Has Room: Chamber of Iron (Tier 3)"),
    // Tier3
    remove("Has Room: Sparring Room (Tier 1)"),
    remove("Has Room: Arena of Valour (Tier 2)"),
    greedy("Has Room: Hall of Champions (Tier 3)"),
    // Tier3
    remove("Has Room: Poison Garden (Tier 1)"),
    remove("Has Room: Cultivar Chamber (Tier 2)"),
    greedy("Has Room: Toxic Grove (Tier 3)"),
    // Tier3
    remove("Has Room: Hatchery (Tier 1)"),
    remove("Has Room: Automaton Lab (Tier 2)"),
    greedy("Has Room: Hybridisation Chamber (Tier 3)"),
    // Tier3
    remove("Has Room: Trap Workshop (Tier 1)"),
    remove("Has Room: Temple Defense Workshop (Tier 2)"),
    greedy("Has Room: Defense Research Lab (Tier 3)"),
    // Tier3
    remove("Has Room: Flame Workshop (Tier 1)"),
    remove("Has Room: Royal Forge (Tier 2)"),
    greedy("Has Room: Crucible of Flame (Tier 3)"),
    // Tier3
    remove("Has Room: Lightning Workshop (Tier 1)"),
    remove("Has Room: Grand Reactor (Tier 2)"),
    greedy("Has Room: Conduit of Lightning (Tier 3)"),
    // Tier3
    remove("Has Room: Pools of Restoration (Tier 1)"),
    remove("Has Room: Sanctum of Vitality (Tier 2)"),
    greedy("Has Room: Sanctum of Immortality (Tier 3)"),
];

/// `filter.option!.value`
fn option_value(filter: &StatFilter) -> R<f64> {
    filter.option.as_ref().map(|o| o.value).ok_or_else(|| missing("option"))
}

/// `applyRules`
pub fn apply_rules(filters: &mut Vec<StatFilter>) -> R<()> {
    for filter in filters.iter_mut() {
        filter.disabled = true;
    }

    let open = IncursionRoom::Open.as_value();
    let mut has_explosives = false;
    'rules: for rule in RULES.iter().filter(|r| r.explosives_room) {
        for filter in filters.iter().filter(|f| f.stat_ref == rule.stat_ref) {
            if js_equal(option_value(filter)?, open) {
                has_explosives = true;
                break 'rules;
            }
        }
    }

    for rule in &RULES {
        let Some(index) = filters.iter().position(|f| f.stat_ref == rule.stat_ref) else { continue };

        let Some(obstructed_filter) = filters.get(index) else { continue };
        let is_obstructed = js_equal(option_value(obstructed_filter)?, IncursionRoom::Obstructed.as_value());
        if rule.remove || (is_obstructed && !has_explosives) {
            filters.remove(index);
            continue;
        }

        let Some(filter) = filters.get_mut(index) else { continue };
        if let Some(hidden) = rule.hidden {
            filter.hidden = Some(hidden);
        }
        if let Some(disabled) = rule.disabled {
            filter.disabled = disabled;
        }
        if is_obstructed {
            // Search both Obstructed and Open rooms.
            filter.option = None;
        }
    }
    Ok(())
}

/// Every ref this module passes to `stat()`.
pub fn stat_refs() -> Vec<&'static str> {
    RULES.iter().map(|r| r.stat_ref).collect()
}
