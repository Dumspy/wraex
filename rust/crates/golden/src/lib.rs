//! The Rust side of the golden fixture harness (`tools/golden`).
//!
//! The TypeScript references write `fixtures/<game>/<case>/`; ported modules load
//! those cases here, produce the same JSON, and compare it as text with
//! [`canonical`]. Layout and rules: `tools/golden/README.md`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use wraex_core::GameId;

/// `fixtures/` at the repository root.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures")
}

/// One recorded case: the reference's inputs and outputs.
#[derive(Debug, Clone)]
pub struct Case {
    pub game: GameId,
    pub slug: String,
    pub dir: PathBuf,
    /// `input.txt`, the clipboard text.
    pub input: String,
    /// `options.json`: `lang`, `options`, `have`, `config`.
    pub options: Value,
    /// `parsed.json`: the `ParsedItem`, or `{ "error": ... }` if parsing failed.
    pub parsed: Value,
    /// `presets.json`, absent when parsing failed.
    pub presets: Option<Value>,
    /// `map-check.json`: the map check's prepared stats, for map-like items.
    pub map_check: Option<Value>,
    /// `item-check.json`: what the item check's hotkey actions open or send.
    pub item_check: Option<Value>,
    /// `trade/<preset id>.json` as `(id, { api, request })`, sorted by id.
    pub trade: Vec<(String, Value)>,
}

impl Case {
    /// The parser's error key (`item.unknown`, ...) when the reference rejected the text.
    pub fn parse_error(&self) -> Option<&str> {
        self.parsed.get("error").and_then(Value::as_str)
    }
}

/// `fixtures/listings/<game>/<name>`: what the reference makes of a recorded
/// trade site answer, with the path of that answer in the reference checkout.
pub fn listing_fixture(game: GameId, name: &str) -> Value {
    let file = fixtures_dir().join("listings").join(game.as_str()).join(name);
    let text = fs::read_to_string(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()))
}

/// A settings case: `fixtures/config/<game>/<slug>/` (`tools/golden/src/config-cases.ts`).
#[derive(Debug, Clone)]
pub struct ConfigCase {
    pub slug: String,
    /// The config file's text; `None` for the `default` case (no file).
    pub input: Option<String>,
    /// `defaultConfig()`, for the `default` case only.
    pub default: Option<Value>,
    /// The config after `initConfig`.
    pub loaded: Value,
    /// `getConfigForHost()` of it.
    pub host: Value,
    /// The save events `initConfig` sent (an unreadable file's temporary save).
    pub saves: Option<Value>,
}

/// Every settings case for `game`, `default` first.
pub fn config_cases(game: GameId) -> Vec<ConfigCase> {
    let root = fixtures_dir().join("config").join(game.as_str());
    let mut slugs: Vec<String> = match fs::read_dir(&root) {
        Ok(entries) => entries.map(|e| e.unwrap().file_name().into_string().unwrap()).collect(),
        Err(_) => return Vec::new(),
    };
    slugs.sort_by_key(|s| (s != "default", s.clone()));
    slugs
        .into_iter()
        .map(|slug| {
            let dir = root.join(&slug);
            let read = |name: &str| fs::read_to_string(dir.join(name)).ok();
            let json = |name: &str| -> Option<Value> {
                read(name).map(|t| serde_json::from_str(&t).unwrap_or_else(|e| panic!("{}/{name}: {e}", dir.display())))
            };
            ConfigCase {
                input: read("input.txt"),
                default: json("config.json"),
                loaded: json("loaded.json").expect("loaded.json"),
                host: json("host.json").expect("host.json"),
                saves: json("saves.json"),
                slug,
            }
        })
        .collect()
}

/// Every case for `game`, sorted by slug.
pub fn cases(game: GameId) -> Vec<Case> {
    let root = fixtures_dir().join(game.as_str());
    let mut slugs: Vec<String> = match fs::read_dir(&root) {
        Ok(entries) => entries.map(|e| e.unwrap().file_name().into_string().unwrap()).collect(),
        Err(_) => return Vec::new(),
    };
    slugs.sort();
    slugs.into_iter().map(|slug| load(game, slug)).collect()
}

