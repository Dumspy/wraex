//! Shared widgets: the primitives every window is built from.
//!
//! Each one matches a component or class in `ref/apt/renderer/src/web/ui` (or a utility class
//! in `web/App.vue`), so a PoE1 and a PoE2 window can be laid out the same way.

pub mod art;
pub mod button;
pub mod checkbox;
pub mod error_box;
pub mod field;
pub mod icon;
pub mod item_modifier_text;
pub mod item_quick_price;
pub mod radio;
pub mod select;
pub mod title_bar;
pub mod toggle;

pub use button::button;
pub use checkbox::checkbox;
pub use error_box::{actions, error_box};
pub use item_modifier_text::item_modifier_text;
pub use item_quick_price::ItemQuickPrice as QuickPrice;
pub use radio::radio;
pub use title_bar::{TitleBar, title_bar};
pub use toggle::toggle;
