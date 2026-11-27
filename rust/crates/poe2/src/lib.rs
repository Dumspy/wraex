//! Path of Exile 2: parser, data loading and game-specific filters.
//!
//! Ported from `ref/ee2` (Exiled Exchange 2). Its vitest suite under
//! `ref/ee2/renderer/specs/` passes at the pinned revision (527 passed, 2 skipped,
//! after `npm run make-index-files`) and seeds the PoE2 fixtures.

pub mod client_log;
pub mod config;
pub mod data;
pub mod filters;
pub mod item_check;
pub mod item_search;
pub mod map_check;
pub mod parser;
pub mod prices;
pub mod results;
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
#[expect(clippy::disallowed_macros, reason = "expected JSON written as literals")]
mod tests {
    use serde::Deserialize;
    use serde_json::{Value, json};

    use super::*;

    /// One fixture case's recorded options (`options.json`).
    #[derive(Deserialize)]
    struct CaseOptions {
        lang: String,
        #[serde(default)]
        options: Value,
        #[serde(default)]
        have: Vec<String>,
        #[serde(default)]
        config: CaseConfig,
    }

    /// `options.json`'s `config`: the settings the reference used.
    #[derive(Deserialize, Default)]
    struct CaseConfig {
        #[serde(default)]
        language: Option<String>,
    }

    /// `saves.json`: a recorded `CLIENT->MAIN::save-config` event.
    #[derive(Deserialize)]
    struct SaveEvent {
        payload: SavePayload,
    }

    /// `saveConfig({ ... })`'s payload.
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SavePayload {
        contents: String,
        is_temporary: bool,
    }

    /// EE2's recorded fetch answer (`specs/docs/fetchResponses.json`).
    #[derive(Deserialize)]
    struct FetchAnswer {
        result: Vec<Value>,
    }

    /// The `id` the display-item fixture keys a `result` entry on.
    #[derive(Deserialize)]
    struct FetchResultId {
        #[serde(default)]
        id: Value,
    }

    /// `case.options`, typed.
    fn options(case: &wraex_golden::Case) -> CaseOptions {
        serde_json::from_value(case.options.clone()).unwrap()
    }

    #[test]
    fn reports_the_ported_game() {
        assert_eq!(Poe2.id(), GameId::Poe2);
    }

