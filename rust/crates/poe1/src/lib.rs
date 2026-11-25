//! Path of Exile 1: parser, data loading and game-specific filters.
//!
//! Ported from `ref/apt` (Awakened PoE Trade). Ported so far: the data layer
//! ([`data`]), the parser ([`parser`]), the default filters ([`filters`]),
//! trade request building ([`trade`]), the map check ([`map_check`]), the
//! item check's actions ([`item_check`]) and Inscribed Ultimatum items
//! ([`ultimatum`]).

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
pub mod trends;
pub mod ultimatum;

use wraex_core::GameId;
use wraex_game::{Game, ParseError};

#[derive(Debug, Default, Clone, Copy)]
pub struct Poe1;

impl Game for Poe1 {
    type Data = data::Data;
    type Item = parser::item::Item;

    fn id(&self) -> GameId {
        GameId::Poe1
    }

    fn name(&self) -> &'static str {
        "Path of Exile"
    }

    fn parse_clipboard(&self, data: &data::Data, text: &str) -> Result<parser::item::Item, ParseError> {
        parser::parse_clipboard(data, text).map_err(|parser::Fail(key)| ParseError::Rejected { key: key.into() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_the_ported_game() {
        assert_eq!(Poe1.id(), GameId::Poe1);
    }

    /// `parseClipboard` output equals `parsed.json` for every PoE1 fixture.
    #[test]
    fn parsed_items_match_the_fixtures() {
        let mut langs = std::collections::HashMap::new();
        let mut failures = Vec::new();
        let cases = wraex_golden::cases(GameId::Poe1);
        assert!(!cases.is_empty());
        for case in &cases {
            let lang = case.options["lang"].as_str().unwrap().to_owned();
            let data = langs
                .entry(lang.clone())
                .or_insert_with(|| data::Data::load(&data::reference_source(), &lang).unwrap());
            let actual = match Poe1.parse_clipboard(data, &case.input) {
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

    /// `createPresets` output equals `presets.json` for every PoE1 fixture.
    #[test]
    fn presets_match_the_fixtures() {
        let mut langs = std::collections::HashMap::new();
        let mut failures = Vec::new();
        let cases = wraex_golden::cases(GameId::Poe1);
        let mut compared = 0;
        for case in &cases {
            let Some(expected) = &case.presets else { continue };
            let lang = case.options["lang"].as_str().unwrap().to_owned();
            let data = langs
                .entry(lang.clone())
                .or_insert_with(|| data::Data::load(&data::reference_source(), &lang).unwrap());
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

    /// `TradeBulk.vue`'s first (optimistic) exchange search.
    fn optimistic_have(ref_name: &str) -> Vec<String> {
        match ref_name {
            "Chaos Orb" => vec!["divine".into()],
            "Divine Orb" => vec!["chaos".into()],
            _ => vec!["divine".into(), "chaos".into()],
        }
    }

    /// Each preset's `{ api, request }` equals `trade/<id>.json`.
    #[test]
    fn trade_requests_match_the_fixtures() {
        let mut langs = std::collections::HashMap::new();
        let mut failures = Vec::new();
        let mut compared = 0;
        for case in &wraex_golden::cases(GameId::Poe1) {
            if case.presets.is_none() {
                continue;
            }
            let lang = case.options["lang"].as_str().unwrap().to_owned();
            let data = langs
                .entry(lang.clone())
                .or_insert_with(|| data::Data::load(&data::reference_source(), &lang).unwrap());
            let item = parser::parse_clipboard(data, &case.input).unwrap();
            let opts: filters::CreatePresetsOptions = serde_json::from_value(case.options["options"].clone()).unwrap();
            let presets = filters::create_presets(data, &item, &opts).unwrap();
            let have: Vec<String> = serde_json::from_value(case.options["have"].clone()).unwrap();
            assert_eq!(presets.presets.len(), case.trade.len(), "{}", case.slug);
            for preset in &presets.presets {
                let api = trade::api_to_satisfy_search(&item, &preset.stats);
                let request = match api {
                    trade::Api::Bulk => {
                        let have = if have.is_empty() { optimistic_have(&item.info().ref_name) } else { have.clone() };
                        wraex_core::js::json_value(&trade::create_bulk_request(&preset.filters, &item, have))
                    }
                    trade::Api::Trade => match trade::create_trade_request(data, &preset.filters, &preset.stats) {
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

    /// The map check's stats equal `map-check.json`, recorded for map-like items.
    #[test]
    fn map_checks_match_the_fixtures() {
        let data = data::Data::load(&data::reference_source(), "en").unwrap();
        let mut failures = Vec::new();
        let mut compared = 0;
        for case in &wraex_golden::cases(GameId::Poe1) {
            if case.parse_error().is_some() {
                continue;
            }
            let item = parser::parse_clipboard(&data, &case.input).unwrap();
            assert_eq!(map_check::is_map_like(&item), case.map_check.is_some(), "{}: isMapLike", case.slug);
            let Some(expected) = &case.map_check else { continue };
            compared += 1;
            let actual = wraex_core::js::json_value(&map_check::map_stats(&item));
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
        let data = data::Data::load(&data::reference_source(), "en").unwrap();
        let mut failures = Vec::new();
        let mut compared = 0;
        for case in &wraex_golden::cases(GameId::Poe1) {
            let Some(expected) = &case.item_check else { continue };
            let language = case.options["config"]["language"].as_str().unwrap();
            let item = parser::parse_clipboard(&data, &case.input).unwrap();
            let actual = serde_json::json!({
                "wiki": item_check::open_wiki(&item),
                "poedb": item_check::open_poedb(&item, language),
                "craftOfExile": item_check::open_coe(&item),
                "similar": item_check::find_similar_items(&item),
            });
            compared += 1;
            if let Some(diff) = wraex_golden::diff(expected, &actual) {
                failures.push(format!("{}: {diff}", case.slug));
            }
        }
        assert!(compared > 0);
        assert!(failures.is_empty(), "{} of {compared} cases differ:\n\n{}", failures.len(), failures.join("\n\n"));
    }

    /// Inscribed Ultimata are the port's own feature (upstream has no parser for them), so
    /// their filters, preset and request are asserted here rather than against a fixture.
    #[test]
    fn ultimatum_filters_presets_and_requests() {
        use filters::create_item_filters::CreateOptions;

        let data = data::Data::load(&data::reference_source(), "en").unwrap();
        let opts = filters::CreatePresetsOptions {
            filters: CreateOptions {
                league: "Standard".to_owned(),
                merchant_only: true,
                currency: None,
                collapse_listings: "api".to_owned(),
                activate_stock_filter: false,
                exact: false,
                use_en: false,
            },
            search_stat_range: 10.0,
        };
        // The preset and the request a scene would send for one item text.
        let check = |name: &str, text: &str| {
            let item = parser::parse_clipboard(&data, text).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            let presets = filters::create_presets(&data, &item, &opts).unwrap();
            assert_eq!(presets.presets.len(), 1, "{name}");
            assert_eq!(presets.active, "filters.preset_exact", "{name}");
            let preset = &presets.presets[0];
            assert_eq!(preset.id, "filters.preset_exact", "{name}");
            assert!(preset.stats.is_empty(), "{name}: the modifier lines are not trade stats");
            assert!(preset.filters.rarity.is_none(), "{name}: no rarity filter");
            assert!(preset.filters.item_level.is_none(), "{name}: no item level");
            assert!(preset.filters.area_level.is_none(), "{name}: no area level");
            assert_eq!(preset.filters.search_exact.base_type.as_deref(), Some("Inscribed Ultimatum"), "{name}");
            assert_eq!(trade::api_to_satisfy_search(&item, &preset.stats), trade::Api::Trade, "{name}: search API");
            let request = trade::create_trade_request(&data, &preset.filters, &preset.stats)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(
                serde_json::to_value(&request.query.kind).unwrap(),
                serde_json::json!("Inscribed Ultimatum"),
                "{name}"
            );
            (preset.filters.clone(), request)
        };

        let (filters, request) = check("survive-currency", include_str!("../testdata/ultimatum/survive-currency.txt"));
        let ultimatum = filters.ultimatum.as_ref().expect("ultimatum filters");
        fn fields(filter: Option<&filters::UltimatumFilter>) -> Option<(&str, &str, bool)> {
            filter.map(|filter| (filter.value.as_str(), filter.label.as_str(), filter.disabled))
        }
        assert_eq!(fields(ultimatum.challenge.as_ref()), Some(("Survival", "Survive", true)));
        assert_eq!(fields(ultimatum.reward.as_ref()), Some(("DoubleCurrency", "Currency", false)));
        assert_eq!(fields(ultimatum.input.as_ref()), Some(("Vaal Orb", "Vaal Orb", false)));
        assert!(ultimatum.output.is_none());
        let ultimatum_filters = |request: &trade::TradeRequest| request.query.filters.get("ultimatum_filters").cloned();
        assert_eq!(
            ultimatum_filters(&request),
            Some(serde_json::json!({"filters": {
                "ultimatum_reward": {"option": "DoubleCurrency"},
                "ultimatum_input": {"option": "Vaal Orb"},
            }}))
        );

        // The challenge chip starts off; ticking it puts the challenge in the request too.
        let mut with_challenge = filters.clone();
        with_challenge.ultimatum.as_mut().unwrap().challenge.as_mut().unwrap().disabled = false;
        let request = trade::create_trade_request(&data, &with_challenge, &[]).unwrap();
        assert_eq!(
            ultimatum_filters(&request),
            Some(serde_json::json!({"filters": {
                "ultimatum_challenge": {"option": "Survival"},
                "ultimatum_reward": {"option": "DoubleCurrency"},
                "ultimatum_input": {"option": "Vaal Orb"},
            }}))
        );

        let (filters, request) =
            check("survive-unique-swap", include_str!("../testdata/ultimatum/survive-unique-swap.txt"));
        let ultimatum = filters.ultimatum.as_ref().expect("ultimatum filters");
        assert_eq!(fields(ultimatum.challenge.as_ref()), Some(("Survival", "Survive", true)));
        assert_eq!(fields(ultimatum.reward.as_ref()), Some(("ExchangeUnique", "Unique Item", false)));
        assert_eq!(fields(ultimatum.input.as_ref()), Some(("Prospero's Protection", "Prospero's Protection", false)));
        assert_eq!(fields(ultimatum.output.as_ref()), Some(("Grasping Nightshade", "Grasping Nightshade", false)));
        assert_eq!(
            ultimatum_filters(&request),
            Some(serde_json::json!({"filters": {
                "ultimatum_reward": {"option": "ExchangeUnique"},
                "ultimatum_input": {"option": "Prospero's Protection"},
                "ultimatum_output": {"option": "Grasping Nightshade"},
            }}))
        );

        let (filters, request) = check("waves-div-cards", include_str!("../testdata/ultimatum/waves-div-cards.txt"));
        let ultimatum = filters.ultimatum.as_ref().expect("ultimatum filters");
        assert_eq!(fields(ultimatum.challenge.as_ref()), Some(("Exterminate", "Defeat waves of enemies", true)));
        assert_eq!(fields(ultimatum.reward.as_ref()), Some(("DoubleDivCards", "Divination Cards", false)));
        assert_eq!(fields(ultimatum.input.as_ref()), Some(("Monochrome", "Monochrome", false)));
        assert!(ultimatum.output.is_none());
        assert_eq!(
            ultimatum_filters(&request),
            Some(serde_json::json!({"filters": {
                "ultimatum_reward": {"option": "DoubleDivCards"},
                "ultimatum_input": {"option": "Monochrome"},
            }}))
        );
    }

    /// `defaultConfig()`, `initConfig()` and `getConfigForHost()` equal the
    /// recorded settings (`fixtures/config/poe1/`).
    #[test]
    fn settings_match_the_fixtures() {
        let cases = wraex_golden::config_cases(GameId::Poe1);
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
                .as_ref()
                .and_then(|s| s.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .map(|s| {
                    let contents: serde_json::Value =
                        serde_json::from_str(s["payload"]["contents"].as_str().unwrap()).unwrap();
                    serde_json::json!({ "contents": contents, "isTemporary": s["payload"]["isTemporary"] })
                })
                .collect();
            let ours: Vec<serde_json::Value> = if loaded.save_temporary {
                vec![serde_json::json!({ "contents": loaded.config, "isTemporary": true })]
            } else {
                Vec::new()
            };
            check(format!("{} saves", case.slug), &serde_json::json!(saves), &serde_json::json!(ours));
        }
        assert!(failures.is_empty(), "{}", failures.join("\n\n"));
    }
}
