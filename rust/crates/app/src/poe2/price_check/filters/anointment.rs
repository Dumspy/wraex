//! `FilterModifierAnointment.vue`: the oils an anointed amulet needs, with their icons and
//! total price from poe.ninja.
//!
//! The reference draws the oils' `ITEM_BY_REF` icons and an `ItemQuickPrice` whose price is
//! the sum of the oils' divine values; when any oil has no price it draws nothing at all.
//! The icons are the items' own CDN art ([`crate::ui::art`]) and the price comes from
//! [`wraex_poe2::prices`] the same way.

use rune::{El, row, text};
use wraex_poe2::data::Namespace;
use wraex_poe2::filters::StatFilter;

use crate::poe2::price_check::PriceCheck;
use crate::theme::{self, text as type_};
use crate::ui::art;

/// The anointment's oils and their total, when every oil is priced.
pub fn view(check: &PriceCheck, filter: &StatFilter, index: usize) -> Option<El> {
    let oils = filter.oils.as_ref()?;
    let data = crate::poe2::price_check::data(check.mode());
    let mut total = 0.0;
    let mut icons = row().key(format!("stat-{index}-oils")).items_center();
    for (oil_index, oil) in oils.iter().flatten().enumerate() {
        let item = data.item_by_ref(Namespace::Item, oil).and_then(|items| items.into_iter().next())?;
        let ref_name = item.ref_name.clone()?;
        let price = check.prices.find_price(&wraex_poe2::prices::DbQuery {
            ns: "ITEM".to_owned(),
            name: ref_name,
            variant: None,
        })?;
        total += price.info.primary_value;
        // `:src="oil.icon"`, or `/images/404.png` when the data has none: `w-8 h-8`.
        let size = (theme::rem(2.0), theme::rem(2.0));
        let picture = if item.icon.is_empty() || item.icon == "%NOT_FOUND%" {
            art::local_sized(wraex_core::GameId::Poe2, "404.png", size)
        } else {
            art::cdn(wraex_core::GameId::Poe2, &item.icon, size)
        };
        icons = icons.child(picture.key(format!("stat-{index}-oil-{oil_index}")));
    }
    let price = check.prices.auto_currency(total, false);
    Some(
        row().key(format!("stat-{index}-anointment")).items_center().gap(theme::rem(0.25)).child(icons).child(
            text(format!("{} ×", wraex_poe2::prices::display_rounding(price.min, false, false)))
                .key(format!("stat-{index}-anointment-price"))
                .font(type_::SM),
        ),
    )
}
