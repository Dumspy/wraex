//! ndjson text, the fnv1a-32 indexes over it, and the lookups `index.ts` runs.
//!
//! Offsets are byte offsets into the text. The references store UTF-16 offsets
//! (JavaScript string indexes); the entries and their order are the same, which is
//! what decides the result of a lookup when hashes collide.

use std::collections::HashSet;

use serde::Deserialize;
use serde_json::Value;

use super::{DataError, fnv1a32, js_truthy};

/// The text of one ndjson file: one JSON value per line, ending in a newline.
#[derive(Debug, Clone)]
pub struct Ndjson {
    text: String,
}

impl Ndjson {
    /// `path` is only used in errors.
    pub fn new(path: &str, text: String) -> Result<Self, DataError> {
        // The references loop `while (start !== ndjson.length)` and slice up to the
        // next newline, which never terminates without a trailing one.
        if !text.is_empty() && !text.ends_with('\n') {
            return Err(DataError::MissingNewline { path: path.to_owned() });
        }
        Ok(Self { text })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// Every line with the byte offset it starts at, in file order.
    pub fn lines(&self) -> impl Iterator<Item = (usize, &str)> {
        let mut start = 0;
        std::iter::from_fn(move || {
            if start == self.text.len() {
                return None;
            }
            let end = start + self.text[start..].find('\n').expect("ends with a newline");
            let line = (start, &self.text[start..end]);
            start = end + 1;
            Some(line)
        })
    }

    /// The line starting at `start` (`ndjson.slice(start, ndjson.indexOf('\n', start))`).
    pub fn line_at(&self, start: usize) -> &str {
        let rest = &self.text[start..];
        &rest[..rest.find('\n').expect("ends with a newline")]
    }

    /// `ndjsonFindLines(ndjson)(includes, andIncludes)`: lines whose raw text
    /// contains `includes` and every string in `and_includes`.
    ///
    /// Matches against the raw text, so the search strings depend on how the file
    /// is formatted (PoE2's ndjson has spaces after `:`, PoE1's does not). After a
    /// match the search resumes on the next line.
    pub fn find_lines<'a>(&'a self, includes: &'a str, and_includes: &'a [&'a str]) -> impl Iterator<Item = &'a str> {
        let text = self.text.as_str();
        let mut start = 0;
        std::iter::from_fn(move || {
            while start != text.len() {
                let match_pos = start + text[start..].find(includes)?;
                // Works for the first line too (no newline before it).
                let line_start = text[..match_pos].rfind('\n').map_or(0, |i| i + 1);
                let end = match_pos + text[match_pos..].find('\n').expect("ends with a newline");
                let line = &text[line_start..end];
                start = end + 1;
                if and_includes.iter().all(|s| line.contains(s)) {
                    return Some(line);
                }
            }
            None
        })
    }

    fn parse_line<'a, T: Deserialize<'a>>(&self, path: &str, start: usize, line: &'a str) -> Result<T, DataError> {
        serde_json::from_str(line).map_err(|source| DataError::Json {
            path: path.to_owned(),
            line: self.text[..start].matches('\n').count() + 1,
            source,
        })
    }
}

/// A sorted `[hash, start]` table, the in-memory form of an `*.index.bin` file.
#[derive(Debug, Clone, Default)]
pub struct HashIndex {
    rows: Vec<(u32, u32)>,
}

impl HashIndex {
    /// Sorts `(hash, start)` rows by hash, keeping the input order of equal hashes
    /// (`Array.prototype.sort` is stable).
    pub fn new(mut rows: Vec<(u32, u32)>) -> Self {
        rows.sort_by_key(|&(hash, _)| hash);
        Self { rows }
    }

    /// Reads an `*.index.bin` the references generate for `ndjson` (little-endian
    /// `[hash, start]` pairs, `start` in UTF-16 units) into byte offsets. `None` if
    /// the file is malformed or a start is not the start of a line.
    pub fn from_reference_bin(bin: &[u8], ndjson: &Ndjson) -> Option<Self> {
        if !bin.len().is_multiple_of(8) {
            return None;
        }
        let mut line_starts = std::collections::HashMap::new();
        let mut utf16 = 0u32;
        for (start, line) in ndjson.lines() {
            line_starts.insert(utf16, offset(start));
            utf16 += line.encode_utf16().count() as u32 + 1;
        }
        let word = |at: usize| u32::from_le_bytes(bin[at..at + 4].try_into().unwrap());
        let rows = (0..bin.len() / 8)
            .map(|i| Some((word(i * 8), *line_starts.get(&word(i * 8 + 4))?)))
            .collect::<Option<Vec<_>>>()?;
        Some(Self { rows })
    }

    pub fn rows(&self) -> &[(u32, u32)] {
        &self.rows
    }

    /// `dataBinarySearch`, then the row's start offset. With duplicate hashes this
    /// returns whichever row the bisection lands on, exactly like the reference.
    pub fn find(&self, hash: u32) -> Option<usize> {
        let (mut left, mut right) = (0i64, self.rows.len() as i64 - 1);
        while left <= right {
            let mid = (left + right) / 2;
            let (value, start) = self.rows[mid as usize];
            if value < hash {
                left = mid + 1;
            } else if value > hash {
                right = mid - 1;
            } else {
                return Some(start as usize);
            }
        }
        None
    }

    pub fn find_str(&self, key: &str) -> Option<usize> {
        self.find(fnv1a32(key))
    }
}

/// A possibly missing string as a JavaScript template literal prints it.
pub fn js_string(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("undefined")
}

fn offset(start: usize) -> u32 {
    u32::try_from(start).expect("data files are smaller than 4 GiB")
}

/// The fields of an `items.ndjson` line that the indexes and `commonFind` use.
/// `name` and `refName` can be missing (EE2 ships nameless uniques); JavaScript
/// then carries on with `undefined`.
#[derive(Deserialize)]
struct ItemHead {
    namespace: String,
    name: Option<String>,
    #[serde(rename = "refName")]
    ref_name: Option<String>,
    #[serde(default)]
    disc: Value,
    #[serde(default)]
    unique: Value,
}

/// `items.ndjson` with its `items-name` and `items-ref` indexes.
#[derive(Debug, Clone)]
pub struct ItemTable {
    path: String,
    ndjson: Ndjson,
    by_name: HashIndex,
    by_ref: HashIndex,
}

impl ItemTable {
    /// Builds both indexes like `make-index-files.mjs`: one row per distinct
    /// `namespace::refName`, at its first line, hashed as `namespace::name` and
    /// `namespace::refName`.
    pub fn new(path: &str, text: String) -> Result<Self, DataError> {
        let ndjson = Ndjson::new(path, text)?;
        let mut seen = HashSet::new();
        let (mut by_name, mut by_ref) = (Vec::new(), Vec::new());
        for (start, line) in ndjson.lines() {
            let item: ItemHead = ndjson.parse_line(path, start, line)?;
            let key = |name: &Option<String>| format!("{}::{}", item.namespace, js_string(name));
            if seen.insert(key(&item.ref_name)) {
                by_name.push((fnv1a32(&key(&item.name)), offset(start)));
                by_ref.push((fnv1a32(&key(&item.ref_name)), offset(start)));
            }
        }
        Ok(Self { path: path.to_owned(), ndjson, by_name: HashIndex::new(by_name), by_ref: HashIndex::new(by_ref) })
    }

    pub fn ndjson(&self) -> &Ndjson {
        &self.ndjson
    }

    pub fn name_index(&self) -> &HashIndex {
        &self.by_name
    }

    pub fn ref_index(&self) -> &HashIndex {
        &self.by_ref
    }

    /// `ITEM_BY_TRANSLATED(ns, name)`: the matching lines, unparsed.
    pub fn by_translated(&self, namespace: &str, name: &str) -> Option<Vec<&str>> {
        self.common_find(&self.by_name, namespace, name, false)
    }

    /// `ITEM_BY_REF(ns, refName)`: the matching lines, unparsed.
    pub fn by_ref(&self, namespace: &str, ref_name: &str) -> Option<Vec<&str>> {
        self.common_find(&self.by_ref, namespace, ref_name, true)
    }

    /// `commonFind`: `None` when the hash is not indexed; an empty list when it is
    /// but the line belongs to another name (an fnv1a collision). Following lines
    /// with the same name are included while the records have `disc` or `unique`.
    fn common_find(&self, index: &HashIndex, namespace: &str, name: &str, by_ref: bool) -> Option<Vec<&str>> {
        let mut start = index.find_str(&format!("{namespace}::{name}"))?;
        let mut out = Vec::new();
        while start != self.ndjson.text.len() {
            let line = self.ndjson.line_at(start);
            let record: ItemHead =
                self.ndjson.parse_line(&self.path, start, line).expect("every line parsed when the table was built");
            let prop = if by_ref { &record.ref_name } else { &record.name };
            if record.namespace != namespace || prop.as_deref() != Some(name) {
                break;
            }
            out.push(line);
            if !js_truthy(&record.disc) && !js_truthy(&record.unique) {
                break;
            }
            start += line.len() + 1;
        }
        Some(out)
    }

    /// `ITEMS_ITERATOR(includes, andIncludes)`, unparsed.
    pub fn find_lines<'a>(&'a self, includes: &'a str, and_includes: &'a [&'a str]) -> impl Iterator<Item = &'a str> {
        self.ndjson.find_lines(includes, and_includes)
    }
}

/// The keys a `stats.ndjson` line is indexed under, in the order
/// `make-index-files.mjs` pushes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatKeys {
    /// For `stats-ref.index.bin`.
    pub refs: Vec<String>,
    /// For `stats-matcher.index.bin`.
    pub matchers: Vec<String>,
}

/// `stats.ndjson` with its `stats-ref` and `stats-matcher` indexes. What a line
/// holds differs per game (PoE1 lines can be stat groups), so the game parses
/// lines and says which keys each one has.
#[derive(Debug, Clone)]
pub struct StatTable {
    ndjson: Ndjson,
    by_ref: HashIndex,
    by_matcher: HashIndex,
}

impl StatTable {
    pub fn new<T, F>(path: &str, text: String, keys: F) -> Result<Self, DataError>
    where
        T: for<'de> Deserialize<'de>,
        F: Fn(&T) -> StatKeys,
    {
        let ndjson = Ndjson::new(path, text)?;
        let (mut by_ref, mut by_matcher) = (Vec::new(), Vec::new());
        for (start, line) in ndjson.lines() {
            let parsed: T = ndjson.parse_line(path, start, line)?;
            let keys = keys(&parsed);
            by_ref.extend(keys.refs.iter().map(|k| (fnv1a32(k), offset(start))));
            by_matcher.extend(keys.matchers.iter().map(|k| (fnv1a32(k), offset(start))));
        }
        Ok(Self { ndjson, by_ref: HashIndex::new(by_ref), by_matcher: HashIndex::new(by_matcher) })
    }

    pub fn ndjson(&self) -> &Ndjson {
        &self.ndjson
    }

    pub fn ref_index(&self) -> &HashIndex {
        &self.by_ref
    }

    pub fn matcher_index(&self) -> &HashIndex {
        &self.by_matcher
    }

    /// The line indexed under `ref`, unverified (`STAT_BY_REF` returns whatever
    /// line the hash points at).
    pub fn line_by_ref(&self, stat_ref: &str) -> Option<&str> {
        self.by_ref.find_str(stat_ref).map(|start| self.ndjson.line_at(start))
    }

    /// The line indexed under matcher text `matcher`, unverified; the games check
    /// for collisions themselves, each in its own way.
    pub fn line_by_matcher(&self, matcher: &str) -> Option<&str> {
        self.by_matcher.find_str(matcher).map(|start| self.ndjson.line_at(start))
    }

    /// `STATS_ITERATOR(includes, andIncludes)`, unparsed.
    pub fn find_lines<'a>(&'a self, includes: &'a str, and_includes: &'a [&'a str]) -> impl Iterator<Item = &'a str> {
        self.ndjson.find_lines(includes, and_includes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items() -> ItemTable {
        let text = [
            r#"{"name":"Alpha","refName":"Alpha","namespace":"ITEM","icon":""}"#,
            r#"{"name":"Beta","refName":"Beta","namespace":"UNIQUE","icon":"","unique":{"base":"Alpha"}}"#,
            r#"{"name":"Beta","refName":"Beta","namespace":"UNIQUE","icon":"","unique":{"base":"Gamma"}}"#,
            r#"{"name":"Beta","refName":"Beta","namespace":"ITEM","icon":""}"#,
            r#"{"name":"Альфа","refName":"Alpha","namespace":"GEM","icon":""}"#,
            r#"{"namespace":"UNIQUE","icon":"","unique":{"base":"Ring"}}"#,
            "",
        ]
        .join("\n");
        ItemTable::new("items.ndjson", text).unwrap()
    }

    #[test]
    fn rejects_text_without_a_trailing_newline() {
        assert!(matches!(Ndjson::new("x", "{}".into()), Err(DataError::MissingNewline { .. })));
        assert_eq!(Ndjson::new("x", String::new()).unwrap().lines().count(), 0);
    }

    #[test]
    fn looks_items_up_by_name_and_ref() {
        let items = items();
        assert_eq!(items.by_ref("ITEM", "Alpha").unwrap().len(), 1);
        // Uniques sharing a name continue onto the following lines.
        let betas = items.by_translated("UNIQUE", "Beta").unwrap();
        assert_eq!(betas.len(), 2);
        assert!(betas[1].contains("Gamma"));
        assert_eq!(items.by_translated("GEM", "Альфа").unwrap().len(), 1);
        assert_eq!(items.by_ref("GEM", "Alpha").unwrap().len(), 1);
        assert_eq!(items.by_ref("GEM", "Альфа"), None);
        assert_eq!(items.by_translated("ITEM", "Missing"), None);
        // One index row per distinct `namespace::refName`.
        assert_eq!(items.ref_index().rows().len(), 5);
        // A nameless line is indexed as `UNIQUE::undefined` but never matches.
        assert_eq!(items.by_ref("UNIQUE", "undefined"), Some(vec![]));
    }

    #[test]
    fn a_hash_collision_yields_an_empty_list() {
        // Point `ITEM::Missing`'s hash at an unrelated line.
        let mut items = items();
        let rows = vec![(fnv1a32("ITEM::Missing"), 0)];
        items.by_name = HashIndex::new(rows);
        assert_eq!(items.by_translated("ITEM", "Missing"), Some(vec![]));
    }

    #[test]
    fn find_lines_matches_raw_text_once_per_line() {
        let items = items();
        let found: Vec<_> = items.find_lines(r#""Beta""#, &[]).collect();
        assert_eq!(found.len(), 3);
        let found: Vec<_> = items.find_lines(r#""Beta""#, &["UNIQUE", "Gamma"]).collect();
        assert_eq!(found.len(), 1);
        // The first line has no newline before it.
        assert_eq!(items.find_lines("Alpha", &[]).next().unwrap(), items.ndjson().lines().next().unwrap().1);
        assert_eq!(items.find_lines("", &[]).count(), 6);
        assert_eq!(items.find_lines("nowhere", &[]).count(), 0);
    }

    #[test]
    fn reads_reference_index_files_with_utf16_offsets() {
        // "é" is one UTF-16 unit but two bytes; "😀" is two units and four bytes.
        let ndjson = Ndjson::new("x", "\"é😀\"\n\"b\"\n".into()).unwrap();
        let bin: Vec<u8> = [7u32, 6, 3, 0].iter().flat_map(|w| w.to_le_bytes()).collect();
        let index = HashIndex::from_reference_bin(&bin, &ndjson).unwrap();
        assert_eq!(index.rows(), &[(7, 9), (3, 0)]);
        assert_eq!(ndjson.line_at(9), "\"b\"");
        // Not a line start, and a truncated file.
        assert!(HashIndex::from_reference_bin(&[1, 0, 0, 0, 2, 0, 0, 0], &ndjson).is_none());
        assert!(HashIndex::from_reference_bin(&bin[..7], &ndjson).is_none());
    }

    #[test]
    fn binary_search_handles_empty_and_edge_rows() {
        assert_eq!(HashIndex::default().find(1), None);
        let index = HashIndex::new(vec![(5, 50), (1, 10), (9, 90), (5, 51)]);
        assert_eq!(index.rows(), &[(1, 10), (5, 50), (5, 51), (9, 90)]);
        assert_eq!(index.find(1), Some(10));
        assert_eq!(index.find(9), Some(90));
        // Bisection lands on the first of the two 5s: mid = (0 + 3) / 2 = 1.
        assert_eq!(index.find(5), Some(50));
        assert_eq!(index.find(0), None);
        assert_eq!(index.find(u32::MAX), None);
    }
}
