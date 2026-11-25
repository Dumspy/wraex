//! Inscribed Ultimatum items, which upstream has no parser for: the challenge,
//! the sacrifice and reward, and the modifiers the encounter rolls.
//!
//! Works from the parsed [`Item`] and its `rawText`, so [`Item`] stays exactly
//! what the reference's `parseClipboard` produces.

use serde::Serialize;

use crate::data::{Data, Namespace};
use crate::parser::item::Item;
use crate::prices::{DbQuery, Prices};

/// One Inscribed Ultimatum.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ultimatum {
    pub challenge: Challenge,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub area_level: Option<u32>,
    pub sacrifice: Sacrifice,
    pub reward: Reward,
    pub modifiers: Vec<UltimatumModifier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monster_damage: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monster_life: Option<u32>,
}

/// The encounter's objective.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Challenge {
    Exterminate,
    Survival,
    Defense,
    Conquer,
    Unknown(String),
}

impl Challenge {
    /// The trade site's `ultimatum_challenge` option id.
    pub fn trade_id(&self) -> Option<&'static str> {
        match self {
            Challenge::Exterminate => Some("Exterminate"),
            Challenge::Survival => Some("Survival"),
            Challenge::Defense => Some("Defense"),
            Challenge::Conquer => Some("Conquer"),
            Challenge::Unknown(_) => None,
        }
    }

    /// The challenge's own text, as the item writes it (`Challenge: Survive`).
    pub fn label(&self) -> &str {
        match self {
            Challenge::Exterminate => "Defeat waves of enemies",
            Challenge::Survival => "Survive",
            Challenge::Defense => "Protect the Altar",
            Challenge::Conquer => "Stand in the Stone Circles",
            Challenge::Unknown(text) => text,
        }
    }
}

/// What the item demands to start the encounter.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sacrifice {
    pub name: String,
    /// `None` for a unique, which has no stack count.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// What winning the encounter pays out.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum Reward {
    DoubleCurrency,
    DoubleDivCards,
    MirrorRare,
    ExchangeUnique(String),
    Unknown(String),
}

impl Reward {
    /// The trade site's `ultimatum_reward` option id.
    pub fn trade_id(&self) -> Option<&'static str> {
        match self {
            Reward::DoubleCurrency => Some("DoubleCurrency"),
            Reward::DoubleDivCards => Some("DoubleDivCards"),
            Reward::MirrorRare => Some("MirrorRare"),
            Reward::ExchangeUnique(_) => Some("ExchangeUnique"),
            Reward::Unknown(_) => None,
        }
    }

    /// The short name a filter chip shows (`Reward: Currency`); the trade site's wording.
    pub fn label(&self) -> &str {
        match self {
            Reward::DoubleCurrency => "Currency",
            Reward::DoubleDivCards => "Divination Cards",
            Reward::MirrorRare => "Mirrored Rare Item",
            Reward::ExchangeUnique(_) => "Unique Item",
            Reward::Unknown(text) => text,
        }
    }

    /// The reward as the item writes it (`Doubles sacrificed Currency`, or the unique's name).
    pub fn description(&self) -> &str {
        match self {
            Reward::DoubleCurrency => "Doubles sacrificed Currency",
            Reward::DoubleDivCards => "Doubles sacrificed Divination Cards",
            Reward::MirrorRare => "Mirrored Rare Item",
            Reward::ExchangeUnique(name) | Reward::Unknown(name) => name,
        }
    }
}

/// One rolled modifier and its explanation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UltimatumModifier {
    pub name: String,
    /// `1` when the name carries no numeral.
    pub tier: u8,
    pub description: String,
}

/// What an Inscribed Ultimatum is worth in chaos, both sides of the deal. The sacrifice is the
/// whole stack the item demands, not one unit; `None` where poe.ninja has no price for it.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct UltimatumValue {
    pub sacrifice: Option<f64>,
    pub reward: Option<f64>,
}

