//! Path of Exile 1: parser, data loading and game-specific filters.
//!
//! Ported from `ref/apt` (Awakened PoE Trade). Nothing is ported yet; the parser
//! lands with phase 3, after the golden fixtures exist (phase 1).

use wraex_core::{GameId, Item};
use wraex_game::{Game, ParseError};

#[derive(Debug, Default, Clone, Copy)]
pub struct Poe1;

impl Game for Poe1 {
    fn id(&self) -> GameId {
        GameId::Poe1
    }

    fn name(&self) -> &'static str {
        "Path of Exile"
    }

    fn parse_clipboard(&self, _text: &str) -> Result<Item, ParseError> {
        Err(ParseError::NotPorted { game: GameId::Poe1 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_ported_game() {
        assert_eq!(Poe1.id(), GameId::Poe1);
    }
}
