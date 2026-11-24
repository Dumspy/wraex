//! The text and number fields the settings pages are built from: upstream's
//! `<input class="rounded bg-gray-900 px-1 …">` variants.

use rune::{Align, El, TextEdit, text_input};

use crate::theme::{self, text as type_};

/// Which input class a field copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `rounded bg-gray-900 px-1 block w-full font-poe`
    Text,
    /// `rounded bg-gray-900 px-1 block w-full font-sans` (drawn in Exo 2, the system-sans
    /// stand-in)
    Sans,
    /// `rounded bg-gray-900 px-1 block w-16 font-poe text-center`, an `<input type="number">`
    Number,
    /// `rounded bg-gray-900 px-1 mb-1 flex-1`, the private-league box
    Grow,
}

impl Kind {
    fn font(self) -> rune::tok::TextStyle {
        match self {
            Kind::Text | Kind::Number => type_::SM.family(theme::FAMILY_POE),
            Kind::Sans | Kind::Grow => type_::SM.mono(),
        }
    }

    fn align(self) -> Align {
        match self {
            Kind::Number => Align::Center,
            _ => Align::Start,
        }
    }

    fn numeric(self) -> bool {
        self == Kind::Number
    }
}

/// A field bound to `edit`: the value the settings' working copy holds, as text. `on_edit`
/// hands the state's `TextEdit` back, `on_change` writes the edited text into the config.
pub fn text_field<S: 'static>(
    edit: &TextEdit,
    key: &str,
    kind: Kind,
    width: Option<f32>,
    placeholder: &str,
    on_edit: impl for<'a> Fn(&'a mut S) -> &'a mut TextEdit + 'static,
    on_change: impl Fn(&mut S, &mut rune::Tx) + 'static,
) -> El {
    let font = kind.font();
    let mut field = text_input(edit, font)
        .key(key)
        .h(font.line_px())
        .single_line()
        .align(kind.align())
        .radius(theme::ROUNDED)
        .fill(theme::gray(900))
        .caret(theme::gray(300), theme::gray(400).alpha(0.35))
        .color(theme::gray(300))
        .label(if placeholder.is_empty() { key.to_owned() } else { placeholder.to_owned() })
        .placeholder(placeholder, theme::gray(600))
        .placeholder_font(font)
        .on_edit(on_edit)
        .on_input_change(on_change);
    field = match (kind, width) {
        // A long placeholder must not force the field wider than its column (`<input>` has no
        // min-content width).
        (Kind::Text | Kind::Sans, Some(width)) => field.w(width).min_w(0.0),
        (Kind::Text | Kind::Sans, None) => field.w_full().min_w(0.0),
        (Kind::Grow, _) => field.grow(1.0).min_w(0.0),
        (Kind::Number, Some(width)) => field.w(width),
        (Kind::Number, None) => field.w(theme::rem(4.0)),
    };
    if kind.numeric() {
        // `<input type="number">` never takes a letter.
        field = field.on_input_key(|_s: &mut S, event: &rune::KeyEvent, _tx: &mut rune::Tx| match &event.key {
            rune::Key::Text(text) => !text.chars().all(|c| c.is_ascii_digit() || c == '.' || c == '-'),
            _ => false,
        });
    }
    field
}

/// A label above a field: `flex-1 mb-1`.
pub fn label(key: &str, text: &str) -> El {
    rune::text(text).key(key).font(type_::SM).mb(theme::rem(0.25)).truncate()
}

/// A hint line: `italic text-gray-500` (`settings.general`'s blur and log explanations).
pub fn hint(key: &str, text: &str) -> El {
    rune::text(text).key(key).font(type_::SM.italic()).color(theme::gray(500)).mb(theme::rem(1.0)).wrap()
}

/// A `bg-gray-200 text-gray-900 rounded px-1` badge, as `Restart required` and
/// `www.pathofexile.com` are drawn.
pub fn badge(key: &str, text: &str) -> El {
    rune::row()
        .key(key)
        .px(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(200))
        .center_y()
        .child(rune::text(text).key(format!("{key}-label")).font(type_::SM).color(theme::gray(900)))
}
