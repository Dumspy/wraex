//! `ref/ee2/renderer/src/web/background/Prices.ts`: poe.ninja prices in
//! divines, shown in the chosen core currency (exalted or chaos).
//!
//! Upstream downloads three files from its own server
//! (`api.exiledexchange2.dev`: the overview blob, poe.ninja's namespace map and
//! `item-drop.json`); where Wraex gets them is not decided, so this takes their
//! text ([`Prices::loaded`]) and leaves the download out. Lookup works like
//! PoE1's: the blob is searched as text for the start of an item's JSON
//! object (`name`, `variant`, `primaryValue` come first, in that order).

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wraex_core::js::{json_string, number_to_string, to_fixed};
use wraex_core::leagues::SelectedLeague;

use crate::data::{Data, Namespace};

pub const RETRY_INTERVAL_MS: u64 = 4 * 60 * 1000;
pub const UPDATE_INTERVAL_MS: u64 = 31 * 60 * 1000;
pub const INTEREST_SPAN_MS: u64 = 20 * 60 * 1000;

/// `NinjaSchemaV1`: which overviews hold which namespace.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NinjaSchema {
    pub schema_version: u32,
    pub map: Vec<SchemaEntry>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SchemaEntry {
    /// `ITEM`, `GEM` or `UNIQUE`.
    pub ns: String,
    /// From the currency exchange.
    pub cx: bool,
    pub url: String,
    #[serde(rename = "type")]
    pub kind: String,
}

/// `NinjaXchgRates`: divines in each core currency.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NinjaXchgRates {
    pub rates: HashMap<String, f64>,
    pub primary: String,
    pub secondary: String,
}

/// `NinjaDenseExchangeInfo | NinjaDenseStashInfo`, the fields both have.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NinjaDenseInfo {
    pub name: String,
    #[serde(default)]
    pub variant: Option<String>,
    /// In divines.
    pub primary_value: f64,
    pub details_id: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub volume_primary_value: Option<f64>,
    #[serde(default)]
    pub max_volume_currency: Option<String>,
    #[serde(default)]
    pub max_volume_rate: Option<f64>,
    pub sparkline: Sparkline,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sparkline {
    pub total_change: f64,
    pub data: Vec<Option<f64>>,
}

/// A found price, its source and poe.ninja page.
#[derive(Debug, Clone, PartialEq)]
pub struct NinjaPrice {
    pub info: NinjaDenseInfo,
    pub cx: bool,
    pub url: String,
}

/// `DbQuery`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbQuery {
    pub ns: String,
    pub name: String,
    pub variant: Option<String>,
}

/// `CurrencyValue`
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CurrencyValue {
    pub min: f64,
    pub max: f64,
    /// `chaos`, `exalted` or `div`.
    pub currency: &'static str,
}

/// `CoreCurrency`
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CoreCurrency {
    /// `exalted`, `chaos` or `div`.
    pub id: &'static str,
    pub abbrev: &'static str,
    #[serde(rename = "ref")]
    pub ref_name: &'static str,
    pub text: String,
    pub icon: &'static str,
}

/// `DivCurrency`
pub fn div_currency() -> CoreCurrency {
    CoreCurrency {
        id: "div",
        abbrev: "div",
        ref_name: "Divine Orb",
        text: "Divine Orb".into(),
        icon: "/images/div.png",
    }
}

/// `getAvailableCoreCurrencies`, named in the data's language.
fn available_core_currencies(d: &Data) -> Vec<CoreCurrency> {
    [("exalted", "ex", "Exalted Orb", "/images/exa.png"), ("chaos", "c", "Chaos Orb", "/images/chaos.png")]
        .into_iter()
        .map(|(id, abbrev, ref_name, icon)| CoreCurrency {
            id,
            abbrev,
            ref_name,
            text: d
                .item_by_ref(Namespace::Item, ref_name)
                .and_then(|v| v.into_iter().next())
                .and_then(|b| b.name)
                .unwrap_or_else(|| ref_name.to_owned()),
            icon,
        })
        .collect()
}

/// One overview of the blob.
#[derive(Debug, Clone, PartialEq)]
struct Overview {
    ns: String,
    cx: bool,
    url: String,
    lines: String,
}

/// `parseXchg`: the `{"rates":...}` object at the start of the blob.
pub fn parse_xchg(blob: &str) -> Option<NinjaXchgRates> {
    const RATES: &str = r#"{"rates":"#;
    const END_RATES: &str = "\"}";
    let start = blob.find(RATES)?;
    let end = blob[start..].find(END_RATES)? + start + END_RATES.len();
    serde_json::from_str(&blob[start..end]).ok()
}

