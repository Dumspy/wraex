//! `ref/ee2/renderer/src/parser/meta.ts`: item categories.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! categories {
    ($($variant:ident = $text:literal,)*) => {
        /// `ItemCategory`. Data and trade lookups can produce categories the enum
        /// lacks (`forceCraftable` uses the item name); JavaScript passes those
        /// strings through, hence [`ItemCategory::Other`].
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub enum ItemCategory {
            $($variant,)*
            Other(String),
        }

        impl ItemCategory {
            pub fn as_str(&self) -> &str {
                match self {
                    $(ItemCategory::$variant => $text,)*
                    ItemCategory::Other(s) => s,
                }
            }

            pub fn from_text(text: &str) -> Self {
                match text {
                    $($text => ItemCategory::$variant,)*
                    other => ItemCategory::Other(other.to_owned()),
                }
            }
        }
    };
}

categories! {
    Unknown = "Unknown",
    Map = "Map",
    CapturedBeast = "Captured Beast",
    MetamorphSample = "Metamorph Sample",
    Helmet = "Helmet",
    BodyArmour = "Body Armour",
    Gloves = "Gloves",
    Boots = "Boots",
    Shield = "Shield",
    Amulet = "Amulet",
    Belt = "Belt",
    Ring = "Ring",
    Flask = "Flask",
    AbyssJewel = "Abyss Jewel",
    Jewel = "Jewel",
    Quiver = "Quiver",
    Claw = "Claw",
    Bow = "Bow",
    Sceptre = "Sceptre",
    Wand = "Wand",
    FishingRod = "Fishing Rod",
    Staff = "Staff",
    Warstaff = "Warstaff",
    Dagger = "Dagger",
    RuneDagger = "Rune Dagger",
    OneHandedAxe = "One Hand Axe",
    TwoHandedAxe = "Two Hand Axe",
    OneHandedMace = "One Hand Mace",
    TwoHandedMace = "Two Hand Mace",
    OneHandedSword = "One Hand Sword",
    TwoHandedSword = "Two Hand Sword",
    ClusterJewel = "Cluster Jewel",
    HeistBlueprint = "Heist Blueprint",
    HeistContract = "Heist Contract",
    HeistTool = "Heist Tool",
    HeistBrooch = "Heist Brooch",
    HeistGear = "Heist Gear",
    HeistCloak = "Heist Cloak",
    Trinket = "Trinket",
    Invitation = "Invitation",
    Gem = "Gem",
    Currency = "Currency",
    DivinationCard = "Divination Card",
    Voidstone = "Voidstone",
    Sentinel = "Sentinel",
    MemoryLine = "Memory Line",
    SanctumRelic = "Sanctum Relic",
    Tincture = "Tincture",
    Charm = "Charm",
    Crossbow = "Crossbow",
    SkillGem = "Skill Gem",
    SupportGem = "Support Gem",
    MetaGem = "Meta Gem",
    UncutGem = "UncutSkillGem",
    Focus = "Focus",
    Waystone = "Waystone",
    Relic = "Relic",
    Tablet = "TowerAugment",
    Spear = "Spear",
    Flail = "Flail",
    Buckler = "Buckler",
    MapFragment = "MapFragment",
    Talisman = "Talisman",
    Augment = "Augment",
    Wombgift = "BrequelFruit",
}

impl Serialize for ItemCategory {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ItemCategory {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::from_text(&String::deserialize(deserializer)?))
    }
}

use ItemCategory as C;

pub const WEAPON_ONE_HANDED_MELEE: &[ItemCategory] =
    &[C::OneHandedAxe, C::OneHandedMace, C::OneHandedSword, C::Claw, C::Dagger, C::RuneDagger, C::Spear, C::Flail];

pub const WEAPON_TWO_HANDED_MELEE: &[ItemCategory] =
    &[C::TwoHandedAxe, C::TwoHandedMace, C::TwoHandedSword, C::Warstaff, C::Talisman];

/// `WEAPON_ONE_HANDED`
pub fn is_weapon_one_handed(c: &ItemCategory) -> bool {
    matches!(c, C::Sceptre | C::Wand) || WEAPON_ONE_HANDED_MELEE.contains(c)
}

/// `MARTIAL_WEAPON`
pub fn is_martial_weapon(c: &ItemCategory) -> bool {
    matches!(c, C::Bow | C::Crossbow) || WEAPON_ONE_HANDED_MELEE.contains(c) || WEAPON_TWO_HANDED_MELEE.contains(c)
}

/// `WEAPON`
pub fn is_weapon(c: &ItemCategory) -> bool {
    matches!(c, C::Staff | C::FishingRod) || is_weapon_one_handed(c) || is_martial_weapon(c)
}

pub const ARMOUR: &[ItemCategory] = &[C::BodyArmour, C::Boots, C::Gloves, C::Helmet, C::Shield, C::Focus, C::Buckler];

pub const ACCESSORY: &[ItemCategory] = &[C::Amulet, C::Belt, C::Ring, C::Trinket];

pub const GRANTS_REAL_SKILL: &[ItemCategory] = &[C::Staff, C::Wand, C::Sceptre];

/// `GRANTS_SKILL`
pub fn grants_skill(c: &ItemCategory) -> bool {
    GRANTS_REAL_SKILL.contains(c) || matches!(c, C::Spear | C::Shield | C::Buckler)
}

pub const GEM: &[ItemCategory] = &[C::Gem, C::MetaGem, C::SupportGem];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_round_trip_their_text() {
        assert_eq!(ItemCategory::from_text("One Hand Axe"), C::OneHandedAxe);
        assert_eq!(C::Tablet.as_str(), "TowerAugment");
        assert_eq!(ItemCategory::from_text("SoulCore"), C::Other("SoulCore".into()));
        assert!(is_martial_weapon(&C::Spear) && is_weapon(&C::Staff) && !is_martial_weapon(&C::Staff));
    }
}
