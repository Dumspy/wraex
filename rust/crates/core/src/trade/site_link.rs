//! The trade site's page links (`makeTradeLink` in `TradeListing.vue` and
//! `TradeBulk.vue`): where the "Trade" button and the error box's "Browser" button
//! take the user.
//!
//! Upstream writes the raw `JSON.stringify` query into `?q=` and lets Chromium
//! percent-encode it when it opens the page. Rune's `open_url` refuses links with
//! whitespace or `"` `{` `}` and the rest of its forbidden set, so the league and
//! `q` are encoded here with `encodeURIComponent`'s rules; the trade site decodes
//! `q` the same way and sees the same query.

use serde::Serialize;

use super::api::TradeSite;
use crate::js;

/// Rune's `open_url` refuses links longer than this (`MAX_LEN` in
/// `crates/rune/src/open_url.rs`). A by-query link that would exceed it falls back
/// to the bare search/exchange page for the league: Rune cannot open the query, and
/// the button still takes the user to the trade site. The largest request in the
/// corpus encodes to about 2.7 KB, so this is a safety net.
pub const MAX_LINK_LEN: usize = 8192;

/// `https://{host}/{page}/search/{league}/{id}`: the page of a search that has run.
pub fn search_by_id(site: TradeSite, host: &str, league: &str, id: &str) -> String {
    format!("{}/{id}", page_base(site, host, league, "search"))
}

/// `https://{host}/{page}/search/{league}?q={JSON.stringify(query)}`: the search a
/// press on Search would send.
pub fn search_by_query<T: Serialize + ?Sized>(site: TradeSite, host: &str, league: &str, query: &T) -> String {
    with_query(site, host, league, "search", query)
}

/// `https://{host}/{page}/exchange/{league}?q={JSON.stringify({exchange})}`: the
/// bulk exchange a press on Search would send.
pub fn exchange_by_query<T: Serialize + ?Sized>(site: TradeSite, host: &str, league: &str, query: &T) -> String {
    with_query(site, host, league, "exchange", query)
}

fn page_base(site: TradeSite, host: &str, league: &str, page: &str) -> String {
    format!("https://{host}/{}", site.page(page, &js::encode_uri_component(league)))
}