/// `value`: the sacrifice and the reward in chaos, from poe.ninja. Doubling rewards are worth
/// twice the sacrifice; a unique swap is the two uniques' prices, each looked up on its own.
pub fn value(u: &Ultimatum, d: &Data, prices: &Prices) -> UltimatumValue {
    // The sacrifice is a divination card when the reward doubles cards, a unique when it has
    // no stack count, and otherwise currency (or something else poe.ninja lists under `ITEM`).
    let sacrifice = match (&u.reward, u.sacrifice.count) {
        (Reward::DoubleDivCards, Some(count)) => {
            unit_price(prices, Namespace::DivinationCard, &u.sacrifice.name).map(|unit| unit * f64::from(count))
        }
        (_, Some(count)) => unit_price(prices, Namespace::Item, &u.sacrifice.name).map(|unit| unit * f64::from(count)),
        (_, None) => unique_price(d, prices, &u.sacrifice.name),
    };
    let reward = match &u.reward {
        Reward::DoubleCurrency | Reward::DoubleDivCards => sacrifice.map(|sacrifice| sacrifice * 2.0),
        Reward::ExchangeUnique(name) => unique_price(d, prices, name),
        Reward::MirrorRare | Reward::Unknown(_) => None,
    };
    UltimatumValue { sacrifice, reward }
}

/// One poe.ninja entry's chaos value.
fn unit_price(prices: &Prices, ns: Namespace, name: &str) -> Option<f64> {
    prices.find_price(&DbQuery { ns: ns.as_str(), name, variant: None }).map(|price| price.info.chaos)
}

/// A unique's chaos value. poe.ninja lists uniques by name and a variant (mostly the base
/// type), so try the base, then whichever entry carries the name.
fn unique_price(d: &Data, prices: &Prices, name: &str) -> Option<f64> {
    let ns = Namespace::Unique.as_str();
    let base = d
        .item_by_ref(Namespace::Unique, name)
        .and_then(|items| items.into_iter().find_map(|i| i.unique))
        .map(|u| u.base);
    base.and_then(|base| prices.find_price(&DbQuery { ns, name, variant: Some(&base) }))
        .or_else(|| prices.find_price_any_variant(ns, name))
        .map(|price| price.info.chaos)
}

/// `item.info.refName === "Inscribed Ultimatum"`.
pub fn is_ultimatum(item: &Item) -> bool {
    item.info.as_ref().is_some_and(|info| info.ref_name == "Inscribed Ultimatum")
}

/// Parse an Inscribed Ultimatum's sections.
pub fn parse(item: &Item, d: &Data) -> Option<Ultimatum> {
    if !is_ultimatum(item) {
        return None;
    }
    let sections: Vec<Vec<&str>> = {
        let mut sections = vec![Vec::new()];
        for line in item.raw_text.split('\n').map(str::trim_end) {
            if line == "--------" {
                sections.push(Vec::new());
            } else {
                sections.last_mut().unwrap().push(line);
            }
        }
        sections
    };

    let mut challenge = Challenge::Unknown(String::new());
    let mut area_level = None;
    let mut sacrifice = Sacrifice { name: String::new(), count: None };
    let mut reward_text = String::new();
    for line in sections.get(1).map(Vec::as_slice).unwrap_or(&[]) {
        if let Some(text) = line.trim().strip_prefix("Challenge:") {
            challenge = challenge_from(text.trim());
        } else if let Some(text) = line.trim().strip_prefix("Area Level:") {
            area_level = text.trim().parse().ok();
        } else if let Some(text) = line.trim().strip_prefix("Requires Sacrifice:") {
            sacrifice = sacrifice_from(text.trim());
        } else if let Some(text) = line.trim().strip_prefix("Reward:") {
            reward_text = text.trim().to_owned();
        }
    }

    let (modifiers, monster_damage, monster_life) = modifiers_from(sections.get(2).map(Vec::as_slice).unwrap_or(&[]));
    let reward = reward_from(&reward_text, &sacrifice, d);
    Some(Ultimatum { challenge, area_level, sacrifice, reward, modifiers, monster_damage, monster_life })
}

fn challenge_from(text: &str) -> Challenge {
    if text.eq_ignore_ascii_case("Defeat waves of enemies") {
        Challenge::Exterminate
    } else if text.eq_ignore_ascii_case("Survive") {
        Challenge::Survival
    } else if text.eq_ignore_ascii_case("Protect the Altar") {
        Challenge::Defense
    } else if text.eq_ignore_ascii_case("Stand in the Stone Circles") {
        Challenge::Conquer
    } else {
        Challenge::Unknown(text.to_owned())
    }
}

