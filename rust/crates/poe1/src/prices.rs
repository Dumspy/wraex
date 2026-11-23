//! `ref/apt/renderer/src/web/background/Prices.ts`: poe.ninja prices for the
//! selected league.
//!
//! poe.ninja answers one JSON blob of every overview; upstream keeps it as
//! text split per overview and finds an item by searching for the start of its
//! JSON object, which only works because poe.ninja writes `name`, `variant`
//! and `chaos` first, in that order. This does the same. The refresh timing
//! is plain state over a clock (`now`, milliseconds), like the rate limits.

use serde::{Deserialize, Serialize};
use wraex_core::js::{normalize_nfkd, number_to_string, to_fixed};
use wraex_core::leagues::SelectedLeague;
use wraex_core::trade::api::{HttpRequest, Method};

/// Retry, refresh and "still interested" intervals (`RETRY_INTERVAL_MS`, ...).
pub const RETRY_INTERVAL_MS: u64 = 4 * 60 * 1000;
pub const UPDATE_INTERVAL_MS: u64 = 31 * 60 * 1000;
pub const INTEREST_SPAN_MS: u64 = 20 * 60 * 1000;

/// One overview of the blob (`PriceDatabase` entry).
#[derive(Debug, Clone, PartialEq)]
struct Overview {
    ns: &'static str,
    url: &'static str,
    lines: String,
}

/// `NinjaDenseInfo`
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct NinjaDenseInfo {
    pub chaos: f64,
    pub graph: Vec<Option<f64>>,
    pub name: String,
    #[serde(default)]
    pub variant: Option<String>,
}

/// A found price: the info plus its poe.ninja page.
#[derive(Debug, Clone, PartialEq)]
pub struct NinjaPrice {
    pub info: NinjaDenseInfo,
    pub url: String,
}

/// `DbQuery`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbQuery<'a> {
    /// `ITEM`, `UNIQUE`, `GEM`, ...
    pub ns: &'a str,
    pub name: &'a str,
    pub variant: Option<&'a str>,
}

/// `CurrencyValue`
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct CurrencyValue {
    pub min: f64,
    pub max: f64,
    /// `chaos` or `div`.
    pub currency: &'static str,
}

/// `NAMESPACE_MAP`: poe.ninja's overview types the app looks prices up in.
const NAMESPACE_MAP: [(&str, &str, &str); 37] = [
    ("ITEM", "currency", "Currency"),
    ("ITEM", "fragments", "Fragment"),
    ("ITEM", "delirium-orbs", "DeliriumOrb"),
    ("ITEM", "artifacts", "Artifact"),
    ("ITEM", "base-types", "BaseType"),
    ("ITEM", "fossils", "Fossil"),
    ("ITEM", "resonators", "Resonator"),
    ("ITEM", "incubators", "Incubator"),
    ("ITEM", "oils", "Oil"),
    ("ITEM", "vials", "Vial"),
    ("ITEM", "invitations", "Invitation"),
    ("ITEM", "blighted-maps", "BlightedMap"),
    ("ITEM", "blight-ravaged-maps", "BlightRavagedMap"),
    ("ITEM", "essences", "Essence"),
    ("ITEM", "maps", "Map"),
    ("ITEM", "tattoos", "Tattoo"),
    ("ITEM", "omens", "Omen"),
    ("ITEM", "coffins", "Coffin"),
    ("ITEM", "allflame-embers", "AllflameEmber"),
    ("ITEM", "djinn-coins", "DjinnCoin"),
    ("ITEM", "astrolabes", "Astrolabe"),
    ("ITEM", "runegrafts", "Runegraft"),
    ("ITEM", "ducats", "Ducat"),
    ("ITEM", "enshrouding-crystals", "EnshroudingCrystal"),
    ("ITEM", "corpses", "Corpse"),
    ("ITEM", "threads", "Thread"),
    ("DIVINATION_CARD", "divination-cards", "DivinationCard"),
    ("CAPTURED_BEAST", "beasts", "Beast"),
    ("UNIQUE", "unique-jewels", "UniqueJewel"),
    ("UNIQUE", "unique-flasks", "UniqueFlask"),
    ("UNIQUE", "unique-weapons", "UniqueWeapon"),
    ("UNIQUE", "unique-armours", "UniqueArmour"),
    ("UNIQUE", "unique-accessories", "UniqueAccessory"),
    ("UNIQUE", "unique-maps", "UniqueMap"),
    ("UNIQUE", "unique-relics", "UniqueRelic"),
    ("UNIQUE", "unique-tinctures", "UniqueTincture"),
    ("GEM", "skill-gems", "SkillGem"),
];