fn with_query<T: Serialize + ?Sized>(site: TradeSite, host: &str, league: &str, page: &str, query: &T) -> String {
    let base = page_base(site, host, league, page);
    let link = format!("{base}?q={}", js::encode_uri_component(&js::stringify(query)));
    if link.len() > MAX_LINK_LEN { base } else { link }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const HOST: &str = "www.pathofexile.com";

    /// `check_link`'s refusal set from `crates/rune/src/open_url.rs`, for a link whose
    /// scheme and host are the ones above: too long, whitespace or control characters,
    /// the characters Windows' `ShellExecute` and the web build dislike, a bracket in
    /// the path or query, and a `%` that is not a two-hex-digit escape other than `%00`.
    fn rune_accepts(url: &str) -> bool {
        if url.len() > MAX_LINK_LEN {
            return false;
        }
        if url.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(c, '\\' | '"' | '<' | '>' | '`' | '^' | '{' | '|' | '}' | '[' | ']')
        }) {
            return false;
        }
        let bytes = url.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' {
                let (Some(&a), Some(&b)) = (bytes.get(i + 1), bytes.get(i + 2)) else { return false };
                if !a.is_ascii_hexdigit() || !b.is_ascii_hexdigit() || (a == b'0' && b == b'0') {
                    return false;
                }
                i += 3;
            } else {
                i += 1;
            }
        }
        true
    }

    #[test]
    fn a_search_by_id_is_the_league_and_the_id() {
        assert_eq!(
            search_by_id(TradeSite::Trade, HOST, "Standard", "abc123"),
            "https://www.pathofexile.com/trade/search/Standard/abc123"
        );
        assert_eq!(
            search_by_id(TradeSite::Trade2, HOST, "Standard", "abc123"),
            "https://www.pathofexile.com/trade2/search/poe2/Standard/abc123"
        );
    }

    #[test]
    fn a_search_by_query_is_the_encoded_json() {
        let query = json!({ "query": { "status": { "option": "online" } }, "sort": { "price": "asc" } });
        assert_eq!(
            search_by_query(TradeSite::Trade, HOST, "Standard", &query),
            concat!(
                "https://www.pathofexile.com/trade/search/Standard?q=",
                "%7B%22query%22%3A%7B%22status%22%3A%7B%22option%22%3A%22online%22%7D%7D",
                "%2C%22sort%22%3A%7B%22price%22%3A%22asc%22%7D%7D",
            )
        );
        assert_eq!(
            search_by_query(TradeSite::Trade2, HOST, "Standard", &query),
            concat!(
                "https://www.pathofexile.com/trade2/search/poe2/Standard?q=",
                "%7B%22query%22%3A%7B%22status%22%3A%7B%22option%22%3A%22online%22%7D%7D",
                "%2C%22sort%22%3A%7B%22price%22%3A%22asc%22%7D%7D",
            )
        );
    }

    #[test]
    fn an_exchange_by_query_is_the_encoded_exchange_json() {
        let query = json!({
            "exchange": {
                "have": ["chaos"],
                "want": ["divine"],
                "status": { "option": "online" },
            },
            "sort": { "have": "asc" },
        });
        assert_eq!(
            exchange_by_query(TradeSite::Trade, HOST, "Standard", &query),
            concat!(
                "https://www.pathofexile.com/trade/exchange/Standard?q=",
                "%7B%22exchange%22%3A%7B%22have%22%3A%5B%22chaos%22%5D%2C%22want%22%3A%5B%22divine%22%5D",
                "%2C%22status%22%3A%7B%22option%22%3A%22online%22%7D%7D%2C%22sort%22%3A%7B%22have%22%3A%22asc%22%7D%7D",
            )
        );
        assert_eq!(
            exchange_by_query(TradeSite::Trade2, HOST, "Standard", &query),
            concat!(
                "https://www.pathofexile.com/trade2/exchange/poe2/Standard?q=",
                "%7B%22exchange%22%3A%7B%22have%22%3A%5B%22chaos%22%5D%2C%22want%22%3A%5B%22divine%22%5D",
                "%2C%22status%22%3A%7B%22option%22%3A%22online%22%7D%7D%2C%22sort%22%3A%7B%22have%22%3A%22asc%22%7D%7D",
            )
        );
    }

    #[test]
    fn a_league_with_a_space_is_encoded() {
        let query = json!({ "query": {} });
        assert_eq!(
            search_by_id(TradeSite::Trade, HOST, "Standard Hardcore", "abc123"),
            "https://www.pathofexile.com/trade/search/Standard%20Hardcore/abc123"
        );
        assert!(
            search_by_query(TradeSite::Trade, HOST, "Standard Hardcore", &query)
                .starts_with("https://www.pathofexile.com/trade/search/Standard%20Hardcore?q=")
        );
        assert!(
            exchange_by_query(TradeSite::Trade2, HOST, "Standard Hardcore", &query)
                .starts_with("https://www.pathofexile.com/trade2/exchange/poe2/Standard%20Hardcore?q=")
        );
    }

    #[test]
    fn query_characters_rune_refuses_are_encoded() {
        let query = json!({ "query": { "name": "A \"Ring\" [#+&%]", "type": "Ring" } });
        let url = search_by_query(TradeSite::Trade, HOST, "Standard", &query);
        assert_eq!(
            url,
            concat!(
                "https://www.pathofexile.com/trade/search/Standard?q=",
                "%7B%22query%22%3A%7B%22name%22%3A%22A%20%5C%22Ring%5C%22%20%5B%23%2B%26%25%5D%22",
                "%2C%22type%22%3A%22Ring%22%7D%7D",
            )
        );
        for forbidden in ['"', '{', '}', '[', ']', '#', '+', '&', '\\'] {
            assert!(!url.contains(forbidden), "{forbidden:?} survived in {url}");
        }
        assert!(rune_accepts(&url));
    }

    #[test]
    fn an_over_long_query_falls_back_to_the_bare_page() {
        // Rune refuses links over `MAX_LINK_LEN`; the site can still be opened, without
        // the query.
        let query = json!({ "query": { "name": "x".repeat(MAX_LINK_LEN) } });
        assert_eq!(
            search_by_query(TradeSite::Trade, HOST, "Standard", &query),
            "https://www.pathofexile.com/trade/search/Standard"
        );
        assert_eq!(
            exchange_by_query(TradeSite::Trade2, HOST, "Standard", &query),
            "https://www.pathofexile.com/trade2/exchange/poe2/Standard"
        );
    }

    #[test]
    fn every_link_is_a_link_rune_can_open() {
        let queries = [
            json!({ "query": {} }),
            json!({ "query": { "name": "A \"Ring\" [#+&%]", "stats": [{ "id": "explicit.stat_1" }] } }),
            json!({ "exchange": { "have": ["chaos"], "want": ["divine"] }, "sort": { "have": "asc" } }),
        ];
        for query in &queries {
            for site in [TradeSite::Trade, TradeSite::Trade2] {
                for league in ["Standard", "Standard Hardcore", "Standard\u{a0}Hardcore"] {
                    let links = [
                        search_by_id(site, HOST, league, "abc123"),
                        search_by_query(site, HOST, league, query),
                        exchange_by_query(site, HOST, league, query),
                    ];
                    for link in links {
                        assert!(rune_accepts(&link), "Rune would refuse {link}");
                    }
                }
            }
        }
    }
}
