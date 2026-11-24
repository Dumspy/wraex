//! Shared widgets: the primitives every window is built from.
//!
//! Each one matches a component or class in `ref/apt/renderer/src/web/ui` (or a utility class
//! in `web/App.vue`), so a PoE1 and a PoE2 window can be laid out the same way.

pub mod button;
pub mod error_box;
pub mod icon;
pub mod title_bar;

pub use button::button;
pub use error_box::{actions, error_box};
pub use title_bar::{TitleBar, title_bar};
