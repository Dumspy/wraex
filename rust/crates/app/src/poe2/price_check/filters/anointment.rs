//! `FilterModifierAnointment.vue`: the oils an anointed amulet needs, with their icons and
//! total price from poe.ninja.
//!
//! The reference draws the oils' `ITEM_BY_REF` icons and an `ItemQuickPrice` whose price is
//! the sum of the oils' divine values; when any oil has no price it draws nothing at all.
//! The port has no item art, so the oils are placeholder squares; the price comes from
//! [`wraex_poe2::prices`] the same way.

use rune::{El, row, text};
use wraex_poe2::data::Namespace;
use wraex_poe2::filters::StatFilter;

use crate::poe2::price_check::PriceCheck;
use crate::theme::{self, text as type_};
use crate::ui::icon;

/// The anointment's oils and their total, when every oil is priced.
pub fn view(check: &PriceCheck, filter: &StatFilter, index: usize) -> Option<El> {
    let oils = filter.oils.as_ref()?;
    let data = crate::poe2::price_check::data(check.mode());
    let mut total = 0.0;
    let mut icons = row().key(format!("stat-{index}-oils")).items_center();
    for oil in oils.iter().flatten() {
        let item = data.item_by_ref(Namespace::Item, oil).and_then(|items| items.into_iter().next())?;
        let ref_name = item.ref_name.clone()?;
        let price = check.prices.find_price(&wraex_poe2::prices::DbQuery {
            ns: "ITEM".to_owned(),
            name: ref_name,
            variant: None,
        })?;
        total += price.info.primary_value;
        icons = icons.child(icon::image_placeholder(theme::rem(2.0)));
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
