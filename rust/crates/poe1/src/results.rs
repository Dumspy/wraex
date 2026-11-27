//! What a PoE1 price check shows from the trade site's answers.
//!
//! `requestResults` (`pathofexile-trade.ts`) turns fetched listings into rows,
//! `execBulkSearch` (`pathofexile-bulk.ts`) groups exchange offers by the
//! currency asked for. The requests themselves are `wraex_core::trade::api`.

use serde::{Deserialize, Serialize};
use wraex_core::relative_time::{EpochMillis, iso_to_relative_short};
use wraex_core::trade::api::ExchangeResult;

/// The account behind a listing (`Account`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub name: String,
    /// Missing on a collapsed merchant listing (the trade site sends `online: null`
    /// and no character name); upstream reads it as `undefined`.
    #[serde(default)]
    pub last_character_name: String,
    #[serde(default)]
    pub online: Option<Online>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Online {
    #[serde(default)]
    pub status: Option<String>,
}

/// `'online' | 'afk' | 'offline'`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountStatus {
    Offline,
    Online,
    Afk,
}

fn account_status(account: &Account) -> AccountStatus {
    match &account.online {
        Some(online) if online.status.as_deref() == Some("afk") => AccountStatus::Afk,
        Some(_) => AccountStatus::Online,
        None => AccountStatus::Offline,
    }
}

/// A fetched listing (`FetchResult`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FetchResult {
    pub id: String,
    pub item: FetchItem,
    pub listing: FetchListing,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchItem {
    #[serde(default)]
    pub ilvl: Option<f64>,
    #[serde(default)]
    pub stack_size: Option<f64>,
    #[serde(default)]
    pub corrupted: Option<bool>,
    #[serde(default)]
    pub properties: Option<Vec<Property>>,
    #[serde(default)]
    pub note: Option<String>,
}

/// An item property: `values` is `[[text, displayMode]]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Property {
    pub values: Vec<(String, f64)>,
    #[serde(rename = "type", default)]
    pub kind: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FetchListing {
    pub indexed: String,
    #[serde(default)]
    pub price: Option<Price>,
    #[serde(default)]
    pub fee: Option<f64>,
    pub account: Account,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Price {
    pub amount: f64,
    pub currency: String,
}

/// Property types the rows read (`FetchResult['item']['properties'][]['type']`).
const PROP_LEVEL: i64 = 5;
const PROP_QUALITY: i64 = 6;
/// Corpse Level (Filled Coffin)
const PROP_CORPSE_LEVEL: i64 = 78;

/// A price-check row (`PricingResult`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingResult {
    pub id: String,
    pub item_level: String,
    pub stack_size: Option<f64>,
    pub corrupted: Option<bool>,
    pub quality: Option<String>,
    pub level: Option<String>,
    pub relative_date: String,
    pub price_amount: f64,
    pub price_currency: String,
    pub is_mine: bool,
    pub has_note: bool,
    pub has_fee: bool,
    pub account_name: String,
    pub account_status: AccountStatus,
    pub ign: String,
}

/// `prop.values[0][0]` of the first property of `kind`.
fn property(item: &FetchItem, kind: i64) -> Option<String> {
    let prop = item.properties.as_ref()?.iter().find(|p| p.kind == Some(kind))?;
    prop.values.first().map(|(text, _)| text.clone())
}

