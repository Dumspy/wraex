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
#[expect(
    clippy::panic,
    reason = "the recorded `overviewData.json` ships in the screenshot corpus; missing or unreadable is a broken checkout, not a runtime case"
)]
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

/// poe.ninja's overviews for `league`, one request per [`NINJA_TYPES`] entry.
///
/// The worker `poll` collects sends them. Upstream refreshes every 31 minutes while a price
/// check is in use; this window loads once when it opens.
pub fn fetch(league: &wraex_core::leagues::SelectedLeague) -> Result<Vec<(&'static str, String)>, String> {
    use wraex_core::trade::api::Transport;

    if !league.is_popular || league.realm != "pc-ggg" {
        return Ok(Vec::new());
    }
    let transport = crate::trade::live_transport();
    let mut bodies = Vec::with_capacity(NINJA_TYPES.len());
    for (kind, _, _) in NINJA_TYPES {
        let response = transport.send(&overview_request(&league.id, kind))?;
        if response.status != 200 {
            return Err(format!("poe.ninja's {kind} overview answered {}", response.status));
        }
        bodies.push((kind, response.body));
    }
    Ok(bodies)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wraex_core::leagues::Leagues;

    #[test]
    fn private_leagues_skip_both_price_loaders() {
        let league = Leagues::default().selected(Some("Guild (PL123)"), "pc-ggg").unwrap();
        let mut poe1 = crate::poe1::price_check::prices::PriceList::new(Mode::Replay);
        poe1.queue_selected(Some(&league), true);
        assert!(!poe1.initial_loading());
        assert_eq!(poe1.xchg_rate(), None);
        assert_eq!(fetch(&league).unwrap(), Vec::new());
        let unlisted = Leagues::default().selected(Some("Standard"), "pc-ggg").unwrap();
        poe1.queue_selected(Some(&unlisted), true);
        assert_eq!(poe1.xchg_rate(), None, "a Standard recording exists, but an unlisted league skips it");
        assert_eq!(fetch(&unlisted).unwrap(), Vec::new());

        let check = crate::poe1::price_check::PriceCheck::with_mode_league(
            &crate::corpus("adv-rare-ring-mirrored-catalyst"),
            Mode::Replay,
            &league.id,
        );
        assert!(check.trade_link().unwrap().contains(&league.id));
        let check = crate::poe2::price_check::PriceCheck::with_mode_league(
            &crate::corpus_poe2("ee2-rare-item"),
            Mode::Replay,
            &league.id,
        );
        assert!(check.trade_link().unwrap().contains(&league.id));
    }
}
