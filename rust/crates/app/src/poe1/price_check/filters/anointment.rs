//! `FilterModifierAnointment.vue`: the oils an anointed amulet needs.
//!
//! Upstream shows the oils' icons and their total price from poe.ninja. The prices are not
//! ported yet (PLAN phase 4's poe.ninja client exists; the widget's store does not), so the
//! icons stand in as placeholder squares and no price is drawn.

use rune::{El, row};
use wraex_poe1::filters::StatFilter;

use crate::theme;
use crate::ui::icon;

/// The oil images, when the filter carries an anointment's oils.
pub fn view(filter: &StatFilter) -> Option<El> {
    let oils = filter.oils.as_ref()?;
    if oils.is_empty() {
        return None;
    }
    let mut out = row().key("anointment-oils").items_center();
    for _ in oils {
        out = out.child(icon::image_placeholder(theme::rem(2.0)));
    }
    Some(out)
}
