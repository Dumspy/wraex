//! `web/ui/ItemModifierText.vue`: a modifier's text with its rolled number in white.
//!
//! Upstream splits the text on `/(?<![#])[+-]?[#]/gm`: each `#` becomes the roll (or stays `#`
//! when the filter has no roll), and the number is drawn `text-white font-exo2 font-semibold`
//! while the rest is `text-gray-200`. Rune's styled text has no per-span family, so the number's
//! span uses the `mono` slot, which [`crate::theme::FAMILIES`] fills with Exo 2.

use rune::tok::TextStyle;
use rune::{Color, El, Span, rich};
use wraex_core::js;

use crate::theme;

/// The still-unresolved placeholder, as the reference draws it with no roll.
const PLACEHOLDER: &str = "#";

/// `ItemModifierText`: `text` with each `#` replaced by `roll` when it has one.
///
/// The root is keyed `key`; the element's text is the substituted string, so a test or the
/// element tree reads "94% total Elemental Resistance", not the raw filter text.
pub fn item_modifier_text(key: &str, text: &str, roll: Option<f64>, style: TextStyle) -> El {
    let (content, numbers) = render(text, roll);
    let spans =
        numbers.into_iter().map(|(start, end)| Span::new(start, end).color(Color::WHITE).bold().mono()).collect();
    rich(content, spans).key(key).font(style).color(theme::gray(200))
}

/// The string `ItemModifierText` draws, and the byte ranges of the rolled numbers in it.
///
/// HTML collapses runs of whitespace in a text node (a multi-line mod is one line), so the
/// string is normalised first, as the reference's `{{ part.text }}` renders it.
pub fn render(text: &str, roll: Option<f64>) -> (String, Vec<(usize, usize)>) {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let roll = roll.map(js::number_to_string);
    let bytes = text.as_bytes();
    let mut content = String::with_capacity(text.len().saturating_add(4));
    let mut numbers = Vec::new();
    let mut copied = 0;
    let mut previous = None;
    for (i, byte) in bytes.iter().copied().enumerate() {
        let after_hash = previous == Some(b'#');
        let after_sign = matches!(previous, Some(b'+' | b'-'));
        previous = Some(byte);
        if byte != b'#' || after_hash {
            continue;
        }
        // The match starts at the sign when one precedes the `#`, and the sign is dropped.
        let start = if i > copied && after_sign { i.saturating_sub(1) } else { i };
        if let Some(prefix) = text.get(copied..start) {
            content.push_str(prefix);
        }
        let at = content.len();
        content.push_str(roll.as_deref().unwrap_or(PLACEHOLDER));
        if roll.is_some() {
            numbers.push((at, content.len()));
        }
        copied = i.saturating_add(1);
    }
    if let Some(tail) = text.get(copied..) {
        content.push_str(tail);
    }
    (content, numbers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_roll_replaces_the_hash_and_its_sign() {
        assert_eq!(
            render("+#% total Elemental Resistance", Some(94.0)),
            ("94% total Elemental Resistance".to_owned(), vec![(0, 2)])
        );
        assert_eq!(render("+# to maximum Life", Some(82.0)).0, "82 to maximum Life");
        assert_eq!(render("Adds # to # Physical Damage", Some(3.0)).0, "Adds 3 to 3 Physical Damage");
        assert_eq!(
            render("Quality (Life and Mana Modifiers): #%", Some(20.0)).0,
            "Quality (Life and Mana Modifiers): 20%"
        );
    }

    #[test]
    fn no_roll_leaves_the_placeholder() {
        let (content, numbers) = render("+# to maximum Life", None);
        assert_eq!(content, "# to maximum Life");
        assert_eq!(numbers, vec![]);
    }

    /// A two-line mod is one line in HTML, and here too.
    #[test]
    fn whitespace_collapses() {
        assert_eq!(
            render("Commanded leadership over # warriors\nPassives in radius are Conquered", Some(12345.0)).0,
            "Commanded leadership over 12345 warriors Passives in radius are Conquered"
        );
    }
}
