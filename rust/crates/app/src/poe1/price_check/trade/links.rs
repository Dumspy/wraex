//! `TradeLinks.vue`: the button that opens the current search on the trade site.
//!
//! Upstream shows two buttons (the built-in browser and an external tab) only when
//! `widget.builtinBrowser` is on; the default is off, so this is the one `Trade` link
//! with the external-link icon.

use rune::{El, row, text};
use wraex_core::trade::api::TradeSite;
use wraex_core::trade::site_link;
use wraex_poe1::trade::{Api, api_to_satisfy_search, create_bulk_request, create_trade_request};

use crate::poe1::i18n;
use crate::poe1::price_check::PriceCheck;
use crate::poe1::price_check::view::Project;
use crate::theme::{self, text as type_};
use crate::ui::icon;

/// The trade site the app talks to (`getTradeEndpoint` for `en`).
pub const HOST: &str = "www.pathofexile.com";

/// `TradeLinks.vue`.
pub fn trade_links<S: 'static>(check: &PriceCheck, _project: Project<S>) -> El {
    let url = link(check);
    let mut link = row()
        .key("trade-link")
        .h(theme::rem(1.25))
        .px(theme::rem(0.5))
        .items_center()
        .gap(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(700))
        .focusable(true)
        .role("button")
        .label(i18n::t("Trade"))
        .child(text(i18n::t("Trade")).key("trade-link-label").font(type_::SM).color(theme::gray(400)))
        .child(icon::external_link(theme::rem(0.75), theme::gray(400)));
    if let Some(url) = url {
        link = link.on_press_tx(move |_s: &mut S, tx: &mut rune::Tx| {
            if let Err(error) = tx.open_url(&url) {
                wraex_platform::log!("wraex: cannot open the trade link {url}: {error}");
            }
        });
    }
    row().key("trade-links").items_center().child(link)
}

/// `makeTradeLink`: the found search's id once there is one, else the query itself, and
/// the exchange query for bulk.
pub fn link(check: &PriceCheck) -> Option<String> {
    let league = &check.item_filters()?.trade.league;
    match api_to_satisfy_search(check.item()?, check.item_stats()) {
        Api::Trade => match &check.trade.list {
            Some(list) => Some(site_link::search_by_id(TradeSite::Trade, HOST, league, &list.id)),
            None => {
                let request =
                    create_trade_request(crate::poe1::price_check::data(), check.item_filters()?, check.item_stats())
                        .ok()?;
                Some(site_link::search_by_query(TradeSite::Trade, HOST, league, &request))
            }
        },
        Api::Bulk => {
            let have = super::listing::have_for(check.trade.selected);
            let request = create_bulk_request(check.item_filters()?, check.item()?, vec![have.to_owned()]);
            // `JSON.stringify({ exchange: httpPostBody.query })`
            let query = serde_json::json!({ "exchange": request.query });
            Some(site_link::exchange_by_query(TradeSite::Trade, HOST, league, &query))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::trade::Mode;
    use rune::{Env, Runtime};

    use super::*;

    /// An empty view: these tests only need a `Tx` to hand Rune a link.
    fn empty_view(_: &(), _: &mut rune::Cx<()>) -> El {
        rune::el()
    }

    /// Every corpus case and preset, with no search run (the `?q=` form): Rune's
    /// `open_url` must accept every link the Trade and Browser buttons open.
    #[test]
    fn every_corpus_preset_links_to_a_page_rune_can_open() {
        let mut runtime = Runtime::new((), empty_view, (16.0, 16.0), Env::new(true, 1.0));
        runtime.frame(0.0);

        let corpus = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tools/golden/corpus/poe1");
        let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(corpus)
            .expect("read the PoE1 corpus")
            .map(|entry| entry.expect("a corpus entry").path())
            .filter(|path| path.extension().and_then(std::ffi::OsStr::to_str) == Some("txt"))
            .collect();
        paths.sort();

        let mut links = 0;
        for path in paths {
            let slug = path.file_stem().and_then(std::ffi::OsStr::to_str).expect("a corpus slug");
            let mut check = PriceCheck::with_mode(&crate::corpus(slug), Mode::Replay);
            // `with_mode` searches the cases the reference searches by itself; the link
            // must be the query form, as it is before any search runs.
            check.trade.list = None;
            let presets: Vec<String> = match &check.presets {
                Some(Ok(presets)) => presets.presets.iter().map(|preset| preset.id.clone()).collect(),
                _ => Vec::new(),
            };
            for preset in presets {
                check.select_preset(&preset);
                check.trade.list = None;
                let Some(url) = link(&check) else { continue };
                runtime.update(|_, tx| {
                    tx.open_url(&url).unwrap_or_else(|error| panic!("{slug} ({preset}): {error}\n{url}"));
                });
                links += 1;
            }
        }
        assert!(links > 0, "the corpus produced no trade links");
        assert_eq!(runtime.take_url_requests().len(), links);
    }
}