/// `requestResults`' rows, for the user's `account_name` at `now`.
pub fn pricing_results(listings: &[FetchResult], account_name: &str, now: EpochMillis) -> Vec<PricingResult> {
    listings
        .iter()
        .map(|result| {
            let item = &result.item;
            let listing = &result.listing;
            PricingResult {
                id: result.id.clone(),
                // `?? String(result.item.ilvl)`
                item_level: property(item, PROP_CORPSE_LEVEL).unwrap_or_else(|| {
                    item.ilvl.map_or_else(|| "undefined".to_owned(), wraex_core::js::number_to_string)
                }),
                stack_size: item.stack_size,
                corrupted: item.corrupted,
                quality: property(item, PROP_QUALITY),
                level: property(item, PROP_LEVEL),
                relative_date: iso_to_relative_short(&listing.indexed, now).unwrap_or_default(),
                price_amount: listing.price.as_ref().map_or(0.0, |p| p.amount),
                price_currency: listing.price.as_ref().map_or_else(|| "no price".to_owned(), |p| p.currency.clone()),
                is_mine: listing.account.name == account_name,
                has_note: item.note.is_some(),
                has_fee: listing.fee.is_some(),
                account_name: listing.account.name.clone(),
                // A listing with a fee (a merchant's) is always online.
                account_status: if listing.fee.is_some() {
                    AccountStatus::Online
                } else {
                    account_status(&listing.account)
                },
                ign: listing.account.last_character_name.clone(),
            }
        })
        .collect()
}