/// `splitJsonBlob`: the blob cut at each `{"type":"`, the schema's types kept.
fn split_json_blob(blob: &str, schema: &NinjaSchema) -> Vec<Overview> {
    const NINJA_OVERVIEW: &str = r#"{"type":""#;
    let mut database = Vec::new();
    if schema.schema_version != 1 {
        return database;
    }
    let Some(mut start) = blob.find(NINJA_OVERVIEW) else { return database };
    loop {
        let end = blob[start + 1..].find(NINJA_OVERVIEW).map(|i| i + start + 1);
        let type_start = start + NINJA_OVERVIEW.len();
        let kind = blob[type_start..].find('"').map_or(&blob[type_start..], |i| &blob[type_start..type_start + i]);
        let lines = &blob[start..end.unwrap_or(blob.len())];
        if let Some(entry) = schema.map.iter().find(|e| e.kind == kind) {
            database.push(Overview {
                ns: entry.ns.clone(),
                cx: entry.cx,
                url: entry.url.clone(),
                lines: lines.to_owned(),
            });
        }
        match end {
            Some(end) => start = end,
            None => break,
        }
    }
    database
}

/// `selectedLeagueToUrl(false)`: poe.ninja's name for a league.
pub fn league_to_url(league: &str) -> String {
    match league {
        "Standard" => "standard".to_owned(),
        "Hardcore" => "hardcore".to_owned(),
        _ => {
            let id = league.to_lowercase().replace(' ', "");
            match id.strip_prefix("hc") {
                Some(rest) => format!("{rest}hc"),
                None => id,
            }
        }
    }
}

/// `displayRounding`, with `truncateLargeNumbers` shortening to k/m/b/t.
pub fn display_rounding(value: f64, fraction: bool, truncate_large_numbers: bool) -> String {
    let short = |v: f64| {
        let n: f64 = to_fixed(v, 1).parse().unwrap_or(f64::NAN);
        number_to_string(n).replace('.', "\u{200A}.\u{200A}")
    };
    if fraction && value.abs() < 1.0 {
        if value == 0.0 {
            return "0".to_owned();
        }
        let r = format!("1\u{200A}/\u{200A}{}", display_rounding(1.0 / value, false, false));
        return if r == "1\u{200A}/\u{200A}1" { "1".to_owned() } else { r };
    }
    if value.abs() < 10.0 {
        return short(value);
    }
    if truncate_large_numbers && value.abs() > 2250.0 {
        let a = value.abs();
        return if a < 1_050_000.0 {
            short(value / 1e3) + "k"
        } else if a < 1_050_000_000.0 {
            short(value / 1e6) + "m"
        } else if a < 1_050_000_000_000.0 {
            short(value / 1e9) + "b"
        } else {
            short(value / 1e12) + "t"
        };
    }
    // `Math.round`: halves toward +∞.
    number_to_string((value + 0.5).floor())
}

/// A comparison for [`Prices::compare_price`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceOp {
    Gt,
    Ge,
    /// `==`: the ranges are the same single value.
    Eq,
    /// `===`: also the same currency.
    StrictEq,
    Le,
    Lt,
}

/// Prices for the selected league (`usePoeninja`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prices {
    database: Vec<Overview>,
    /// Core currency per divine.
    pub xchg_rate: Option<f64>,
    /// The core currency `xchg_rate` is in.
    pub xchg_rate_currency: Option<&'static str>,
    pub available_core_currencies: Vec<CoreCurrency>,
    /// The price-check widget's `coreCurrency`.
    pub core_currency: Option<String>,
    league_id: Option<String>,
    last_update: Option<u64>,
    last_interest: Option<u64>,
    price_cache: HashMap<String, CurrencyValue>,
}

impl Prices {
    /// Whether to download prices for `league` now (`load`): a public pc-ggg
    /// league, and either `force`, a new league, or an old download someone
    /// is still interested in.
    pub fn wants_load(&mut self, league: Option<&SelectedLeague>, force: bool, now: u64) -> bool {
        let Some(league) = league.filter(|l| l.is_popular && l.realm == "pc-ggg") else { return false };
        if self.league_id.as_deref() != Some(league.id.as_str()) {
            // `watch(leagues.selectedId)`
            self.xchg_rate = None;
            self.database.clear();
            self.league_id = Some(league.id.clone());
            return true;
        }
        let since = |t: Option<u64>| t.map_or(u64::MAX, |t| now.saturating_sub(t));
        force || (since(self.last_update) >= UPDATE_INTERVAL_MS && since(self.last_interest) <= INTEREST_SPAN_MS)
    }

