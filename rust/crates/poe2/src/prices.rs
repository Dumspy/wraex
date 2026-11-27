//! `ref/ee2/renderer/src/web/background/Prices.ts`: poe.ninja prices in
//! divines, shown in the chosen core currency (exalted or chaos).
//!
//! **Deviation:** upstream downloads a ready-made blob, poe.ninja's namespace
//! map and `item-drop.json` from its own server (`api.exiledexchange2.dev`) and
//! finds items in the blob by searching its text. Wraex asks poe.ninja
//! directly, one exchange overview per [`NINJA_TYPES`] entry, and looks items
//! up by name in the parsed answers. The prices, rates, currency choice and
//! refresh timing follow upstream. poe.ninja has no PoE2 unique overview, so
//! (as with upstream's data) uniques have no price here.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use wraex_core::js::{number_to_string, to_fixed};
use wraex_core::leagues::SelectedLeague;
use wraex_core::trade::api::{HttpRequest, Method};

use crate::data::{Data, Namespace};

pub const RETRY_INTERVAL_MS: u64 = 4 * 60 * 1000;
pub const UPDATE_INTERVAL_MS: u64 = 31 * 60 * 1000;
pub const INTEREST_SPAN_MS: u64 = 20 * 60 * 1000;

/// poe.ninja's PoE2 exchange overviews (`type`), the namespace the app looks
/// their items up in, and their page on poe.ninja.
///
/// Upstream's namespace map comes from its server; this list is poe.ninja's
/// non-empty types (checked 2026-11-24). The page slugs follow poe.ninja's
/// naming but are unconfirmed: its site refuses scripted requests.
pub const NINJA_TYPES: [(&str, &str, &str); 14] = [
    ("Currency", "ITEM", "currency"),
    ("Fragments", "ITEM", "fragments"),
    ("Runes", "ITEM", "runes"),
    ("Essences", "ITEM", "essences"),
    ("SoulCores", "ITEM", "soul-cores"),
    ("Idols", "ITEM", "idols"),
    ("UncutGems", "ITEM", "uncut-gems"),
    ("LineageSupportGems", "GEM", "lineage-support-gems"),
    ("Expedition", "ITEM", "expedition"),
    ("Ritual", "ITEM", "ritual"),
    ("Breach", "ITEM", "breach"),
    ("Delirium", "ITEM", "delirium"),
    ("Abyss", "ITEM", "abyss"),
    ("Verisium", "ITEM", "verisium"),
];

/// The request for one overview of `league`.
pub fn overview_request(league: &str, kind: &str) -> HttpRequest {
    let league: String = league.chars().map(|c| if c == ' ' { "%20".to_owned() } else { c.to_string() }).collect();
    HttpRequest {
        method: Method::Get,
        url: format!("https://poe.ninja/poe2/api/economy/exchange/current/overview?league={league}&type={kind}"),
        headers: Vec::new(),
        body: None,
    }
}

/// An exchange overview as poe.ninja answers it: prices by currency id, and
/// the currencies' names.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExchangeOverview {
    pub core: OverviewCore,
    pub lines: Vec<ExchangeLine>,
    pub items: Vec<ExchangeItem>,
}

