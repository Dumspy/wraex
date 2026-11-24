//! Game-agnostic types shared by the PoE1 and PoE2 front ends.
//!
//! Everything here is ported from the TypeScript reference implementations in
//! `ref/apt` (PoE1) and `ref/ee2` (PoE2). The public surface starts small and grows
//! one module at a time, each ported module backed by golden fixtures (see
//! `PLAN.md`, phase 3).

pub mod assets;
pub mod config_store;
pub mod config_upgrade;
pub mod data;
pub mod filters;
pub mod game_files;
pub mod images;
pub mod js;
pub mod js_regex;
pub mod keys;
pub mod leagues;
pub mod levenshtein;
pub mod parse;
pub mod rarity;
pub mod relative_time;
pub mod trade;
pub mod updates;

use serde::{Deserialize, Serialize};

pub use rarity::Rarity;

/// Which Path of Exile game an item, data set or game window belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameId {
    Poe1,
    Poe2,
}

impl GameId {
    pub const ALL: [GameId; 2] = [GameId::Poe1, GameId::Poe2];

    /// Stable directory/CLI name, e.g. `poe1` for `fixtures/poe1/`.
    pub const fn as_str(self) -> &'static str {
        match self {
            GameId::Poe1 => "poe1",
            GameId::Poe2 => "poe2",
        }
    }
}

impl std::fmt::Display for GameId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_ids_have_stable_names() {
        assert_eq!(GameId::Poe1.to_string(), "poe1");
        assert_eq!(GameId::Poe2.to_string(), "poe2");
        assert_eq!(GameId::ALL.len(), 2);
    }
}
