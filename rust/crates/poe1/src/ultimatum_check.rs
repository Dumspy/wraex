//! The Inscribed Ultimatum check: the modifiers a player refuses, per profile. Upstream has no
//! Ultimatum support, so the rules are the port's own; the decision characters and the profile
//! are [`crate::map_check`]'s, which is why both live in the item check widget's config.
//!
//! A modifier's tier is part of the game's roll: marking `Razor Dance III` as deadly also marks
//! its IV (the same modifier, rolled harder) and leaves I and II alone.

use serde::{Deserialize, Serialize};

use crate::map_check::decision_create;
use crate::ultimatum::Ultimatum;

/// The item check widget's `ultimatum` object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UltimatumCheckConfig {
    /// Draw the eye column for modifiers the profile has not seen yet.
    pub show_new_mods: bool,
    /// One entry per modifier the player has marked or seen.
    pub selected_mods: Vec<UltimatumModDecision>,
}

/// One modifier's decisions, one map-check decision set per tier (`I`..`IV`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UltimatumModDecision {
    /// The modifier's name without its tier numeral (`Razor Dance`).
    pub name: String,
    pub tiers: [String; 4],
}

impl Default for UltimatumModDecision {
    fn default() -> Self {
        Self { name: String::new(), tiers: std::array::from_fn(|_| UNTOUCHED.to_owned()) }
    }
}

/// A decision set with no profile decided, the same three characters the map check writes.
const UNTOUCHED: &str = "---";

/// `tier` (1..=4) as an index into [`UltimatumModDecision::tiers`].
fn tier_index(tier: u8) -> Option<usize> {
    tier.checked_sub(1).filter(|index| *index < 4).map(usize::from)
}

/// The decision of `profile` for `name` at `tier`: `-` undecided, `d` deadly, `w` warning,
/// `g` good, `s` seen. `None` when the modifier has no entry or the profile is past its set.
pub fn decision(config: &UltimatumCheckConfig, profile: u32, name: &str, tier: u8) -> Option<char> {
    let entry = config.selected_mods.iter().find(|entry| entry.name == name)?;
    entry.tiers.get(tier_index(tier)?)?.chars().nth(profile.checked_sub(1)? as usize)
}

/// The entry for `name`, created on first use: there is no static list of Ultimatum modifiers.
fn entry_mut<'a>(config: &'a mut UltimatumCheckConfig, name: &str) -> &'a mut UltimatumModDecision {
    let index = match config.selected_mods.iter().position(|entry| entry.name == name) {
        Some(index) => index,
        None => {
            config.selected_mods.push(UltimatumModDecision { name: name.to_owned(), ..Default::default() });
            config.selected_mods.len() - 1
        }
    };
    &mut config.selected_mods[index]
}

/// One tier's decision, which the settings page sets: every tier stands on its own there.
pub fn set_tier_decision(config: &mut UltimatumCheckConfig, profile: u32, name: &str, tier: u8, value: &str) {
    let Some(index) = tier_index(tier) else { return };
    let entry = entry_mut(config, name);
    entry.tiers[index] = decision_create(value, profile, &entry.tiers[index]);
}

/// A decision from the item check: marking a tier marks it and every higher one for `profile`,
/// leaving the lower tiers as they were.
pub fn set_decision(config: &mut UltimatumCheckConfig, profile: u32, name: &str, tier: u8, value: &str) {
    let Some(index) = tier_index(tier) else { return };
    let entry = entry_mut(config, name);
    for set in &mut entry.tiers[index..] {
        *set = decision_create(value, profile, set);
    }
}

/// Forget a modifier, as the settings page's Remove does.
pub fn remove(config: &mut UltimatumCheckConfig, name: &str) {
    config.selected_mods.retain(|entry| entry.name != name);
}

/// The header badge the item check shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Deadly,
    Warning,
    Fine,
}