/// `core`: divines in each core currency (`NinjaXchgRates`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OverviewCore {
    pub rates: HashMap<String, f64>,
    pub primary: String,
    pub secondary: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeLine {
    pub id: String,
    pub primary_value: f64,
    #[serde(default)]
    pub volume_primary_value: Option<f64>,
    #[serde(default)]
    pub max_volume_currency: Option<String>,
    #[serde(default)]
    pub max_volume_rate: Option<f64>,
    pub sparkline: Sparkline,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeItem {
    pub id: String,
    pub name: String,
    pub details_id: String,
}

/// `NinjaDenseExchangeInfo`: a line joined with its item.
#[derive(Debug, Clone, PartialEq)]
pub struct NinjaDenseInfo {
    pub name: String,
    pub variant: Option<String>,
    /// In divines.
    pub primary_value: f64,
    pub details_id: String,
    pub id: Option<String>,
    pub volume_primary_value: Option<f64>,
    pub max_volume_currency: Option<String>,
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

/// One overview's prices (`PriceDatabase` entry).
#[derive(Debug, Clone, PartialEq)]
struct Overview {
    ns: &'static str,
    /// From the currency exchange.
    cx: bool,
    url: &'static str,
    entries: Vec<NinjaDenseInfo>,
}

/// `selectedLeagueToUrl(false)`: poe.ninja's name for a league.
pub fn league_to_url(league: &str) -> String {
    match league {
        "Standard" => "standard".to_owned(),
        "Hardcore" => "hardcore".to_owned(),
        _ => {
            let id = league.to_lowercase().replace(' ', "");
            let Some(rest) = id.strip_prefix("hc") else { return id };
            format!("{rest}hc")
        }
    }
}

/// `displayRounding`, with `truncateLargeNumbers` shortening to k/m/b/t.
#[expect(
    clippy::fn_params_excessive_bools,
    reason = "upstream's `displayRounding(value, fraction, truncateLargeNumbers)` signature, kept for its callers"
)]
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
    pub const fn mark_interest(&mut self, now: u64) {
        self.last_interest = Some(now);
    }

    /// Takes poe.ninja's answers: `(type, body)` per [`NINJA_TYPES`] entry
    /// (`load` after the requests). Unparsable or unknown ones are skipped. A
    /// preferred core currency the data lacks falls back to exalted.
    pub fn loaded(&mut self, d: &Data, overviews: &[(&str, &str)], now: u64) {
        let parsed = overviews.iter().filter_map(|(kind, body)| {
            let overview = serde_json::from_str::<ExchangeOverview>(body).ok()?;
            let kind = NINJA_TYPES.iter().find(|(t, _, _)| t == kind).map(|(t, _, _)| *t)?;
            Some((kind, overview))
        });
        self.apply_overviews(d, parsed.collect(), now);
    }

    /// Upstream's own price database: the `overviewData.json` its server hands out, which
    /// lists the dense info per line instead of poe.ninja's separate `items` array.
    ///
    /// Wraex asks poe.ninja directly, so this reads the answer the screenshot harness
    /// recorded (`corpus/screens/poe2/proxy`) and nothing else. False when it does not
    /// parse; overview types the app does not ask poe.ninja for are skipped.
    pub fn loaded_upstream(&mut self, d: &Data, league: &str, body: &str, now: u64) -> bool {
        #[derive(Deserialize)]
        struct Upstream {
            core: OverviewCore,
            #[serde(rename = "itemOverviews")]
            item_overviews: Vec<UpstreamOverview>,
        }

        #[derive(Deserialize)]
        struct UpstreamOverview {
            #[serde(rename = "type")]
            kind: String,
            lines: Vec<UpstreamLine>,
        }

        /// A line with its item's name and details id on it (poe.ninja keeps them apart).
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct UpstreamLine {
            id: String,
            name: String,
            details_id: String,
            primary_value: f64,
            #[serde(default)]
            volume_primary_value: Option<f64>,
            #[serde(default)]
            max_volume_currency: Option<String>,
            #[serde(default)]
            max_volume_rate: Option<f64>,
            sparkline: Sparkline,
        }

        let Ok(blob) = serde_json::from_str::<Upstream>(body) else { return false };
        if self.league_id.as_deref() != Some(league) {
            self.xchg_rate = None;
            self.database.clear();
            self.league_id = Some(league.to_owned());
        }
        let overviews = blob
            .item_overviews
            .into_iter()
            .filter_map(|overview| {
                let kind = NINJA_TYPES.iter().find(|(t, _, _)| *t == overview.kind).map(|(t, _, _)| *t)?;
                let mut items = Vec::with_capacity(overview.lines.len());
                let mut lines = Vec::with_capacity(overview.lines.len());
                for line in overview.lines {
                    items.push(ExchangeItem { id: line.id.clone(), name: line.name, details_id: line.details_id });
                    lines.push(ExchangeLine {
                        id: line.id,
                        primary_value: line.primary_value,
                        volume_primary_value: line.volume_primary_value,
                        max_volume_currency: line.max_volume_currency,
                        max_volume_rate: line.max_volume_rate,
                        sparkline: line.sparkline,
                    });
                }
                Some((kind, ExchangeOverview { core: blob.core.clone(), lines, items }))
            })
            .collect();
        self.apply_overviews(d, overviews, now);
        true
    }

    /// `load`'s body once the answers are parsed: rebuild the database from them.
    fn apply_overviews(&mut self, d: &Data, overviews: Vec<(&'static str, ExchangeOverview)>, now: u64) {
        self.available_core_currencies = available_core_currencies(d);
        let selected_id = self.selected_core_currency_id();
        if !self.available_core_currencies.iter().any(|c| c.id == selected_id) {
            self.core_currency = Some("exalted".to_owned());
        }

        let mut rates = None;
        self.database.clear();
        for (kind, overview) in overviews {
            let Some(&(_, ns, url)) = NINJA_TYPES.iter().find(|(t, _, _)| *t == kind) else { continue };
            rates.get_or_insert_with(|| overview.core.rates.clone());
            let entries = overview
                .lines
                .into_iter()
                .filter_map(|line| {
                    let item = overview.items.iter().find(|i| i.id == line.id)?;
                    Some(NinjaDenseInfo {
                        name: item.name.clone(),
                        variant: None,
                        primary_value: line.primary_value,
                        details_id: item.details_id.clone(),
                        id: Some(line.id),
                        volume_primary_value: line.volume_primary_value,
                        max_volume_currency: line.max_volume_currency,
                        max_volume_rate: line.max_volume_rate,
                        sparkline: line.sparkline,
                    })
                })
                .collect();
            self.database.push(Overview { ns, cx: true, url, entries });
        }

        // TODO (upstream): search the requested currency instead of divine.
        if let Some(rates) = rates.filter(|r| r.values().any(|&v| v >= 10.0)) {
            let preferred = self.selected_core_currency().map(|c| c.id);
            if let Some(id) = preferred.filter(|&p| p != "div" && rates.get(p).is_some_and(|&r| r >= 5.0)) {
                self.xchg_rate = rates.get(id).copied();
                self.xchg_rate_currency = Some(id);
            } else {
                self.xchg_rate = rates.get("exalted").copied();
                self.xchg_rate_currency = Some("exalted");
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
    pub const fn is_empty(&self) -> bool {
        self.database.is_empty()
    }

    /// `findPriceByQuery`: the first overview of the namespace with the item.
    pub fn find_price(&self, query: &DbQuery) -> Option<NinjaPrice> {
        for overview in self.database.iter().filter(|o| o.ns == query.ns) {
            let Some(info) = overview.entries.iter().find(|e| e.name == query.name && e.variant == query.variant)
            else {
                continue;
            };
            let league = league_to_url(self.league_id.as_deref().unwrap_or_default());
            let url = format!("https://poe.ninja/poe2/economy/{league}/{}/{}", overview.url, info.details_id);
            return Some(NinjaPrice { info: info.clone(), cx: overview.cx, url });
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
        self.selected_core_currency().map_or_else(
            // Shouldn't happen once a currency has loaded.
            || CurrencyValue { min: value, max: value, currency: "div" },
            |c| CurrencyValue { min: core, max: core, currency: c.id },
        )
    }

    /// `autoCurrency([min, max])`
    pub fn auto_currency_range(&self, min: f64, max: f64, core_only: bool) -> CurrencyValue {
        let (core_min, core_max) = (self.divine_to_core(min), self.divine_to_core(max));
        if core_min > self.rate() && !core_only {
            return CurrencyValue { min, max, currency: "div" };
        }
        self.selected_core_currency().map_or_else(
            || CurrencyValue { min, max, currency: "div" },
            |c| CurrencyValue { min: core_min, max: core_max, currency: c.id },
        )
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
    #[expect(
        clippy::float_cmp,
        clippy::suspicious_operation_groupings,
        reason = "upstream's `==` compares `min` with `max` on each side"
    )]
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

    /// The shape of poe.ninja's answer (2026-11-24), cut down.
    const CURRENCY: &str = r#"{"core":{"items":[],"rates":{"exalted":132.4,"chaos":1.86},"primary":"divine","secondary":"chaos"},
        "lines":[{"id":"exalted","primaryValue":0.00755,"volumePrimaryValue":4.2,"maxVolumeCurrency":"divine","maxVolumeRate":132,
                  "sparkline":{"totalChange":-1.5,"data":[-1,null]}},
                 {"id":"lost","primaryValue":1,"sparkline":{"totalChange":0,"data":[]}}],
        "items":[{"id":"exalted","name":"Exalted Orb","image":"/x.png","category":"Currency","detailsId":"exalted-orb"}]}"#;
    const GEMS: &str = r#"{"core":{"items":[],"rates":{"exalted":132.4,"chaos":1.86},"primary":"divine","secondary":"chaos"},
        "lines":[{"id":"chimes","primaryValue":2.5,"sparkline":{"totalChange":0,"data":[]}}],
        "items":[{"id":"chimes","name":"Ailith's Chimes","image":"/y.png","category":"LineageSupportGems","detailsId":"ailiths-chimes"}]}"#;

    #[test]
    fn requests_one_overview_per_type() {
        let r = overview_request("Runes of Aldur", "SoulCores");
        assert_eq!(
            r.url,
            "https://poe.ninja/poe2/api/economy/exchange/current/overview?league=Runes%20of%20Aldur&type=SoulCores"
        );
    }

    #[test]
    fn prices_convert_through_the_core_currency() {
        let d = en();
        let mut prices = Prices { core_currency: Some("exalted".into()), ..Default::default() };
        let league = SelectedLeague { id: "Runes of Aldur".into(), realm: "pc-ggg".into(), is_popular: true };
        assert!(prices.wants_load(Some(&league), false, 0));
        prices.loaded(d, &[("Currency", CURRENCY), ("LineageSupportGems", GEMS), ("Nonsense", "{}")], 0);
        assert_eq!((prices.xchg_rate, prices.xchg_rate_currency), (Some(132.4), Some("exalted")));

        let query = DbQuery { ns: "ITEM".into(), name: "Exalted Orb".into(), variant: None };
        let ex = prices.find_price(&query).unwrap();
        assert_eq!(ex.url, "https://poe.ninja/poe2/economy/runesofaldur/currency/exalted-orb");
        assert!(ex.cx);
        // A line without an item has no name to be found by.
        assert_eq!(prices.database[0].entries.len(), 1);
        let gem = DbQuery { ns: "GEM".into(), name: "Ailith's Chimes".into(), variant: None };
        assert_eq!(prices.find_price(&gem).unwrap().info.primary_value, 2.5);

        // 100 exalted orbs, in exalted.
        let value = prices.cached_currency_by_query(&query, 100.0).unwrap();
        assert_eq!(value.currency, "exalted");
        let expected = 0.00755 * 100.0 * 132.4;
        assert!((value.min - expected).abs() < 1e-9);
        // A divine's worth switches to divines.
        assert_eq!(prices.auto_currency(1.0, false), CurrencyValue { min: 1.0, max: 1.0, currency: "div" });
        assert_eq!(prices.auto_currency(3.0, false).currency, "div");
        assert_eq!(prices.auto_currency(3.0, true).currency, "exalted");

        let one_div = CurrencyValue { min: 1.0, max: 1.0, currency: "div" };
        let half = CurrencyValue { min: 66.2, max: 66.2, currency: "exalted" };
        assert_eq!(prices.add_price(one_div, half).unwrap().currency, "div");
        assert!(prices.compare_price(one_div, PriceOp::Gt, half).unwrap());
        let chaos = CurrencyValue { min: 1.0, max: 1.0, currency: "chaos" };
        assert!(prices.to_divine(chaos).is_err());

        // Chaos as the core currency, when its rate is usable.
        let mut chaos_prices = Prices { core_currency: Some("chaos".into()), ..Default::default() };
        chaos_prices.loaded(d, &[("Currency", CURRENCY)], 0);
        // 1.86 chaos per divine is below upstream's threshold of 5: exalted.
        assert_eq!(chaos_prices.xchg_rate_currency, Some("exalted"));
    }

    /// Upstream's `overviewData.json` holds the item's name and details id on each line;
    /// `loaded_upstream` turns it into the database `find_price` searches.
    #[test]
    fn the_upstream_blob_becomes_the_price_database() {
        let blob = r#"{"core":{"rates":{"exalted":131.7,"chaos":2.24},"primary":"divine","secondary":"chaos"},
            "itemOverviews":[
              {"type":"Currency","lines":[{"name":"Exalted Orb","primaryValue":0.007594,"detailsId":"exalted-orb","id":"exalted","sparkline":{"totalChange":0,"data":[]}}]},
              {"type":"Ultimatum","lines":[{"name":"Something","primaryValue":1,"detailsId":"x","id":"x","sparkline":{"totalChange":0,"data":[]}}]}]}"#;
        let mut prices = Prices { core_currency: Some("exalted".into()), ..Default::default() };
        assert!(prices.loaded_upstream(en(), "Standard", blob, 0));
        // `Math.round(xchgRate)`: the title bar's number.
        assert_eq!((prices.xchg_rate, prices.xchg_rate_currency), (Some(131.7), Some("exalted")));
        let query = DbQuery { ns: "ITEM".into(), name: "Exalted Orb".into(), variant: None };
        let price = prices.find_price(&query).expect("the currency's line");
        assert_eq!(price.url, "https://poe.ninja/poe2/economy/standard/currency/exalted-orb");
        // An overview type the app does not ask poe.ninja for is skipped, not an error.
        assert!(!prices.is_empty());
        assert!(!prices.loaded_upstream(en(), "Standard", "{", 0), "a broken blob is not applied");
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