    /// `specs/Parser/integration.test.ts` (`Parse Item Properties`, `Parse Charm
    /// Properties`): the fields those assertions read off whole items.
    #[test]
    fn test_items_parse_the_properties_the_spec_asserts() {
        use crate::parser::item::Requires;

        let data = data::Data::load(&data::reference_source(), "en", Some(data::reference_trade_data())).unwrap();
        let parse = |slug: &str| {
            let input = wraex_golden::cases(GameId::Poe2)
                .into_iter()
                .find(|case| case.slug == slug)
                .unwrap_or_else(|| panic!("no fixture {slug}"))
                .input;
            parser::parse_clipboard(&data, &input).unwrap()
        };

        let rare = parse("ee2-rare-item");
        assert_eq!((rare.weapon_physical, rare.weapon_elemental), (Some(48.5), Some(86.0)));
        assert_eq!((rare.weapon_as, rare.weapon_crit), (Some(1.2), Some(5.0)));
        assert_eq!(rare.requires, Some(Requires { level: 51.0, str: 0.0, dex: 103.0, int: 0.0 }));

        let spectre = parse("ee2-spectre-inc-spirit");
        assert_eq!(spectre.weapon_spirit, Some(152.0));
        assert_eq!(spectre.requires, Some(Requires { level: 72.0, str: 39.0, dex: 0.0, int: 98.0 }));

        let normal = parse("ee2-normal-item");
        assert_eq!((normal.armour_ar, normal.armour_ev, normal.armour_es), (Some(174.0), None, Some(60.0)));
        assert_eq!(normal.requires, Some(Requires { level: 75.0, str: 67.0, dex: 0.0, int: 67.0 }));

        assert_eq!(parse("ee2-charm-quality").quality, Some(14.0));
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
            let lang = options(case).lang;
            let data = langs.entry(lang.clone()).or_insert_with(|| {
                data::Data::load(&data::reference_source(), &lang, Some(data::reference_trade_data())).unwrap()
            });
            let actual = match Poe2.parse_clipboard(data, &case.input) {
                Ok(item) => wraex_core::js::json_value(&item),
                Err(ParseError::Rejected { key }) => json!({ "error": key }),
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
            let options = options(case);
            let lang = options.lang;
            let data = langs.entry(lang.clone()).or_insert_with(|| {
                data::Data::load(&data::reference_source(), &lang, Some(data::reference_trade_data())).unwrap()
            });
            let item = parser::parse_clipboard(data, &case.input).unwrap();
            let opts: filters::CreatePresetsOptions = serde_json::from_value(options.options).unwrap();
            let actual = match filters::create_presets(data, &item, &opts) {
                Ok(presets) => wraex_core::js::json_value(&presets),
                Err(e) => json!({ "error": e.to_string() }),
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
        have.iter().map(ToString::to_string).collect()
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
            let options = options(case);
            let lang = options.lang;
            let have = options.have;
            let data = langs.entry(lang.clone()).or_insert_with(|| {
                data::Data::load(&data::reference_source(), &lang, Some(data::reference_trade_data())).unwrap()
            });
            let item = parser::parse_clipboard(data, &case.input).unwrap();
            let opts: filters::CreatePresetsOptions = serde_json::from_value(options.options).unwrap();
            let presets = filters::create_presets(data, &item, &opts).unwrap();
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
                        Err(e) => json!({ "error": e.to_string() }),
                    },
                };
                let actual = json!({ "api": api, "request": request });
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

    /// The map check's stats equal `map-check.json`, recorded for map-like items.
    #[test]
    fn map_checks_match_the_fixtures() {
        let data = data::Data::load(&data::reference_source(), "en", Some(data::reference_trade_data())).unwrap();
        let mut failures = Vec::new();
        let mut compared = 0;
        for case in &wraex_golden::cases(GameId::Poe2) {
            if case.parse_error().is_some() {
                continue;
            }
            let item = parser::parse_clipboard(&data, &case.input).unwrap();
            assert_eq!(map_check::is_map_like(&item), case.map_check.is_some(), "{}: isMapLike", case.slug);
            let Some(expected) = &case.map_check else { continue };
            compared += 1;
            let actual = wraex_core::js::json_value(&map_check::prepare_map_stats(&item));
            if let Some(diff) = wraex_golden::diff(expected, &actual) {
                failures.push(format!("{}: {diff}", case.slug));
            }
        }
        assert!(compared > 0);
        assert!(failures.is_empty(), "{} of {compared} cases differ:\n\n{}", failures.len(), failures.join("\n\n"));
    }

    /// What the item check's actions open or send equals `item-check.json`.
    #[test]
    fn item_checks_match_the_fixtures() {
        let data = data::Data::load(&data::reference_source(), "en", Some(data::reference_trade_data())).unwrap();
        let mut failures = Vec::new();
        let mut compared = 0;
        for case in &wraex_golden::cases(GameId::Poe2) {
            let Some(expected) = &case.item_check else { continue };
            let language = options(case).config.language.unwrap();
            let item = parser::parse_clipboard(&data, &case.input).unwrap();
            let mut actual = json!({
                "wiki": item_check::open_wiki(&item),
                "poedb": item_check::open_poedb(&item, &language),
                "craftOfExile": item_check::open_coe(&item),
                "similar": item_check::find_similar_items(&item),
            });
            let same_priced = item_check::find_same_priced_items(&item, item_check::SamePricedType::Both);
            if let Some(action) = same_priced {
                actual["samePriced"] = serde_json::to_value(action).unwrap();
            }
            compared += 1;
            if let Some(diff) = wraex_golden::diff(expected, &actual) {
                failures.push(format!("{}: {diff}", case.slug));
            }
        }
        assert!(compared > 0);
        assert!(failures.is_empty(), "{} of {compared} cases differ:\n\n{}", failures.len(), failures.join("\n\n"));
    }

    /// `parseFetchResult` of each listing in EE2's recorded fetch answer
    /// (`specs/docs/fetchResponses.json`) equals `listings/poe2/display-items.json`.
    #[test]
    fn display_items_match_the_fixtures() {
        let data = data::Data::load(&data::reference_source(), "en", Some(data::reference_trade_data())).unwrap();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../ref/ee2/renderer/specs/docs/fetchResponses.json");
        let answer: FetchAnswer = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        let actual: Vec<serde_json::Value> = answer
            .result
            .iter()
            .map(|result| {
                let id = serde_json::from_value::<FetchResultId>(result.clone()).unwrap().id;
                match results::parse_fetch_result(&data, result) {
                    Ok(item) => json!({ "id": id, "displayItem": item }),
                    Err(e) => json!({ "id": id, "error": e.0 }),
                }
            })
            .collect();
        let expected = wraex_golden::listing_fixture(GameId::Poe2, "display-items.json");
        if let Some(diff) = wraex_golden::diff(&expected, &wraex_core::js::json_value(&actual)) {
            panic!("display items differ: {diff}");
        }
    }

    /// `defaultConfig()`, `initConfig()` and `getConfigForHost()` equal the
    /// recorded settings (`fixtures/config/poe2/`).
    #[test]
    fn settings_match_the_fixtures() {
        let cases = wraex_golden::config_cases(GameId::Poe2);
        assert!(cases.len() > 1);
        let mut failures = Vec::new();
        let mut check = |what: String, expected: &serde_json::Value, actual: &serde_json::Value| {
            if let Some(diff) = wraex_golden::diff(expected, &wraex_core::js::json_value(actual)) {
                failures.push(format!("{what}: {diff}"));
            }
        };
        for case in &cases {
            if let Some(default) = &case.default {
                check(format!("{} config", case.slug), default, &config::default_config());
            }
            let loaded = config::load(case.input.as_deref()).unwrap();
            check(format!("{} loaded", case.slug), &case.loaded, &loaded.config);
            check(format!("{} host", case.slug), &case.host, &config::host_config(&loaded.config).unwrap());
            // Upstream saves `JSON.stringify(AppConfig())` with its key order;
            // the contents are compared as JSON.
            let saves: Vec<serde_json::Value> = case
                .saves
                .clone()
                .and_then(|saves| serde_json::from_value::<Vec<SaveEvent>>(saves).ok())
                .unwrap_or_default()
                .iter()
                .map(|event| {
                    let contents: serde_json::Value = serde_json::from_str(&event.payload.contents).unwrap();
                    json!({ "contents": contents, "isTemporary": event.payload.is_temporary })
                })
                .collect();
            let ours: Vec<serde_json::Value> = if loaded.save_temporary {
                vec![json!({ "contents": loaded.config, "isTemporary": true })]
            } else {
                Vec::new()
            };
            check(format!("{} saves", case.slug), &json!(saves), &json!(ours));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }
}