/// `splitJsonBlob`: the blob cut at each `{"type":"`, the supported ones kept.
fn split_json_blob(blob: &str) -> Vec<Overview> {
    const NINJA_OVERVIEW: &str = r#"{"type":""#;
    let mut database = Vec::new();
    let Some(mut start) = blob.find(NINJA_OVERVIEW) else { return database };
    loop {
        let end = blob[start + 1..].find(NINJA_OVERVIEW).map(|i| i + start + 1);
        let type_start = start + NINJA_OVERVIEW.len();
        let kind = blob[type_start..].find('"').map_or(&blob[type_start..], |i| &blob[type_start..type_start + i]);
        let lines = &blob[start..end.unwrap_or(blob.len())];
        if let Some(&(ns, url, _)) = NAMESPACE_MAP.iter().find(|(_, _, t)| *t == kind) {
            database.push(Overview { ns, url, lines: lines.to_owned() });
        }
        match end {
            Some(end) => start = end,
            None => break,
        }
    }
    database
}

/// `denseInfoToDetailsId`: the item's slug on poe.ninja.
pub fn dense_info_to_details_id(info: &NinjaDenseInfo) -> String {
    let full = match &info.variant {
        Some(v) if !v.is_empty() => format!("{}, {v}", info.name),
        _ => info.name.clone(),
    };
    normalize_nfkd(&full)
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ':' | '-' | ' '))
        .collect::<String>()
        .to_lowercase()
        .replace(' ', "-")
}

/// `selectedLeagueToUrl`: poe.ninja's name for a league.
pub fn league_to_url(league: &str) -> String {
    match league {
        "Standard" => "standard".to_owned(),
        "Hardcore" => "hardcore".to_owned(),
        _ => {
            let mut id = league.replacen("Hardcore ", "", 1).to_lowercase();
            if league.starts_with("Hardcore ") {
                id.push_str("hc");
            }
            id
        }
    }
}

/// `displayRounding`: a price as shown, with hair spaces around the point.
pub fn display_rounding(value: f64, fraction: bool) -> String {
    if fraction && value.abs() < 1.0 {
        if value == 0.0 {
            return "0".to_owned();
        }
        let r = format!("1\u{200A}/\u{200A}{}", display_rounding(1.0 / value, false));
        return if r == "1\u{200A}/\u{200A}1" { "1".to_owned() } else { r };
    }
    if value.abs() < 10.0 {
        // `Number(value.toFixed(1)).toString()`
        let n: f64 = to_fixed(value, 1).parse().unwrap_or(f64::NAN);
        return number_to_string(n).replace('.', "\u{200A}.\u{200A}");
    }
    // `Math.round`: halves toward +∞.
    number_to_string((value + 0.5).floor())
}

/// Prices for the selected league (`usePoeninja`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Prices {
    database: Vec<Overview>,
    /// Chaos per divine, once known.
    pub xchg_rate: Option<f64>,
    /// The league the prices are for.
    league_id: Option<String>,
    last_update: Option<u64>,
    last_interest: Option<u64>,
}