    /// `queuePricesFetch`: the user looked at a price; keep prices fresh.
    pub fn mark_interest(&mut self, now: u64) {
        self.last_interest = Some(now);
    }

    /// Takes the downloaded schema and overview blob (`load` after the
    /// requests). A preferred core currency the data lacks falls back to exalted.
    pub fn loaded(&mut self, d: &Data, schema: &NinjaSchema, blob: &str, now: u64) {
        self.available_core_currencies = available_core_currencies(d);
        let selected_id = self.selected_core_currency_id();
        if !self.available_core_currencies.iter().any(|c| c.id == selected_id) {
            self.core_currency = Some("exalted".to_owned());
        }

        let xchg = parse_xchg(blob);
        self.database = split_json_blob(blob, schema);

        // TODO (upstream): search the requested currency instead of divine.
        if let Some(rates) = xchg.map(|x| x.rates).filter(|r| r.values().any(|&v| v >= 10.0)) {
            let preferred = self.selected_core_currency().map(|c| c.id);
            match preferred.filter(|&p| p != "div" && rates.get(p).is_some_and(|&r| r >= 5.0)) {
                Some(id) => {
                    self.xchg_rate = rates.get(id).copied();
                    self.xchg_rate_currency = Some(id);
                }
                None => {
                    self.xchg_rate = rates.get("exalted").copied();
                    self.xchg_rate_currency = Some("exalted");
                }
            }
        }

        self.price_cache.clear();
        self.last_update = Some(now);
    }

    /// `selectedCoreCurrencyId`
    pub fn selected_core_currency_id(&self) -> &str {
        if self.available_core_currencies.is_empty() {
            "exalted"
        } else {
            self.core_currency.as_deref().unwrap_or("exalted")
        }
    }

    /// `selectedCoreCurrency`
    pub fn selected_core_currency(&self) -> Option<&CoreCurrency> {
        let id = self.core_currency.as_deref()?;
        self.available_core_currencies.iter().find(|c| c.id == id)
    }

    /// `initialLoading`: nothing to show yet.
    pub fn is_empty(&self) -> bool {
        self.database.is_empty()
    }

    /// `findPriceByQuery`
    pub fn find_price(&self, query: &DbQuery) -> Option<NinjaPrice> {
        // `JSON.stringify({ name, variant, primaryValue: 0 }).replace(':0}', ':')`
        let mut search = format!("{{\"name\":{}", json_string(&query.name));
        if let Some(v) = &query.variant {
            search.push_str(&format!(",\"variant\":{}", json_string(v)));
        }
        search.push_str(",\"primaryValue\":");
        const END: &str = "}}";

        for overview in self.database.iter().filter(|o| o.ns == query.ns) {
            let Some(start) = overview.lines.find(&search) else { continue };
            let end = overview.lines[start..].find(END).map(|i| start + i + END.len())?;
            let info: NinjaDenseInfo = serde_json::from_str(&overview.lines[start..end]).ok()?;
            let league = league_to_url(self.league_id.as_deref().unwrap_or_default());
            let url = format!("https://poe.ninja/poe2/economy/{league}/{}/{}", overview.url, info.details_id);
            return Some(NinjaPrice { info, cx: overview.cx, url });
        }
        None
    }

    fn rate(&self) -> f64 {
        self.xchg_rate.filter(|r| *r != 0.0).unwrap_or(9999.0)
    }

    /// `divineToCore`
    pub fn divine_to_core(&self, count: f64) -> f64 {
        count * self.rate()
    }

    /// `autoCurrency(value)`: a divine value in the core currency, or in
    /// divines once it's worth one (unless `core_only`).
    pub fn auto_currency(&self, value: f64, core_only: bool) -> CurrencyValue {
        let core = self.divine_to_core(value);
        let rate = self.rate();
        if core > rate * 0.94 && !core_only {
            if core < rate * 1.06 {
                return CurrencyValue { min: 1.0, max: 1.0, currency: "div" };
            }
            return CurrencyValue { min: value, max: value, currency: "div" };
        }
        match self.selected_core_currency() {
            Some(c) => CurrencyValue { min: core, max: core, currency: c.id },
            // Shouldn't happen once a currency has loaded.
            None => CurrencyValue { min: value, max: value, currency: "div" },
        }
    }

