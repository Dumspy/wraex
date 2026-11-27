//! Trade request pieces both references share: the bulk exchange request of
//! `web/price-check/trade/pathofexile-bulk.ts` (the game picks the listing
//! status), and `dot-prop`'s `setProperty`, which both use to build search
//! filters. [`rate_limit`] tracks the trade site's rate limits, [`api`]
//! sends requests within them, and [`site_link`] builds the trade site's page
//! links (`makeTradeLink`).

pub mod api;
pub mod rate_limit;
pub mod site_link;

use serde::Serialize;
use serde_json::{Map, Value};

/// `setProperty(object, path, value)` from `dot-prop`: creates the objects on a
/// dotted path and sets the last key. `None` is JavaScript's `undefined`, which
/// `JSON.stringify` leaves out, so the key goes but the objects stay.
pub fn prop_set(object: &mut Map<String, Value>, path: &str, value: Option<Value>) {
    let mut keys = path.split('.').peekable();
    let mut current = object;
    while let Some(key) = keys.next() {
        if keys.peek().is_none() {
            match value {
                Some(value) => {
                    current.insert(key.to_owned(), value);
                }
                None => {
                    current.remove(key);
                }
            }
            return;
        }
        let entry = current.entry(key.to_owned()).or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        current = entry.as_object_mut().expect("just made an object");
    }
}

/// The exchange request (`/api/trade/exchange`, `/api/trade2/exchange`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BulkRequest {
    pub engine: &'static str,
    pub query: BulkQuery,
    pub sort: BulkSort,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BulkQuery {
    pub have: Vec<String>,
    /// `[tradeTag(item)!]`: `null` when the item has no trade tag.
    pub want: Vec<Option<String>>,
    pub status: BulkStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BulkStatus {
    /// `online`, `onlineleague` or `any`.
    pub option: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BulkSort {
    pub have: &'static str,
}

/// `createTradeRequest` (`pathofexile-bulk.ts`). `minimum` is the stack size
/// when its filter is enabled.
pub fn bulk_request(have: Vec<String>, want: Option<String>, status: String, minimum: Option<f64>) -> BulkRequest {
    BulkRequest {
        engine: "new",
        query: BulkQuery { have, want: vec![want], status: BulkStatus { option: status }, minimum },
        sort: BulkSort { have: "asc" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prop_set_creates_paths_and_drops_undefined() {
        let mut m = Map::new();
        prop_set(&mut m, "a.b.min", Some(json!(1)));
        prop_set(&mut m, "a.b.max", None);
        prop_set(&mut m, "a.c.max", None);
        assert_eq!(Value::Object(m), json!({ "a": { "b": { "min": 1 }, "c": {} } }));
    }

    #[test]
    fn bulk_requests_serialise_like_the_reference() {
        let r = bulk_request(vec!["divine".into()], Some("chaos".into()), "online".into(), None);
        assert_eq!(
            serde_json::to_value(r).unwrap(),
            json!({ "engine": "new", "query": { "have": ["divine"], "want": ["chaos"], "status": { "option": "online" } }, "sort": { "have": "asc" } })
        );
    }
}
