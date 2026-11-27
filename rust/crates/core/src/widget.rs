//! The config types both references share: `Widget` and `Anchor`
//! (`web/overlay/widgets.ts`) and an entry of `Config.commands` (`web/Config.ts`).
//!
//! Each game's widget configs `#[serde(flatten)]` a [`Widget`] and add their own
//! fields. Numbers are JavaScript numbers (`f64`); serialise through
//! [`crate::js::json_value`] so integral ones are written as JavaScript writes them.

use serde::{Deserialize, Serialize};

/// `Widget`: the fields every widget instance has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub wm_id: f64,
    pub wm_type: String,
    pub wm_title: String,
    pub wm_wants: Wants,
    pub wm_zorder: Option<Zorder>,
    /// `WellKnownFlag`s, or any other string.
    pub wm_flags: Vec<String>,
}

/// `wmWants`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wants {
    Show,
    Hide,
}

/// `wmZorder` when not `null`: a paint order, or `'exclusive'`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Zorder {
    Exclusive(Exclusive),
    Order(f64),
}

/// The string `'exclusive'`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Exclusive {
    #[serde(rename = "exclusive")]
    Exclusive,
}

/// `Anchor`: where a widget sits on the overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    pub pos: String,
    pub x: f64,
    pub y: f64,
}

/// An entry of `Config.commands`: a chat command and its hotkey.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub text: String,
    pub hotkey: Option<String>,
    pub send: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widget_round_trips_as_upstream_writes_it() {
        let text = r#"{"wmId":3,"wmType":"menu","wmTitle":"","wmWants":"show","wmZorder":"exclusive","wmFlags":["menu::skip"]}"#;
        let widget: Widget = serde_json::from_str(text).unwrap();
        assert_eq!(widget.wm_zorder, Some(Zorder::Exclusive(Exclusive::Exclusive)));
        assert_eq!(crate::js::stringify(&widget), text);

        let ordered: Widget = serde_json::from_str(&text.replace(r#""exclusive""#, "2")).unwrap();
        assert_eq!(ordered.wm_zorder, Some(Zorder::Order(2.0)));
        let none: Widget = serde_json::from_str(&text.replace(r#""exclusive""#, "null")).unwrap();
        assert_eq!(crate::js::stringify(&none), text.replace(r#""exclusive""#, "null"));
    }
}