    /// `autoCurrency([min, max])`
    pub fn auto_currency_range(&self, min: f64, max: f64, core_only: bool) -> CurrencyValue {
        let (core_min, core_max) = (self.divine_to_core(min), self.divine_to_core(max));
        if core_min > self.rate() && !core_only {
            return CurrencyValue { min, max, currency: "div" };
        }
        match self.selected_core_currency() {
            Some(c) => CurrencyValue { min: core_min, max: core_max, currency: c.id },
            None => CurrencyValue { min, max, currency: "div" },
        }
    }

    /// `cachedCurrencyByQuery`: `count` of an item, priced and cached.
    pub fn cached_currency_by_query(&mut self, query: &DbQuery, count: f64) -> Option<CurrencyValue> {
        let key = format!(
            "{}:{}:{}:{}",
            query.ns,
            query.name,
            query.variant.as_deref().unwrap_or(""),
            number_to_string(count)
        );
        if let Some(value) = self.price_cache.get(&key) {
            return Some(*value);
        }
        let price = self.find_price(query)?;
        let value = self.auto_currency(price.info.primary_value * count, false);
        self.price_cache.insert(key, value);
        Some(value)
    }

    /// `toDivine`: throws when the value is in a core currency the rate isn't for.
    pub fn to_divine(&self, value: CurrencyValue) -> Result<CurrencyValue, String> {
        if value.currency == "div" {
            return Ok(value);
        }
        let Some(rate) = self.xchg_rate.filter(|r| *r != 0.0) else { return Ok(value) };
        if self.xchg_rate_currency != Some(value.currency) {
            return Err(format!(
                "Cannot convert {} to div, current core currency is {}",
                value.currency,
                self.xchg_rate_currency.unwrap_or("undefined")
            ));
        }
        Ok(CurrencyValue { min: value.min / rate, max: value.max / rate, currency: "div" })
    }

    /// `addPrice`
    pub fn add_price(&self, a: CurrencyValue, b: CurrencyValue) -> Result<CurrencyValue, String> {
        let (a, b) = (self.to_divine(a)?, self.to_divine(b)?);
        Ok(self.auto_currency_range(a.min + b.min, a.max + b.max, false))
    }

    /// `subtractPrice`
    pub fn subtract_price(&self, a: CurrencyValue, b: CurrencyValue) -> Result<CurrencyValue, String> {
        let (a, b) = (self.to_divine(a)?, self.to_divine(b)?);
        Ok(self.auto_currency_range(a.min - b.max, a.max - b.min, false))
    }