impl Prices {
    /// The request for `league` (`load`), or `None` when prices aren't wanted
    /// now: not a public pc-ggg league, updated recently, or no interest
    /// lately. `force` skips the timing checks.
    pub fn request(&mut self, league: Option<&SelectedLeague>, force: bool, now: u64) -> Option<HttpRequest> {
        let league = league.filter(|l| l.is_popular && l.realm == "pc-ggg")?;
        if self.league_id.as_deref() != Some(league.id.as_str()) {
            // `watch(leagues.selectedId)`: a new league drops the old prices.
            self.xchg_rate = None;
            self.database.clear();
            self.league_id = Some(league.id.clone());
            return Some(Self::url(&league.id));
        }
        let since = |t: Option<u64>| t.map_or(u64::MAX, |t| now.saturating_sub(t));
        if !force && (since(self.last_update) < UPDATE_INTERVAL_MS || since(self.last_interest) > INTEREST_SPAN_MS) {
            return None;
        }
        Some(Self::url(&league.id))
    }

    fn url(league: &str) -> HttpRequest {
        let league: String = league.chars().map(|c| if c == ' ' { "%20".to_owned() } else { c.to_string() }).collect();
        HttpRequest {
            method: Method::Get,
            url: format!("https://poe.ninja/poe1/api/economy/current/dense/overviews?league={league}&language=en"),
            headers: Vec::new(),
            body: None,
        }
    }

    /// `queuePricesFetch`: the user looked at a price; keep prices fresh.
    pub fn mark_interest(&mut self, now: u64) {
        self.last_interest = Some(now);
    }

    /// Takes poe.ninja's answer for the league.
    pub fn loaded(&mut self, blob: &str, now: u64) {
        self.database = split_json_blob(blob);
        // A divine below 30 chaos is a broken price, not a rate.
        if let Some(divine) = self.find_price(&DbQuery { ns: "ITEM", name: "Divine Orb", variant: None })
            && divine.info.chaos >= 30.0
        {
            self.xchg_rate = Some(divine.info.chaos);
        }
        self.last_update = Some(now);
    }

    /// `initialLoading`: nothing to show yet.
    pub fn is_empty(&self) -> bool {
        self.database.is_empty()
    }

    /// `findPriceByQuery`
    pub fn find_price(&self, query: &DbQuery) -> Option<NinjaPrice> {
        // `JSON.stringify({ name, variant, chaos: 0 }).replace(':0}', ':')`:
        // an `undefined` variant is left out.
        let mut search = format!("{{\"name\":{}", wraex_core::js::json_string(query.name));
        if let Some(v) = query.variant {
            search.push_str(&format!(",\"variant\":{}", wraex_core::js::json_string(v)));
        }
        search.push_str(",\"chaos\":");

        for overview in self.database.iter().filter(|o| o.ns == query.ns) {
            let Some(start) = overview.lines.find(&search) else { continue };
            let end = overview.lines[start..].find('}').map(|i| start + i)?;
            let info: NinjaDenseInfo = serde_json::from_str(&overview.lines[start..=end]).ok()?;
            let league = league_to_url(self.league_id.as_deref().unwrap_or_default());
            let url =
                format!("https://poe.ninja/poe1/economy/{league}/{}/{}", overview.url, dense_info_to_details_id(&info));
            return Some(NinjaPrice { info, url });
        }
        None
    }

    fn rate(&self) -> f64 {
        self.xchg_rate.filter(|r| *r != 0.0).unwrap_or(9999.0)
    }

    /// `chaosToStable`: chaos in divines.
    pub fn chaos_to_stable(&self, count: f64) -> f64 {
        count / self.rate()
    }

    /// `autoCurrency`: a value in chaos, or in divines once it's worth one.
    pub fn auto_currency(&self, value: f64) -> CurrencyValue {
        let rate = self.rate();
        if value > rate * 0.94 {
            if value < rate * 1.06 {
                return CurrencyValue { min: 1.0, max: 1.0, currency: "div" };
            }
            let d = self.chaos_to_stable(value);
            return CurrencyValue { min: d, max: d, currency: "div" };
        }
        CurrencyValue { min: value, max: value, currency: "chaos" }
    }

