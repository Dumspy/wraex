//! `usePoeninja` from EE2's `web/background/Prices.ts`, as the window needs it: the
//! divine rate the title bar shows, and the database the listing rows normalise prices in.
//!
//! EE2 downloads a ready-made `overviewData.json` from its own server at startup. Wraex
//! asks poe.ninja directly ([`wraex_poe2::prices`]' docs explain the deviation), so a live
//! window fetches one overview per [`NINJA_TYPES`] entry from a worker; the screenshot
//! harness recorded EE2's own answer, and a replay reads that instead. Both end in
//! [`Prices::loaded`]/[`Prices::loaded_upstream`].

use wraex_poe2::prices::{NINJA_TYPES, Prices, overview_request};

use crate::trade::Mode;

/// `PriceCheckWidget.coreCurrency`: EE2's default is exalted, which prices are shown in
/// until the settings page changes it.
pub const CORE_CURRENCY: &str = "exalted";

/// The recorded `overviewData.json` (`--record` saved it with the screenshots).
const RECORDED: &str = "api.exiledexchange2.dev__proxy__standard__overviewData.json.GET.json";

/// A price database with EE2's default core currency and, in a replay, the recorded prices.
pub fn load(mode: Mode) -> Prices {
    let mut prices = Prices::default();
    prices.core_currency = Some(CORE_CURRENCY.to_owned());
    if mode == Mode::Replay {
        let path = format!("{}/poe2/proxy/{RECORDED}", crate::replay::CORPUS);
        let body = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let now = crate::trade::now(mode);
        let data = crate::poe2::price_check::data(mode);
        let parsed = prices.loaded_upstream(data, "Standard", &body, now);
        assert!(parsed, "{path} is not a price database");
    }
    prices
}

/// poe.ninja's overviews for `league`, one request per [`NINJA_TYPES`] entry, sent from the
/// worker that `poll` collects. (Upstream refreshes every 31 minutes while a price check is
/// in use; this window loads once when it opens.)
pub fn fetch(league: &str) -> Result<Vec<(&'static str, String)>, String> {
    use wraex_core::trade::api::Transport;

    let transport = wraex_platform::http::HttpTransport::default();
    let mut bodies = Vec::with_capacity(NINJA_TYPES.len());
    for (kind, _, _) in NINJA_TYPES {
        let response = transport.send(&overview_request(league, kind))?;
        if response.status != 200 {
            return Err(format!("poe.ninja's {kind} overview answered {}", response.status));
        }
        bodies.push((kind, response.body));
    }
    Ok(bodies)
}
