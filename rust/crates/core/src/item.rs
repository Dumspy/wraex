use serde::{Deserialize, Serialize};

/// How rare an item is, as written on the item's header line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

impl Rarity {
    /// Parses the `Rarity: Rare` header line found in copied item text.
    pub fn from_header(header: &str) -> Option<Self> {
        let value = header.strip_prefix("Rarity:")?.trim();
        match value {
            "Normal" => Some(Self::Normal),
            "Magic" => Some(Self::Magic),
            "Rare" => Some(Self::Rare),
            "Unique" => Some(Self::Unique),
            _ => None,
        }
    }
}

/// A parsed clipboard item.
///
/// Deliberately small: these are the fields both reference parsers agree on. The
/// rest of the model (upstream's `ParsedItem`, EE2's augmented one) arrives with
/// the modules that produce it, driven by the fixtures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    /// Name line, e.g. `Headhunter`. Unidentified items carry the base type here.
    pub name: Option<String>,
    /// Base type line, e.g. `Leather Belt`.
    pub base_type: Option<String>,
    pub rarity: Option<Rarity>,
    /// The clipboard text this item was parsed from.
    pub raw: String,
}

impl Item {
    /// An empty item that keeps the text it came from, so fixtures stay traceable
    /// while the parser is still a stub.
    pub fn from_raw(raw: impl Into<String>) -> Self {
        Self { raw: raw.into(), ..Self::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_rarity_header() {
        assert_eq!(Rarity::from_header("Rarity: Rare"), Some(Rarity::Rare));
        assert_eq!(Rarity::from_header("Rarity: Unique"), Some(Rarity::Unique));
        assert_eq!(Rarity::from_header("Rarity: Foo"), None);
        assert_eq!(Rarity::from_header("Item Class: Belts"), None);
    }
}