/// `"Vaal Orb x20"` is a name and a count; a unique's name has neither.
fn sacrifice_from(text: &str) -> Sacrifice {
    if let Some((name, count)) = text.rsplit_once(" x")
        && let Ok(count) = count.trim().parse::<u32>()
    {
        return Sacrifice { name: name.trim().to_owned(), count: Some(count) };
    }
    Sacrifice { name: text.to_owned(), count: None }
}

fn reward_from(text: &str, sacrifice: &Sacrifice, d: &Data) -> Reward {
    match text {
        "Doubles sacrificed Currency" => Reward::DoubleCurrency,
        "Doubles sacrificed Divination Cards" => Reward::DoubleDivCards,
        "Mirrored Rare Item" => Reward::MirrorRare,
        _ if sacrifice.count.is_none() && d.item_by_ref(Namespace::Unique, text).is_some() => {
            Reward::ExchangeUnique(text.to_owned())
        }
        _ => Reward::Unknown(text.to_owned()),
    }
}

fn modifiers_from(lines: &[&str]) -> (Vec<UltimatumModifier>, Option<u32>, Option<u32>) {
    let mut modifiers = Vec::new();
    let mut monster_damage = None;
    let mut monster_life = None;
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if line.is_empty() {
            i += 1;
        } else if let Some(value) = percent_value(line, " increased Monster Damage") {
            monster_damage = Some(value);
            i += 1;
        } else if let Some(value) = percent_value(line, " more Monster Life") {
            monster_life = Some(value);
            i += 1;
        } else if line.starts_with('(') {
            // A description with no modifier before it.
            while i < lines.len() && !lines[i].ends_with(')') {
                i += 1;
            }
            i += 1;
        } else {
            let (name, tier) = split_tier(line);
            i += 1;
            let mut description = Vec::new();
            while i < lines.len() && lines[i].starts_with('(') {
                loop {
                    let l = lines[i];
                    description.push(l);
                    i += 1;
                    if l.ends_with(')') || i >= lines.len() {
                        break;
                    }
                }
            }
            modifiers.push(UltimatumModifier { name, tier, description: strip_parentheses(description.join("\n")) });
        }
    }
    (modifiers, monster_damage, monster_life)
}

/// `"30% more Monster Life"` with the suffix's leading space: `30`.
fn percent_value(line: &str, suffix: &str) -> Option<u32> {
    line.strip_suffix(suffix)?.strip_suffix('%')?.parse().ok()
}

/// `"Razor Dance IV"` is tier 4; a last word that is not `II`, `III` or `IV` is
/// part of the name.
fn split_tier(line: &str) -> (String, u8) {
    let Some((name, word)) = line.rsplit_once(' ') else { return (line.to_owned(), 1) };
    let tier = match word {
        "II" => 2,
        "III" => 3,
        "IV" => 4,
        _ => return (line.to_owned(), 1),
    };
    (name.to_owned(), tier)
}

