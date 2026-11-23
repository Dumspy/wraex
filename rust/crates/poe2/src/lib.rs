//! Path of Exile 2: parser, data loading and game-specific filters.
//!
//! Ported from `ref/ee2` (Exiled Exchange 2). Its vitest suite under
//! `ref/ee2/renderer/specs/` passes at the pinned revision (527 passed, 2 skipped,
//! after `npm run make-index-files`) and seeds the PoE2 fixtures.

pub mod data;
pub mod filters;
pub mod parser;

use wraex_core::GameId;
use wraex_game::{Game, ParseError};

#[derive(Debug, Default, Clone, Copy)]
pub struct Poe2;

impl Game for Poe2 {
    type Data = data::Data;
    type Item = parser::item::Item;

    fn id(&self) -> GameId {
        GameId::Poe2
    }

    fn name(&self) -> &'static str {
        "Path of Exile 2"
    }

    fn parse_clipboard(&self, data: &data::Data, text: &str) -> Result<parser::item::Item, ParseError> {
        parser::parse_clipboard(data, text).map_err(|parser::Fail(key)| ParseError::Rejected { key })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_ported_game() {
        assert_eq!(Poe2.id(), GameId::Poe2);
    }

    /// `parseClipboard` output equals `parsed.json` for every PoE2 fixture. The
    /// harness serves EE2's trade data snapshot, so the tests load it too.
    #[test]
    fn parsed_items_match_the_fixtures() {
        let mut langs = std::collections::HashMap::new();
        let mut failures = Vec::new();
        let cases = wraex_golden::cases(GameId::Poe2);
        assert!(!cases.is_empty());
        for case in &cases {
            let lang = case.options["lang"].as_str().unwrap().to_owned();
            let data = langs.entry(lang.clone()).or_insert_with(|| {
                data::Data::load(&data::reference_source(), &lang, Some(data::reference_trade_data())).unwrap()
            });
            let actual = match Poe2.parse_clipboard(data, &case.input) {
                Ok(item) => wraex_core::js::json_value(&item),
                Err(ParseError::Rejected { key }) => serde_json::json!({ "error": key }),
            };
            if let Some(diff) = wraex_golden::diff(&case.parsed, &actual) {
                failures.push(format!("{}: {diff}", case.slug));
            }
        }
        assert!(
            failures.is_empty(),
            "{} of {} cases differ:\n\n{}",
            failures.len(),
            cases.len(),
            failures.join("\n\n")
        );
    }

    /// `createPresets` output equals `presets.json` for every PoE2 fixture.
    #[test]
    fn presets_match_the_fixtures() {
        let mut langs = std::collections::HashMap::new();
        let mut failures = Vec::new();
        let cases = wraex_golden::cases(GameId::Poe2);
        let mut compared = 0;
        for case in &cases {
            let Some(expected) = &case.presets else { continue };
            let lang = case.options["lang"].as_str().unwrap().to_owned();
            let data = langs.entry(lang.clone()).or_insert_with(|| {
                data::Data::load(&data::reference_source(), &lang, Some(data::reference_trade_data())).unwrap()
            });
            let item = parser::parse_clipboard(data, &case.input).unwrap();
            let opts: filters::CreatePresetsOptions = serde_json::from_value(case.options["options"].clone()).unwrap();
            let actual = match filters::create_presets(data, &item, &opts) {
                Ok(presets) => wraex_core::js::json_value(&presets),
                Err(e) => serde_json::json!({ "error": e.to_string() }),
            };
            compared += 1;
            if let Some(diff) = wraex_golden::diff(expected, &actual) {
                failures.push(format!("{}: {diff}", case.slug));
            }
        }
        assert!(compared > 0);
        assert!(failures.is_empty(), "{} of {compared} cases differ:\n\n{}", failures.len(), failures.join("\n\n"));
    }
}