    /// `autoCurrency` of a range.
    pub fn auto_currency_range(&self, min: f64, max: f64) -> CurrencyValue {
        if max > self.rate() {
            return CurrencyValue { min: self.chaos_to_stable(min), max: self.chaos_to_stable(max), currency: "div" };
        }
        CurrencyValue { min, max, currency: "chaos" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOB: &str = concat!(
        r#"{"overviews":[{"type":"Currency","lines":[{"name":"Divine Orb","chaos":210.5,"graph":[1,null]},"#,
        r#"{"name":"Exalted Orb","chaos":12,"graph":[]}]},"#,
        r#"{"type":"Unknown","lines":[{"name":"Divine Orb","chaos":1,"graph":[]}]},"#,
        r#"{"type":"UniqueArmour","lines":[{"name":"Kaom's Heart","variant":"Gold","chaos":40,"graph":[]}]}]}"#
    );

    fn league(id: &str) -> SelectedLeague {
        SelectedLeague { id: id.into(), realm: "pc-ggg".into(), is_popular: true }
    }

    #[test]
    fn prices_are_found_in_the_blob() {
        let mut prices = Prices::default();
        let request = prices.request(Some(&league("Hardcore Settlers")), false, 0).unwrap();
        assert_eq!(
            request.url,
            "https://poe.ninja/poe1/api/economy/current/dense/overviews?league=Hardcore%20Settlers&language=en"
        );
        prices.loaded(BLOB, 0);
        assert_eq!(prices.xchg_rate, Some(210.5));
        let kaom = prices.find_price(&DbQuery { ns: "UNIQUE", name: "Kaom's Heart", variant: Some("Gold") }).unwrap();
        assert_eq!(kaom.info.chaos, 40.0);
        assert_eq!(kaom.url, "https://poe.ninja/poe1/economy/settlershc/unique-armours/kaoms-heart-gold");
        assert!(prices.find_price(&DbQuery { ns: "UNIQUE", name: "Kaom's Heart", variant: None }).is_none());
        assert!(prices.find_price(&DbQuery { ns: "GEM", name: "Divine Orb", variant: None }).is_none());
    }

    #[test]
    fn prices_refresh_while_someone_looks() {
        let mut prices = Prices::default();
        let l = league("Standard");
        assert!(prices.request(Some(&l), false, 0).is_some());
        prices.loaded(BLOB, 0);
        // Fresh: no request.
        assert!(prices.request(Some(&l), false, 1000).is_none());
        // Old, but nobody looked lately.
        assert!(prices.request(Some(&l), false, UPDATE_INTERVAL_MS + 1).is_none());
        prices.mark_interest(UPDATE_INTERVAL_MS);
        assert!(prices.request(Some(&l), false, UPDATE_INTERVAL_MS + 1).is_some());
        // Private and non-pc-ggg leagues get none.
        let private = SelectedLeague { is_popular: false, ..l.clone() };
        assert!(prices.request(Some(&private), true, 0).is_none());
    }

    #[test]
    fn values_switch_to_divines() {
        let mut prices = Prices::default();
        prices.loaded(BLOB, 0);
        assert_eq!(prices.auto_currency(100.0).currency, "chaos");
        assert_eq!(prices.auto_currency(200.0), CurrencyValue { min: 1.0, max: 1.0, currency: "div" });
        assert_eq!(prices.auto_currency(421.0).min, 2.0);
        assert_eq!(prices.auto_currency_range(10.0, 421.0).currency, "div");
    }

    #[test]
    fn rounding_for_display() {
        assert_eq!(display_rounding(0.25, false), "0\u{200A}.\u{200A}3");
        assert_eq!(display_rounding(2.0, false), "2");
        assert_eq!(display_rounding(12.5, false), "13");
        assert_eq!(display_rounding(0.5, true), "1\u{200A}/\u{200A}2");
        assert_eq!(display_rounding(0.0, true), "0");
        assert_eq!(league_to_url("Settlers"), "settlers");
        assert_eq!(
            dense_info_to_details_id(&NinjaDenseInfo {
                chaos: 0.0,
                graph: vec![],
                name: "Maelström Staff".into(),
                variant: None
            }),
            "maelstrom-staff"
        );
    }
}
