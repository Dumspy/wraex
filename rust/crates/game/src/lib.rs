//! The per-game seam: what the shared core and the app need from a specific Path
//! of Exile game.
//!
//! The trait stays small on purpose. Phase 3 diffs `ref/apt` against `ref/ee2`
//! module by module; anything both games do identically belongs in `wraex-core`,
//! and only genuinely game-specific behaviour lands behind this trait.

use std::fmt;

use serde::Serialize;
use thiserror::Error;
use wraex_core::GameId;

#[derive(Debug, Error)]
pub enum ParseError {
    /// The reference's `parseClipboard` returned an error: `item.unknown` (not in
    /// the game data) or `item.parse_error` (not item text, or a parser threw).
    #[error("{key}")]
    Rejected { key: &'static str },
    #[error("{game} parsing is not ported yet")]
    NotPorted { game: GameId },
}

/// One supported game.
///
/// The games' items, data and filters differ in shape (PLAN.md, phase 3), so they
/// are associated types; the app picks a game through an enum over the two
/// implementations rather than a trait object.
pub trait Game: Send + Sync + 'static {
    /// One language's game data.
    type Data;
    /// The parsed item, serialising like the reference's `ParsedItem`.
    type Item: Serialize + fmt::Debug;

    fn id(&self) -> GameId;

    /// Display name, e.g. `Path of Exile 2`.
    fn name(&self) -> &'static str;

    /// Parses the text produced by the in-game copy key (Ctrl+Alt+C).
    ///
    /// Reference: `ref/apt/renderer/src/parser/Parser.ts` (`parseClipboard`) for
    /// PoE1, `ref/ee2/renderer/src/parser/Parser.ts` for PoE2.
    fn parse_clipboard(&self, data: &Self::Data, text: &str) -> Result<Self::Item, ParseError>;
}
