use serde::{Deserialize, Serialize};

/// How rare an item is.
///
/// Serialises as upstream's `ItemRarity` values (`"Rare"`, see
/// `renderer/src/parser/ParsedItem.ts`), so fixtures compare field by field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

/// A parsed clipboard item.
///
/// Names and serialisation follow upstream's `ParsedItem` (`rawText`) so fixtures
/// compare directly. Deliberately tiny: fields arrive with the parser modules that
/// produce them. Rarity, for instance, is matched through the localised client
/// strings (`Parser.ts` in `ref/apt`), not by matching English words here.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    /// The clipboard text this item was parsed from.
    pub raw_text: String,
}

impl Item {
    pub fn new(raw_text: impl Into<String>) -> Self {
        Self { raw_text: raw_text.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_and_raw_text_serialise_like_the_reference() {
        // Upstream: `{ rarity: 'Rare', rawText: '...' }`.
        assert_eq!(serde_json::to_string(&Rarity::Rare).unwrap(), r#""Rare""#);
        let item = Item::new("Rarity: Rare");
        assert_eq!(serde_json::to_string(&item).unwrap(), r#"{"rawText":"Rarity: Rare"}"#);
    }
}