/// A deadly modifier makes the encounter a skip, a warning makes it a careful run, and an
/// unmarked one is fine to attempt.
pub fn verdict(config: &UltimatumCheckConfig, profile: u32, ultimatum: &Ultimatum) -> Verdict {
    let mut verdict = Verdict::Fine;
    for modifier in &ultimatum.modifiers {
        match decision(config, profile, &modifier.name, modifier.tier) {
            Some('d') => return Verdict::Deadly,
            Some('w') => verdict = Verdict::Warning,
            _ => {}
        }
    }
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;
    use crate::parser::parse_clipboard;

    fn survive() -> Ultimatum {
        parse_test("survive-currency", include_str!("../testdata/ultimatum/survive-currency.txt"))
    }

    fn altar() -> Ultimatum {
        parse_test("altar-currency", include_str!("../testdata/ultimatum/altar-currency.txt"))
    }

    fn parse_test(name: &str, text: &str) -> Ultimatum {
        let item = parse_clipboard(en(), text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        crate::ultimatum::parse(&item, en()).unwrap_or_else(|| panic!("{name}: not an ultimatum"))
    }

    #[test]
    fn marking_a_tier_marks_the_higher_ones() {
        let mut config = UltimatumCheckConfig::default();
        set_decision(&mut config, 1, "Razor Dance", 1, "d");
        for tier in 1..=4 {
            assert_eq!(decision(&config, 1, "Razor Dance", tier), Some('d'), "tier {tier}");
        }

        // A tier-III mark leaves I and II alone.
        let mut config = UltimatumCheckConfig::default();
        set_decision(&mut config, 1, "Razor Dance", 3, "w");
        assert_eq!(decision(&config, 1, "Razor Dance", 1), Some('-'));
        assert_eq!(decision(&config, 1, "Razor Dance", 2), Some('-'));
        assert_eq!(decision(&config, 1, "Razor Dance", 3), Some('w'));
        assert_eq!(decision(&config, 1, "Razor Dance", 4), Some('w'));

        // The settings page's chips are independent.
        let mut config = UltimatumCheckConfig::default();
        set_tier_decision(&mut config, 1, "Razor Dance", 2, "g");
        assert_eq!(decision(&config, 1, "Razor Dance", 1), Some('-'));
        assert_eq!(decision(&config, 1, "Razor Dance", 2), Some('g'));
        assert_eq!(decision(&config, 1, "Razor Dance", 3), Some('-'));
    }

    #[test]
    fn profiles_decide_on_their_own() {
        let mut config = UltimatumCheckConfig::default();
        set_decision(&mut config, 1, "Unlucky Criticals", 1, "d");
        assert_eq!(decision(&config, 1, "Unlucky Criticals", 1), Some('d'));
        assert_eq!(decision(&config, 2, "Unlucky Criticals", 1), Some('-'));

        set_decision(&mut config, 2, "Unlucky Criticals", 1, "g");
        assert_eq!(decision(&config, 1, "Unlucky Criticals", 1), Some('d'));
        assert_eq!(decision(&config, 2, "Unlucky Criticals", 1), Some('g'));
    }

    #[test]
    fn the_verdict_reads_the_items_own_tiers() {
        let survive = survive();
        let mut config = UltimatumCheckConfig::default();
        assert_eq!(verdict(&config, 1, &survive), Verdict::Fine);

        // `Unlucky Criticals` is tier I on this Ultimatum.
        set_decision(&mut config, 1, "Unlucky Criticals", 1, "w");
        assert_eq!(verdict(&config, 1, &survive), Verdict::Warning);
        assert_eq!(verdict(&config, 2, &survive), Verdict::Fine);

        // `Stormcaller Runes` is tier II: a mark on II and up is deadly.
        set_decision(&mut config, 1, "Stormcaller Runes", 2, "d");
        assert_eq!(verdict(&config, 1, &survive), Verdict::Deadly);

        // A mark on III is not a mark on the II this item rolled.
        let mut config = UltimatumCheckConfig::default();
        set_decision(&mut config, 1, "Stormcaller Runes", 3, "d");
        assert_eq!(verdict(&config, 1, &survive), Verdict::Fine, "the item rolled tier II");
        set_decision(&mut config, 1, "Stormcaller Runes", 2, "d");
        assert_eq!(verdict(&config, 1, &survive), Verdict::Deadly);

        // `Razor Dance` is tier III on the altar Ultimatum.
        let mut config = UltimatumCheckConfig::default();
        set_decision(&mut config, 1, "Razor Dance", 1, "d");
        assert_eq!(verdict(&config, 1, &altar()), Verdict::Deadly, "tier I carries up to III");
    }

    #[test]
    fn removing_a_modifier_forgets_it() {
        let mut config = UltimatumCheckConfig::default();
        set_decision(&mut config, 1, "Razor Dance", 1, "d");
        remove(&mut config, "Razor Dance");
        assert!(config.selected_mods.is_empty());
        assert_eq!(decision(&config, 1, "Razor Dance", 1), None);
    }

    #[test]
    fn a_tier_outside_the_card_is_ignored() {
        let mut config = UltimatumCheckConfig::default();
        set_decision(&mut config, 1, "Razor Dance", 0, "d");
        set_decision(&mut config, 1, "Razor Dance", 5, "d");
        assert!(config.selected_mods.is_empty());
        assert_eq!(decision(&config, 1, "Razor Dance", 0), None);
        assert_eq!(decision(&config, 1, "Razor Dance", 5), None);
    }
}