/// An exchange listing.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExchangeListing {
    pub id: String,
    pub listing: ExchangeOffers,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExchangeOffers {
    pub indexed: String,
    pub offers: Vec<Offer>,
    pub account: Account,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Offer {
    pub exchange: OfferExchange,
    pub item: OfferItem,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OfferExchange {
    pub currency: String,
    pub amount: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct OfferItem {
    pub amount: f64,
    pub stock: f64,
}

/// A bulk row (`PricingResult` of `pathofexile-bulk.ts`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkPricingResult {
    pub id: String,
    pub relative_date: String,
    pub exchange_amount: f64,
    pub item_amount: f64,
    pub stock: f64,
    pub account_status: AccountStatus,
    pub is_mine: bool,
    pub account_name: String,
    pub ign: String,
}

/// The listings paid in one currency (`BulkSearch`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkSearch {
    pub query_id: String,
    pub have_tag: String,
    pub total: f64,
    pub listed: Vec<BulkPricingResult>,
}

/// Rows shown per currency (`SHOW_RESULTS`).
const SHOW_RESULTS: usize = 20;
/// Listings the exchange answers with at most (`API_FETCH_LIMIT`).
const API_FETCH_LIMIT: f64 = 100.0;

/// `execBulkSearch` after the request: one group per `have` currency, cheapest
/// first. `None` for chaos when they would need loading on demand.
pub fn bulk_searches(
    query: &ExchangeResult<ExchangeListing>,
    have: &[String],
    account_name: &str,
    now: EpochMillis,
) -> Vec<Option<BulkSearch>> {
    // Only a listing with exactly one offer gives a rate (`offers[0]`).
    let results: Vec<(&ExchangeListing, &Offer)> = query
        .result
        .values()
        .filter_map(|listing| match listing.listing.offers.as_slice() {
            [offer] => Some((listing, offer)),
            _ => None,
        })
        .collect();

    have.iter()
        .map(|trade_tag| {
            let mut tagged: Vec<(&ExchangeListing, &Offer)> =
                results.iter().copied().filter(|(_, offer)| &offer.exchange.currency == trade_tag).collect();

            let loaded_on_demand = trade_tag == "chaos" && tagged.len() < SHOW_RESULTS && query.total > API_FETCH_LIMIT;
            if loaded_on_demand {
                return None;
            }

            let ratio = |offer: &Offer| offer.exchange.amount / offer.item.amount;
            // Stable, like `Array.prototype.sort`.
            tagged.sort_by(|(_, a), (_, b)| ratio(a).partial_cmp(&ratio(b)).unwrap_or(std::cmp::Ordering::Equal));
            let listed = tagged
                .iter()
                .take(SHOW_RESULTS)
                .map(|(listing, offer)| {
                    let account = &listing.listing.account;
                    BulkPricingResult {
                        id: listing.id.clone(),
                        relative_date: iso_to_relative_short(&listing.listing.indexed, now).unwrap_or_default(),
                        exchange_amount: offer.exchange.amount,
                        item_amount: offer.item.amount,
                        stock: offer.item.stock,
                        account_status: account_status(account),
                        is_mine: account.name == account_name,
                        account_name: account.name.clone(),
                        ign: account.last_character_name.clone(),
                    }
                })
                .collect();

            let others = wraex_core::num::usize_to_f64(results.len().saturating_sub(tagged.len()));
            let chaos_is_loaded = trade_tag == "divine"
                && tagged.len() < results.len()
                && (others >= wraex_core::num::usize_to_f64(SHOW_RESULTS) || query.total <= API_FETCH_LIMIT);

            Some(BulkSearch {
                query_id: query.id.clone(),
                have_tag: trade_tag.clone(),
                // A best guess when asking with several `have` currencies.
                total: if chaos_is_loaded { wraex_core::num::usize_to_f64(tagged.len()) } else { query.total - others },
                listed,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use wraex_core::relative_time::parse_iso;

    #[test]
    fn listings_become_rows() {
        let listings: Vec<FetchResult> = serde_json::from_str(
            r#"[{
                "id": "a",
                "item": { "ilvl": 84, "corrupted": true, "note": "~price 5 chaos",
                          "properties": [{ "values": [["+20%", 1]], "type": 6 }] },
                "listing": { "indexed": "2024-03-31T11:55:00Z",
                             "price": { "amount": 5, "currency": "chaos" },
                             "account": { "name": "me#1", "lastCharacterName": "Me",
                                          "online": { "status": "afk" } } }
            }, {
                "id": "b",
                "item": {},
                "listing": { "indexed": "2024-03-30T12:00:00Z", "fee": 10,
                             "account": { "name": "them#2", "lastCharacterName": "Them" } }
            }]"#,
        )
        .unwrap();
        let now = parse_iso("2024-03-31T12:00:00Z").unwrap();
        let rows = pricing_results(&listings, "me#1", now);
        let a = &rows[0];
        assert_eq!(
            (a.item_level.as_str(), a.quality.as_deref(), a.relative_date.as_str()),
            ("84", Some("+20%"), "5 min. ago")
        );
        assert_eq!((a.price_amount, a.is_mine, a.has_note, a.account_status), (5.0, true, true, AccountStatus::Afk));
        let b = &rows[1];
        assert_eq!((b.item_level.as_str(), b.price_currency.as_str(), b.price_amount), ("undefined", "no price", 0.0));
        // Merchants are online.
        assert_eq!((b.has_fee, b.account_status, b.relative_date.as_str()), (true, AccountStatus::Online, "1 day ago"));
    }

    #[test]
    fn exchange_offers_group_by_currency_cheapest_first() {
        let listing = |id: &str, currency: &str, amount: f64, items: f64| {
            format!(
                r#""{id}": {{ "id": "{id}", "listing": {{ "indexed": "2024-03-31T11:00:00Z",
                    "offers": [{{ "exchange": {{ "currency": "{currency}", "amount": {amount} }},
                                  "item": {{ "amount": {items}, "stock": 50 }} }}],
                    "account": {{ "name": "x", "lastCharacterName": "X", "online": {{}} }} }} }}"#
            )
        };
        let body = format!(
            r#"{{ "id": "q", "total": 3, "result": {{ {}, {}, {} }} }}"#,
            listing("1", "divine", 2.0, 1.0),
            listing("2", "chaos", 150.0, 1.0),
            listing("3", "divine", 1.0, 1.0)
        );
        let query: ExchangeResult<ExchangeListing> = serde_json::from_str(&body).unwrap();
        let now = parse_iso("2024-03-31T12:00:00Z").unwrap();
        let groups = bulk_searches(&query, &["divine".into(), "chaos".into()], "y", now);
        let divine = groups[0].as_ref().unwrap();
        assert_eq!(divine.listed.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["3", "1"]);
        // Divine: fewer than all, and the whole answer is loaded.
        assert_eq!(divine.total, 2.0);
        let chaos = groups[1].as_ref().unwrap();
        assert_eq!((chaos.total, chaos.listed[0].account_status), (1.0, AccountStatus::Online));
        assert_eq!(chaos.listed[0].relative_date, "1 hr. ago");
    }
}