fn strip_parentheses(text: String) -> String {
    let text = text.strip_prefix('(').unwrap_or(&text).to_owned();
    text.strip_suffix(')').unwrap_or(&text).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;
    use crate::parser::parse_clipboard;

    fn parse_text(name: &str, text: &str) -> Option<Ultimatum> {
        let item = parse_clipboard(en(), text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
        parse(&item, en())
    }

    fn modifier(name: &str, tier: u8, description: &str) -> UltimatumModifier {
        UltimatumModifier { name: name.into(), tier, description: description.into() }
    }

    #[test]
    fn survive_currency() {
        let u = parse_text("survive-currency", include_str!("../testdata/ultimatum/survive-currency.txt")).unwrap();
        assert_eq!(
            u,
            Ultimatum {
                challenge: Challenge::Survival,
                area_level: Some(83),
                sacrifice: Sacrifice { name: "Vaal Orb".into(), count: Some(20) },
                reward: Reward::DoubleCurrency,
                modifiers: vec![
                    modifier(
                        "Stormcaller Runes",
                        2,
                        "Large runes will appear that will call deadly Lightning storms if you remain in them"
                    ),
                    modifier(
                        "Razor Dance",
                        1,
                        "Blades will periodically appear and fly at you, applying Corrupted Blood on Hit"
                    ),
                    modifier(
                        "Totem of Costly Potency",
                        1,
                        "An invulnerable Totem will appear that imbues both its allies and its enemies with Speed and powerful Elemental Damage benefits, but causes enemies to take 15% of their maximum Life and Energy Shield as Fire Damage per second"
                    ),
                    modifier(
                        "Unlucky Criticals",
                        1,
                        "Your Critical Strike Chance is Unlucky\nMonsters take 50% reduced Extra Damage from Critical Strikes"
                    ),
                    modifier(
                        "Siphoned Charges",
                        1,
                        "Lose a Power, Frenzy, and Endurance Charge every second\nMonsters gain Power, Frenzy, and Endurance Charges on hit"
                    ),
                    modifier("Deadly Monsters", 1, "Monster Hits are always Critical Strikes"),
                    modifier("Resistant Monsters", 1, "Monsters have +50% to all Resistances"),
                    modifier("Siphoning Monsters", 1, "Monster Hits remove 10% of current Energy Shield and Mana"),
                ],
                monster_damage: Some(10),
                monster_life: Some(30),
            }
        );
    }

    #[test]
    fn survive_unique_swap() {
        let u =
            parse_text("survive-unique-swap", include_str!("../testdata/ultimatum/survive-unique-swap.txt")).unwrap();
        assert_eq!(
            u,
            Ultimatum {
                challenge: Challenge::Survival,
                area_level: Some(83),
                sacrifice: Sacrifice { name: "Prospero's Protection".into(), count: None },
                reward: Reward::ExchangeUnique("Grasping Nightshade".into()),
                modifiers: vec![
                    modifier("Choking Miasma", 1, "A cloud that deals Chaos Damage over Time will follow you"),
                    modifier(
                        "Razor Dance",
                        4,
                        "Swarms of blades will periodically appear and fly at you, applying Corrupted Blood on Hit and leaving a trail of Blood ground"
                    ),
                    modifier(
                        "Totem of Costly Might",
                        1,
                        "An invulnerable Totem will appear that imbues both its allies and its enemies with Speed and powerful Physical Damage benefits, but causes enemies to take 10% of their maximum Life and Energy Shield as Physical Damage per second"
                    ),
                    modifier(
                        "Escalating Monster Speed",
                        1,
                        "Monsters gain 5% increased attack, cast, and movement speed per second they are alive, up to 50%"
                    ),
                    modifier("Treacherous Auras", 1, "Your Auras that affect Allies also affect Enemies"),
                    modifier("Resistant Monsters", 1, "Monsters have +50% to all Resistances"),
                ],
                monster_damage: Some(10),
                monster_life: Some(30),
            }
        );
    }

    #[test]
    fn waves_div_cards() {
        let u = parse_text("waves-div-cards", include_str!("../testdata/ultimatum/waves-div-cards.txt")).unwrap();
        assert_eq!(
            u,
            Ultimatum {
                challenge: Challenge::Exterminate,
                area_level: Some(83),
                sacrifice: Sacrifice { name: "Monochrome".into(), count: Some(2) },
                reward: Reward::DoubleDivCards,
                modifiers: vec![
                    modifier(
                        "Stormcaller Runes",
                        2,
                        "Large runes will appear that will call deadly Lightning storms if you remain in them"
                    ),
                    modifier(
                        "Blistering Cold",
                        4,
                        "Many frozen pustules will periodically appear, chill you, and rapidly explode when you approach, freezing if they hit"
                    ),
                    modifier("Less Cooldown Recovery", 1, "Cooldowns take five times longer to recharge"),
                    modifier(
                        "Escalating Monster Speed",
                        1,
                        "Monsters gain 5% increased attack, cast, and movement speed per second they are alive, up to 50%"
                    ),
                    modifier("Hindering Flasks", 1, "Using a Flask Hinders you by 50% for 4 seconds"),
                    modifier("Lethal Rare Monsters", 1, "Rare Monsters have 2 additional Modifiers"),
                    modifier("Deadly Monsters", 1, "Monster Hits are always Critical Strikes"),
                ],
                monster_damage: Some(20),
                monster_life: Some(70),
            }
        );
    }

    #[test]
    fn altar_currency() {
        let u = parse_text("altar-currency", include_str!("../testdata/ultimatum/altar-currency.txt")).unwrap();
        assert_eq!(
            u,
            Ultimatum {
                challenge: Challenge::Defense,
                area_level: Some(83),
                sacrifice: Sacrifice { name: "Divine Orb".into(), count: Some(1) },
                reward: Reward::DoubleCurrency,
                modifiers: vec![
                    modifier(
                        "Stalking Ruin",
                        1,
                        "An invulnerable shade stalks you, inflicting Ruin with its Melee Attacks\nFail on reaching 7 Ruin"
                    ),
                    modifier(
                        "Razor Dance",
                        3,
                        "Swarms of blades will periodically appear and fly at you, applying Corrupted Blood on Hit"
                    ),
                    modifier("Reduced Recovery", 1, "60% Reduced Life, Mana, and Energy Shield Recovery Rate"),
                    modifier(
                        "Escalating Damage Taken",
                        1,
                        "For the remainder of the encounter, damage taken will slowly increase by 2% per second, up to 50%"
                    ),
                    modifier("Lightning Damage from Mana Costs", 1, "Take 1000% of Mana costs as Lightning Damage"),
                    modifier("Deadly Monsters", 1, "Monster Hits are always Critical Strikes"),
                    modifier(
                        "Prismatic Monsters",
                        1,
                        "Monsters gain 50% of Physical Damage as Extra Damage of each Element"
                    ),
                ],
                monster_damage: Some(10),
                monster_life: Some(30),
            }
        );
    }

    #[test]
    fn altar_div_cards() {
        let u = parse_text("altar-div-cards", include_str!("../testdata/ultimatum/altar-div-cards.txt")).unwrap();
        assert_eq!(u.challenge, Challenge::Defense);
        assert_eq!(u.sacrifice, Sacrifice { name: "The Siren".into(), count: Some(4) });
        assert_eq!(u.reward, Reward::DoubleDivCards);
        assert_eq!(
            modifier_names(&u),
            [
                ("Choking Miasma", 3),
                ("Blistering Cold", 3),
                ("Reduced Recovery", 1),
                ("Lethal Rare Monsters", 1),
                ("Overwhelming Monsters", 1),
            ]
        );
    }

    #[test]
    fn stone_circles_currency() {
        let u = parse_text("stone-circles-currency", include_str!("../testdata/ultimatum/stone-circles-currency.txt"))
            .unwrap();
        assert_eq!(u.challenge, Challenge::Conquer);
        assert_eq!(u.sacrifice, Sacrifice { name: "Chaos Orb".into(), count: Some(20) });
        assert_eq!(u.reward, Reward::DoubleCurrency);
        assert_eq!(
            modifier_names(&u),
            [
                ("Choking Miasma", 3),
                ("Raging Dead", 1),
                ("Razor Dance", 1),
                ("Reduced Recovery", 1),
                ("Less Cooldown Recovery", 1),
                ("Escalating Damage Taken", 1),
                ("Overwhelming Monsters", 1),
            ]
        );
    }

    fn modifier_names(u: &Ultimatum) -> Vec<(&str, u8)> {
        u.modifiers.iter().map(|m| (m.name.as_str(), m.tier)).collect()
    }

    #[test]
    fn trade_ids() {
        assert_eq!(Challenge::Exterminate.trade_id(), Some("Exterminate"));
        assert_eq!(Challenge::Unknown("?".into()).trade_id(), None);
        assert_eq!(Reward::DoubleCurrency.trade_id(), Some("DoubleCurrency"));
        assert_eq!(Reward::DoubleDivCards.trade_id(), Some("DoubleDivCards"));
        assert_eq!(Reward::MirrorRare.trade_id(), Some("MirrorRare"));
        assert_eq!(Reward::ExchangeUnique("Grasping Nightshade".into()).trade_id(), Some("ExchangeUnique"));
        assert_eq!(Reward::Unknown("?".into()).trade_id(), None);
    }

    #[test]
    fn labels_are_the_items_own_words() {
        assert_eq!(Challenge::Survival.label(), "Survive");
        assert_eq!(Challenge::Unknown("Hold the line".into()).label(), "Hold the line");
        assert_eq!(Reward::DoubleCurrency.label(), "Currency");
        assert_eq!(Reward::DoubleDivCards.label(), "Divination Cards");
        assert_eq!(Reward::ExchangeUnique("Grasping Nightshade".into()).label(), "Unique Item");
    }

    /// A small poe.ninja blob: the units `value` looks up, in their overview types.
    const PRICES: &str = concat!(
        r#"{"overviews":[{"type":"Currency","lines":[{"name":"Chaos Orb","chaos":1,"graph":[]},"#,
        r#"{"name":"Vaal Orb","chaos":4.5,"graph":[]},{"name":"Divine Orb","chaos":200,"graph":[]}]},"#,
        r#"{"type":"DivinationCard","lines":[{"name":"Monochrome","chaos":100,"graph":[]}]},"#,
        r#"{"type":"UniqueArmour","lines":[{"name":"Prospero's Protection","variant":"Iron Ring","chaos":5,"graph":[]}]},"#,
        r#"{"type":"UniqueTincture","lines":[{"name":"Grasping Nightshade","variant":"Tincture","chaos":40,"graph":[]}]}]}"#
    );

    fn prices() -> Prices {
        let mut prices = Prices::default();
        prices.loaded(PRICES, 0);
        prices
    }

    #[test]
    fn value_prices_both_sides_of_the_deal() {
        let prices = prices();
        let value_of = |name: &str, text: &str| value(&parse_text(name, text).unwrap(), en(), &prices);

        // Twenty Vaal Orbs at 4.5 chaos double to 180.
        assert_eq!(
            value_of("survive-currency", include_str!("../testdata/ultimatum/survive-currency.txt")),
            UltimatumValue { sacrifice: Some(90.0), reward: Some(180.0) }
        );
        // Twenty Chaos Orbs, which poe.ninja prices at 1 chaos each.
        assert_eq!(
            value_of("stone-circles-currency", include_str!("../testdata/ultimatum/stone-circles-currency.txt")),
            UltimatumValue { sacrifice: Some(20.0), reward: Some(40.0) }
        );
        // Two divination cards price in the card overview, not the currency one.
        assert_eq!(
            value_of("waves-div-cards", include_str!("../testdata/ultimatum/waves-div-cards.txt")),
            UltimatumValue { sacrifice: Some(200.0), reward: Some(400.0) }
        );
        // A unique swap: poe.ninja lists uniques with a variant, found by base type (Iron Ring)
        // or, failing that, by name alone (Tincture).
        assert_eq!(
            value_of("survive-unique-swap", include_str!("../testdata/ultimatum/survive-unique-swap.txt")),
            UltimatumValue { sacrifice: Some(5.0), reward: Some(40.0) }
        );
    }

    #[test]
    fn an_unknown_challenge_or_reward_has_no_filter() {
        let u = Ultimatum {
            challenge: Challenge::Unknown("Hold the line".into()),
            area_level: None,
            sacrifice: Sacrifice { name: "Vaal Orb".into(), count: Some(1) },
            reward: Reward::Unknown("Something new".into()),
            modifiers: vec![],
            monster_damage: None,
            monster_life: None,
        };
        let filters = crate::filters::UltimatumFilters::from(&u);
        assert!(filters.challenge.is_none());
        assert!(filters.reward.is_none());
        assert!(filters.output.is_none());
        assert_eq!(filters.input.map(|filter| filter.value), Some("Vaal Orb".to_owned()));
    }

    #[test]
    fn value_prices_a_sacrifice_whose_reward_it_cannot() {
        let prices = prices();
        let u = Ultimatum {
            challenge: Challenge::Survival,
            area_level: None,
            sacrifice: Sacrifice { name: "Vaal Orb".into(), count: Some(2) },
            reward: Reward::MirrorRare,
            modifiers: vec![],
            monster_damage: None,
            monster_life: None,
        };
        assert_eq!(value(&u, en(), &prices), UltimatumValue { sacrifice: Some(9.0), reward: None });

        let u = Ultimatum { sacrifice: Sacrifice { name: "Not An Item".into(), count: Some(1) }, ..u };
        assert_eq!(value(&u, en(), &prices), UltimatumValue { sacrifice: None, reward: None });
    }

    #[test]
    fn other_items_are_not_ultimatums() {
        let text = "Item Class: Stackable Currency\nRarity: Currency\nChaos Orb\n--------\nStack Size: 1/20\n--------\nReforges a rare item with new random modifiers";
        let item = parse_clipboard(en(), text).unwrap();
        assert!(!is_ultimatum(&item));
        assert_eq!(parse(&item, en()), None);
    }
}