    /// `comparePrice`
    pub fn compare_price(&self, left: CurrencyValue, op: PriceOp, right: CurrencyValue) -> Result<bool, String> {
        let (l, r) = (self.to_divine(left)?, self.to_divine(right)?);
        Ok(match op {
            PriceOp::Gt => l.min > r.max,
            PriceOp::Ge => l.min >= r.max,
            PriceOp::Eq => l.min == r.max && l.max == r.min,
            PriceOp::StrictEq => left.currency == right.currency && l.min == r.max && l.max == r.min,
            PriceOp::Le => l.max <= r.min,
            PriceOp::Lt => l.max < r.min,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::tests::en;

    /// EE2's `specs/web/prices.test.ts` inputs.
    const XCHG_BLOB: &str = r#"{"core":{"rates":{"exalted":2312,"chaos":76.2},"primary":"divine","secondary":"chaos"},"itemOverviews":[{"type":"Currency","lines":[{"name":"Orb of Alchemy","detailsId":"orb-of-alchemy","id":"alch","primaryValue":0.00003575,"volumePrimaryValue":0.4008,"maxVolumeCurrency":"exalted","#;
    const SPLIT_BLOB: &str = r#"chaos"},"itemOverviews":[{"type":"Currency","lines":[{"name":"Orb of Alchemy","detailsId":"orb-of-alchemy","id":"alch","primaryValue":0.00003575,"volumePrimaryValue":0.4008,"maxVolumeCurrency":"exalted","maxVolumeRate":12.05,"sparkline":{"totalChange":-40.89,"data":[-9.56,-69.14,-70.07,-20.05,-14.13,18.75,-40.89]}},{"name":"Exalted Orb","detailsId":"exalted-orb","id":"exalted","primaryValue":0.0004308,"volumePrimaryValue":449.7,"maxVolumeCurrency":"divine","maxVolumeRate":2321,"sparkline":{"totalChange":-20.32,"data":[-5.22,-15.82,-13.6,-15.2,-15.99,-21.78,-20.32]}}]}]"#;

    fn schema() -> NinjaSchema {
        NinjaSchema {
            schema_version: 1,
            map: vec![SchemaEntry { ns: "ITEM".into(), cx: true, url: "my-url".into(), kind: "Currency".into() }],
        }
    }

    #[test]
    fn parses_the_rates_like_ee2s_test() {
        let x = parse_xchg(XCHG_BLOB).unwrap();
        assert_eq!((x.rates["exalted"], x.rates["chaos"]), (2312.0, 76.2));
        assert_eq!((x.primary.as_str(), x.secondary.as_str()), ("divine", "chaos"));
        assert_eq!(split_json_blob(SPLIT_BLOB, &schema()).len(), 1);
    }

    #[test]
    fn prices_convert_through_the_core_currency() {
        let d = en();
        let mut prices = Prices { core_currency: Some("exalted".into()), ..Default::default() };
        let league = SelectedLeague { id: "Dawn of the Hunt".into(), realm: "pc-ggg".into(), is_popular: true };
        assert!(prices.wants_load(Some(&league), false, 0));
        // poe.ninja's own order (`name`, `detailsId`, ..., `primaryValue`) never
        // matches the search; upstream relies on its server writing
        // `primaryValue` right after `name`, as here.
        assert!({
            let mut p = prices.clone();
            p.loaded(d, &schema(), &format!("{XCHG_BLOB}{SPLIT_BLOB}"), 0);
            p.find_price(&DbQuery { ns: "ITEM".into(), name: "Exalted Orb".into(), variant: None }).is_none()
        });
        let blob = concat!(
            r#"{"core":{"rates":{"exalted":2312,"chaos":76.2},"primary":"divine","secondary":"chaos"},"itemOverviews":["#,
            r#"{"type":"Currency","lines":[{"name":"Exalted Orb","primaryValue":0.0004308,"detailsId":"exalted-orb","#,
            r#""sparkline":{"totalChange":-20.32,"data":[-5.22,null]}}]}]}"#
        );
        prices.loaded(d, &schema(), blob, 0);
        assert_eq!((prices.xchg_rate, prices.xchg_rate_currency), (Some(2312.0), Some("exalted")));

        let ex = prices.find_price(&DbQuery { ns: "ITEM".into(), name: "Exalted Orb".into(), variant: None }).unwrap();
        assert_eq!(ex.url, "https://poe.ninja/poe2/economy/dawnofthehunt/my-url/exalted-orb");
        assert!(ex.cx);

        // 100 exalted orbs, in exalted.
        let query = DbQuery { ns: "ITEM".into(), name: "Exalted Orb".into(), variant: None };
        let value = prices.cached_currency_by_query(&query, 100.0).unwrap();
        assert_eq!(value.currency, "exalted");
        assert!((value.min - 0.0004308 * 100.0 * 2312.0).abs() < 1e-9);
        // A divine's worth switches to divines.
        assert_eq!(prices.auto_currency(1.0, false), CurrencyValue { min: 1.0, max: 1.0, currency: "div" });
        assert_eq!(prices.auto_currency(3.0, false).currency, "div");
        assert_eq!(prices.auto_currency(3.0, true).currency, "exalted");

        let one_div = CurrencyValue { min: 1.0, max: 1.0, currency: "div" };
        let half = CurrencyValue { min: 1156.0, max: 1156.0, currency: "exalted" };
        assert_eq!(prices.add_price(one_div, half).unwrap().currency, "div");
        assert!(prices.compare_price(one_div, PriceOp::Gt, half).unwrap());
        let chaos = CurrencyValue { min: 1.0, max: 1.0, currency: "chaos" };
        assert!(prices.to_divine(chaos).is_err());
    }

    #[test]
    fn display_rounding_shortens_large_numbers() {
        assert_eq!(display_rounding(12_345.0, false, true), "12\u{200A}.\u{200A}3k");
        assert_eq!(display_rounding(2_000_000.0, false, true), "2m");
        assert_eq!(display_rounding(2_000.0, false, true), "2000");
        assert_eq!(league_to_url("HC Dawn of the Hunt"), "dawnofthehunthc");
        assert_eq!(league_to_url("Standard"), "standard");
    }
}
