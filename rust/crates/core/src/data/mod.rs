//! The static game data both references ship: `items.ndjson`, `stats.ndjson`,
//! `client_strings.js`, read through a swappable [`DataSource`].
//!
//! Ports the mechanics of `renderer/src/assets/data/index.ts`, which are the same
//! in `ref/apt` and `ref/ee2`: the fnv1a-32 hash indexes that
//! `renderer/src/assets/make-index-files.mjs` writes to `*.index.bin`, the binary
//! search over them, and the substring search over raw ndjson lines. The indexes
//! are built in memory from the ndjson instead of being read from the generated
//! files. What a line contains (and so which keys it is indexed under) is up to
//! each game.

mod client_strings;
mod ndjson;

use std::fmt;
use std::path::{Path, PathBuf};

use thiserror::Error;

pub use client_strings::{ClientString, ClientStrings};
pub use ndjson::{HashIndex, ItemTable, Ndjson, StatKeys, StatTable, js_string};

/// `fnv1a(text, { size: 32 })` from `@sindresorhus/fnv1a`: FNV-1a over the UTF-8
/// bytes of `text`.
pub fn fnv1a32(text: &str) -> u32 {
    let mut hash: u32 = 0x811c_9dc5;
    for byte in text.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash
}

#[derive(Debug, Error)]
pub enum DataError {
    #[error("{path}: {source}")]
    Io { path: String, source: std::io::Error },
    #[error("{path}: not UTF-8")]
    Utf8 { path: String },
    #[error("{path}: does not end with a newline")]
    MissingNewline { path: String },
    #[error("{path}:{line}: {source}")]
    Json { path: String, line: usize, source: serde_json::Error },
    #[error("{path}:{line}: {message}")]
    Syntax { path: String, line: usize, message: String },
}

/// Where the data files come from. Paths are relative to the data root, as in the
/// references' `public/data/` (`en/items.ndjson`, `item-drop.json`).
///
/// Static data stays as upstream and EE2 ship it for now (AGENTS.md); this is the
/// seam for loading it from elsewhere later.
pub trait DataSource: Send + Sync + fmt::Debug {
    fn read(&self, path: &str) -> Result<Vec<u8>, DataError>;

    fn read_string(&self, path: &str) -> Result<String, DataError> {
        String::from_utf8(self.read(path)?).map_err(|_| DataError::Utf8 { path: path.to_owned() })
    }
}

/// Data files in a directory laid out like a reference's `renderer/public/data`.
#[derive(Debug, Clone)]
pub struct DirSource {
    root: PathBuf,
}

impl DirSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl DataSource for DirSource {
    fn read(&self, path: &str) -> Result<Vec<u8>, DataError> {
        let full = self.root.join(path);
        std::fs::read(&full).map_err(|source| DataError::Io { path: full.display().to_string(), source })
    }
}

/// JavaScript truthiness of a JSON value, for checks like `if (!record.disc)`.
pub fn js_truthy(value: &serde_json::Value) -> bool {
    use serde_json::Value;
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0 && !f.is_nan()),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

/// Structural JSON equality with JavaScript numbers: `2` and `2.0` are the same
/// value, as they are once `JSON.parse` has read them.
pub fn json_eq(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(x, y)| json_eq(x, y)),
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| json_eq(v, w)))
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_eq_compares_numbers_like_javascript() {
        use serde_json::json;
        assert!(json_eq(&json!({ "a": [2, 1.5] }), &json!({ "a": [2.0, 1.5] })));
        assert!(!json_eq(&json!({ "a": 1 }), &json!({ "a": 1, "b": null })));
        assert!(!json_eq(&json!([1]), &json!(["1"])));
    }

    #[test]
    fn fnv1a32_matches_the_reference_vectors() {
        // Published FNV-1a 32 test vectors, and what `@sindresorhus/fnv1a` gives.
        assert_eq!(fnv1a32(""), 0x811c_9dc5);
        assert_eq!(fnv1a32("a"), 0xe40c_292c);
        assert_eq!(fnv1a32("foobar"), 0xbf9c_f968);
        // Non-ASCII hashes the UTF-8 bytes (TextEncoder), not UTF-16 units.
        let step = |hash: u32, byte: u32| (hash ^ byte).wrapping_mul(16_777_619);
        assert_eq!(fnv1a32("é"), step(step(0x811c_9dc5, 0xc3), 0xa9));
    }
}
