//! Path of Exile 2: parser, data loading and game-specific filters.
//!
//! Ported from `ref/ee2` (Exiled Exchange 2). Its vitest suite under
//! `ref/ee2/renderer/specs/` passes at the pinned revision (527 passed, 2 skipped,
//! after `npm run make-index-files`) and seeds the PoE2 fixtures.

pub mod data;

use wraex_core::{GameId, Item};
use wraex_game::{Game, ParseError};

#[derive(Debug, Default, Clone, Copy)]
pub struct Poe2;

impl Game for Poe2 {
    fn id(&self) -> GameId {
        GameId::Poe2
    }

    fn name(&self) -> &'static str {
        "Path of Exile 2"
    }

    fn parse_clipboard(&self, _text: &str) -> Result<Item, ParseError> {
        Err(ParseError::NotPorted { game: GameId::Poe2 })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_ported_game() {
        assert_eq!(Poe2.id(), GameId::Poe2);
    }
}
