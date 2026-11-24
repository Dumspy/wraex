//! PoE2's windows: `Exiled Exchange 2`'s price check, ported from
//! `ref/ee2/renderer/src/web/price-check`.
//!
//! The shared widgets ([`crate::ui`]), palette and fonts ([`crate::theme`]),
//! the trade transport ([`crate::trade`]) and the recorded answers
//! ([`crate::replay`]) are the same ones the PoE1 window uses; what changes is
//! EE2's own layout (its `CheckedItem.vue`, `FiltersBlock.vue`, stats without
//! groups, the three bulk currencies) and its strings ([`i18n`]).

pub mod i18n;
pub mod price_check;
