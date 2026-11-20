//! The per-game seam: what the shared core and the app need from a specific Path
//! of Exile game.
//!
//! The trait stays small on purpose. Phase 3 diffs `ref/apt` against `ref/ee2`
//! module by module; anything both games do identically belongs in `wraex-core`,
//! and only genuinely game-specific behaviour lands behind this trait.

use thiserror::Error;
use wraex_core::{GameId, Item};

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("clipboard text is not an item this parser understands")]
    NotAnItem,
    #[error("{game} parsing is not ported yet")]
    NotPorted { game: GameId },
}

/// One supported game.
pub trait Game: Send + Sync + 'static {
    fn id(&self) -> GameId;

    /// Display name, e.g. `Path of Exile 2`.
    fn name(&self) -> &'static str;

    /// Parses the text produced by the in-game copy key (Ctrl+Alt+C).
    ///
    /// Reference: `ref/apt/renderer/src/parser/Parser.ts` (`parseClipboard`) for
    /// PoE1, `ref/ee2/renderer/src/parser/Parser.ts` for PoE2.
    fn parse_clipboard(&self, text: &str) -> Result<Item, ParseError>;
}
