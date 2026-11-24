//! `FilterModifierAnointment.vue`: the oils an anointed amulet needs.
//!
//! Upstream draws each oil's icon (an item from the data layer, so its art is on the CDN) and
//! their total price from poe.ninja. The prices are not ported yet (PLAN phase 4's poe.ninja
//! client exists; the widget's store does not), so only the icons are drawn.

use rune::{El, row};
use wraex_core::GameId;
use wraex_poe1::data::Namespace;
use wraex_poe1::filters::StatFilter;

use crate::poe1::price_check;
use crate::theme;
use crate::ui::art;

/// The oil images, when the filter carries an anointment's oils.
pub fn view(filter: &StatFilter) -> Option<El> {
    let oils = filter.oils.as_ref()?;
    if oils.is_empty() {
        return None;
    }
    let data = price_check::data();
    let mut out = row().key("anointment-oils").items_center();
    for (index, oil) in oils.iter().enumerate() {
        // `ITEM_BY_REF('ITEM', oilName)[0].icon`: `w-8 h-8`.
        let icon = oil
            .and_then(|name| data.item_by_ref(Namespace::Item, name))
            .and_then(|items| items.into_iter().next())
            .map(|item| item.icon);
        out = match icon {
            Some(url) => out.child(
                art::cdn(GameId::Poe1, &url, (theme::rem(2.0), theme::rem(2.0))).key(format!("anointment-oil-{index}")),
            ),
            // A missing item is a gap upstream too (`ITEMS_BY_REF` returns nothing to draw).
            None => out,
        };
    }
    Some(out)
}
