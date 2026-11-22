//! `ref/apt/renderer/src/parser/meta.ts`: item categories.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

macro_rules! categories {
    ($($variant:ident = $text:literal,)*) => {
        /// `ItemCategory`. The data can name categories the enum lacks (it has
        /// `Unique Fragment`); JavaScript passes those strings through, hence
        /// [`ItemCategory::Other`].
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
    OneHandedAxe = "One-Handed Axe",
    TwoHandedAxe = "Two-Handed Axe",
    OneHandedMace = "One-Handed Mace",
    TwoHandedMace = "Two-Handed Mace",
    OneHandedSword = "One-Handed Sword",
    TwoHandedSword = "Two-Handed Sword",
    ClusterJewel = "Cluster Jewel",
    HeistBlueprint = "Heist Blueprint",
    HeistContract = "Heist Contract",
    HeistTool = "Heist Tool",
    HeistBrooch = "Heist Brooch",
    HeistGear = "Heist Gear",
    HeistCloak = "Heist Cloak",
    Trinket = "Trinket",
    Invitation = "Invitation",
    ExpeditionLogbook = "Expedition Logbook",
    Gem = "Gem",
    Currency = "Currency",
    DivinationCard = "Divination Card",
    Voidstone = "Voidstone",
    Sentinel = "Sentinel",
    MemoryLine = "Memory Line",
    SanctumRelic = "Sanctum Relic",
    Tincture = "Tincture",
    Charm = "Charm",
    Idol = "Idol",
    Graft = "Graft",
    Chart = "Chart",
    MercenaryWarrant = "Mercenary Warrant",
    UpperLimbViscera = "Upper Limb Viscera",
    LowerLimbViscera = "Lower Limb Viscera",
    HeadViscera = "Head Viscera",
    TorsoViscera = "Torso Viscera",
}

impl Serialize for ItemCategory {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ItemCategory {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Self::from_text(&String::deserialize(d)?))
    }
}

use ItemCategory as C;

pub const WEAPON_ONE_HANDED_MELEE: &[ItemCategory] =
    &[C::OneHandedAxe, C::OneHandedMace, C::OneHandedSword, C::Claw, C::Dagger, C::RuneDagger, C::Sceptre];

pub const WEAPON_ONE_HANDED: &[ItemCategory] =
    &[C::Wand, C::OneHandedAxe, C::OneHandedMace, C::OneHandedSword, C::Claw, C::Dagger, C::RuneDagger, C::Sceptre];

pub const WEAPONE_TWO_HANDED_MELEE: &[ItemCategory] =
    &[C::TwoHandedAxe, C::TwoHandedMace, C::TwoHandedSword, C::Staff, C::Warstaff];

pub const WEAPON: &[ItemCategory] = &[
    C::FishingRod,
    C::Bow,
    C::Wand,
    C::OneHandedAxe,
    C::OneHandedMace,
    C::OneHandedSword,
    C::Claw,
    C::Dagger,
    C::RuneDagger,
    C::Sceptre,
    C::TwoHandedAxe,
    C::TwoHandedMace,
    C::TwoHandedSword,
    C::Staff,
    C::Warstaff,
];

pub const ARMOUR: &[ItemCategory] = &[C::BodyArmour, C::Boots, C::Gloves, C::Helmet, C::Shield];

pub const JEWELLERY: &[ItemCategory] = &[C::Amulet, C::Belt, C::Ring];

pub const HEIST_EQUIPMENT: &[ItemCategory] = &[C::HeistTool, C::HeistBrooch, C::HeistGear, C::HeistCloak];

pub const MAGIC_ONLY_OR_UNIQUE_ITEM: &[ItemCategory] =
    &[C::Flask, C::Tincture, C::Voidstone, C::SanctumRelic, C::Charm];

pub const CONSUMABLE_CRAFTABLE_ITEM: &[ItemCategory] =
    &[C::Map, C::HeistBlueprint, C::HeistContract, C::Invitation, C::MemoryLine, C::ExpeditionLogbook, C::Chart];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_round_trip_their_text() {
        assert_eq!(ItemCategory::from_text("One-Handed Axe"), C::OneHandedAxe);
        assert_eq!(C::BodyArmour.as_str(), "Body Armour");
        let other = ItemCategory::from_text("Unique Fragment");
        assert_eq!(serde_json::to_string(&other).unwrap(), r#""Unique Fragment""#);
        assert_eq!(WEAPON.len(), 15);
        assert!(WEAPON_ONE_HANDED.iter().all(|c| WEAPON.contains(c)));
        assert!(WEAPONE_TWO_HANDED_MELEE.iter().all(|c| WEAPON.contains(c)));
        assert!(WEAPON_ONE_HANDED_MELEE.iter().all(|c| WEAPON_ONE_HANDED.contains(c)));
    }
}
