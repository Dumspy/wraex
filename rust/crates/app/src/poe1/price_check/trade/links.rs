//! `TradeLinks.vue`: the button that opens the current search on the trade site.
//!
//! Upstream shows two buttons (the built-in browser and an external tab) only when
//! `widget.builtinBrowser` is on; the default is off, so this is the one `Trade` link
//! with the external-link icon.

use rune::{El, row, text};
use wraex_core::js;
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
            let _ = tx.open_url(&url);
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
            Some(list) => Some(format!("https://{HOST}/trade/search/{league}/{}", list.id)),
            None => {
                let request =
                    create_trade_request(crate::poe1::price_check::data(), check.item_filters()?, check.item_stats())
                        .ok()?;
                Some(format!("https://{HOST}/trade/search/{league}?q={}", js::stringify(&request)))
            }
        },
        Api::Bulk => {
            let have = super::listing::have_for(check.trade.selected);
            let request = create_bulk_request(check.item_filters()?, check.item()?, vec![have.to_owned()]);
            // `JSON.stringify({ exchange: httpPostBody.query })`
            let query = serde_json::json!({ "exchange": request.query });
            Some(format!("https://{HOST}/trade/exchange/{league}?q={}", js::stringify(&query)))
        }
    }
}