fn load(game: GameId, slug: String) -> Case {
    let dir = fixtures_dir().join(game.as_str()).join(&slug);
    let read =
        |name: &str| fs::read_to_string(dir.join(name)).unwrap_or_else(|e| panic!("{}/{name}: {e}", dir.display()));
    let json = |name: &str| -> Value {
        serde_json::from_str(&read(name)).unwrap_or_else(|e| panic!("{}/{name}: {e}", dir.display()))
    };

    let presets = dir.join("presets.json").exists().then(|| json("presets.json"));
    let map_check = dir.join("map-check.json").exists().then(|| json("map-check.json"));
    let item_check = dir.join("item-check.json").exists().then(|| json("item-check.json"));
    let mut trade = Vec::new();
    if let Ok(entries) = fs::read_dir(dir.join("trade")) {
        for entry in entries {
            let name = entry.unwrap().file_name().into_string().unwrap();
            let id = name.strip_suffix(".json").unwrap().to_owned();
            trade.push((id, json(&format!("trade/{name}"))));
        }
    }
    trade.sort_by(|a, b| a.0.cmp(&b.0));

    Case {
        game,
        input: read("input.txt"),
        options: json("options.json"),
        parsed: json("parsed.json"),
        presets,
        map_check,
        item_check,
        trade,
        dir,
        slug,
    }
}

/// The harness's canonical JSON text (`canonical` in `tools/golden/src/fixtures.ts`):
/// `JSON.stringify` with sorted keys, two-space indent and a trailing newline.
///
/// `serde_json`'s map is a `BTreeMap`, which sorts like JavaScript's default sort
/// for the ASCII keys the references use. Numbers keep `serde_json`'s formatting;
/// the round-trip test over every fixture is what pins it to JavaScript's.
pub fn canonical(value: &Value) -> String {
    let mut out = serde_json::to_string_pretty(value).expect("JSON values always serialise");
    out.push('\n');
    out
}

/// Compares `actual` with a fixture value as canonical text. On mismatch, returns
/// the first differing line with some context, for test failure messages.
pub fn diff(expected: &Value, actual: &Value) -> Option<String> {
    let (e, a) = (canonical(expected), canonical(actual));
    if e == a {
        return None;
    }
    let (el, al): (Vec<_>, Vec<_>) = (e.lines().collect(), a.lines().collect());
    let at = el.iter().zip(&al).position(|(x, y)| x != y).unwrap_or(el.len().min(al.len()));
    let from = at.saturating_sub(3);
    let show = |lines: &[&str]| lines[from.min(lines.len())..(at + 4).min(lines.len())].join("\n");
    Some(format!("first difference at line {}\n--- expected\n{}\n+++ actual\n{}", at + 1, show(&el), show(&al)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_json_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                all_json_files(&path, out);
            } else if path.extension().is_some_and(|e| e == "json") {
                out.push(path);
            }
        }
    }

    /// The Rust canonical form must reproduce the TypeScript one byte for byte,
    /// or no ported module could ever match its fixtures.
    #[test]
    fn canonical_matches_the_harness_for_every_fixture() {
        let mut files = Vec::new();
        all_json_files(&fixtures_dir(), &mut files);
        assert!(!files.is_empty(), "no fixtures: run `npm run regen` in tools/golden");
        for file in files {
            let text = fs::read_to_string(&file).unwrap();
            let value: Value = serde_json::from_str(&text).unwrap();
            let ours = canonical(&value);
            if let Some((n, (a, b))) = ours.lines().zip(text.lines()).enumerate().find(|(_, (a, b))| a != b) {
                panic!("{}:{}: harness wrote `{b}`, Rust writes `{a}`", file.display(), n + 1);
            }
            assert!(ours == text, "{} does not round-trip", file.display());
        }
    }

    #[test]
    fn every_case_loads_and_carries_its_input() {
        for game in GameId::ALL {
            let cases = cases(game);
            assert!(!cases.is_empty(), "no {game} fixtures");
            for case in cases {
                if case.parse_error().is_some() {
                    assert!(case.presets.is_none(), "{}: presets after a parse error", case.slug);
                    continue;
                }
                assert_eq!(case.parsed["rawText"].as_str(), Some(case.input.as_str()), "{}: rawText", case.slug);
                let presets = case.presets.as_ref().expect("presets.json");
                let ids: Vec<&str> =
                    presets["presets"].as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap()).collect();
                let mut trade_ids: Vec<&str> = case.trade.iter().map(|(id, _)| id.as_str()).collect();
                let mut sorted = ids.clone();
                sorted.sort();
                trade_ids.sort();
                assert_eq!(sorted, trade_ids, "{}: one trade request per preset", case.slug);
                assert!(ids.contains(&presets["active"].as_str().unwrap()), "{}: active preset", case.slug);
            }
        }
    }

    #[test]
    fn diff_points_at_the_first_changed_line() {
        let a = serde_json::json!({ "a": 1, "b": 2 });
        let b = serde_json::json!({ "a": 1, "b": 3 });
        assert_eq!(diff(&a, &a), None);
        assert!(diff(&a, &b).unwrap().starts_with("first difference at line 3"));
    }
}
