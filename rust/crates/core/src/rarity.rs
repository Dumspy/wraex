use serde::{Deserialize, Serialize};

/// How rare an item is: `ItemRarity`, the same enum in both references.
///
/// Serialises as their values (`"Rare"`, see `renderer/src/parser/ParsedItem.ts`),
/// so fixtures compare field by field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_serialises_like_the_reference() {
        assert_eq!(serde_json::to_string(&Rarity::Rare).unwrap(), r#""Rare""#);
    }
}
