//! Path of Exile 2: parser, data loading and game-specific filters.
//!
//! Ported from `ref/ee2` (Exiled Exchange 2). Its vitest suite under
//! `ref/ee2/renderer/specs/` passes at the pinned revision (527 passed, 2 skipped,
//! after `npm run make-index-files`) and seeds the PoE2 fixtures.

pub mod data;
pub mod filters;
pub mod parser;
pub mod trade;

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

    /// `bulk-api.ts`' first (optimistic) exchange search.
    fn optimistic_have(ref_name: &str) -> Vec<String> {
        let have: &[&str] = match ref_name {
            "Exalted Orb" => &["divine", "chaos"],
            "Chaos Orb" => &["divine", "exalted"],
            "Divine Orb" => &["exalted", "chaos"],
            _ => &["divine", "exalted", "chaos"],
        };
        have.iter().map(|s| s.to_string()).collect()
    }

    /// Each preset's `{ api, request }` equals `trade/<id>.json`.
    #[test]
    fn trade_requests_match_the_fixtures() {
        let mut langs = std::collections::HashMap::new();
        let mut failures = Vec::new();
        let mut compared = 0;
        for case in &wraex_golden::cases(GameId::Poe2) {
            if case.presets.is_none() {
                continue;
            }
            let lang = case.options["lang"].as_str().unwrap().to_owned();
            let data = langs.entry(lang.clone()).or_insert_with(|| {
                data::Data::load(&data::reference_source(), &lang, Some(data::reference_trade_data())).unwrap()
            });
            let item = parser::parse_clipboard(data, &case.input).unwrap();
            let opts: filters::CreatePresetsOptions = serde_json::from_value(case.options["options"].clone()).unwrap();
            let presets = filters::create_presets(data, &item, &opts).unwrap();
            let have: Vec<String> = serde_json::from_value(case.options["have"].clone()).unwrap();
            assert_eq!(presets.presets.len(), case.trade.len(), "{}", case.slug);
            for preset in &presets.presets {
                let api = trade::api_to_satisfy_search(&item, &preset.stats, &preset.filters);
                let request = match api {
                    trade::Api::Bulk => {
                        // CheckedItem.vue forces online listings for bulk.
                        let mut filters = preset.filters.clone();
                        filters.trade.listing_type = "online".to_owned();
                        let ref_name = item.info().ref_name.clone().unwrap_or_default();
                        let have = if have.is_empty() { optimistic_have(&ref_name) } else { have.clone() };
                        wraex_core::js::json_value(&trade::create_bulk_request(&filters, &item, have))
                    }
                    trade::Api::Trade => match trade::create_trade_request(data, &preset.filters, &preset.stats, &item)
                    {
                        Ok(r) => wraex_core::js::json_value(&r),
                        Err(e) => serde_json::json!({ "error": e.to_string() }),
                    },
                };
                let actual = serde_json::json!({ "api": api, "request": request });
                let expected = &case.trade.iter().find(|(id, _)| id == &preset.id).unwrap().1;
                compared += 1;
                if let Some(diff) = wraex_golden::diff(expected, &actual) {
                    failures.push(format!("{} {}: {diff}", case.slug, preset.id));
                }
            }
        }
        assert!(compared > 0);
        assert!(failures.is_empty(), "{} of {compared} requests differ:\n\n{}", failures.len(), failures.join("\n\n"));
    }
}
