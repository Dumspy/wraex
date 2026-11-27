//! The quick-math bar (issue #6): Wraex's own spot to do trade maths —
//! `1.5k / 3 + (20 - 2.5) * 4` — before copying the result.
//!
//! Neither reference has this, so there is nothing to port. The evaluator is
//! [`wraex_core::quick_math`], which knows numbers, `+ - * /`, parentheses and the `k`/`m`
//! suffixes; this module holds the bar's state and its card: a single-line field, the live
//! result under it, and the history ArrowUp/ArrowDown walk.
//!
//! [`evaluate`] is the one place an expression is evaluated, so the two follow-ups plug in
//! there and nowhere else: issue #9 (saved snippets) looks a command up before evaluating
//! it, and issue #7 (currency-aware maths) calls
//! [`wraex_core::quick_math::evaluate_with`] with its own [`Units`](wraex_core::quick_math::Units).
//!
//! The live shell ([`crate::shell`]) wires the bar to the config's `quickMathKey`: a hotkey
//! opens it at the cursor as one more card in the overlay, without taking the game's focus.
//! On Windows the global keyboard hook holds back the keys that type into it until the overlay
//! takes the focus, then types them again ([`wraex_platform::window::retype`]); Enter copies
//! the plain value through the shell's clipboard, not `restoreClipboard`.

use rune::{Color, El, col, row, text, text_input};
use wraex_core::quick_math::{Error, format_grouped};

use crate::theme::{self, text as type_};

/// The scene/card name, as `--scene quick-math` and the overlay's widget list spell it.
pub const WM_TYPE: &str = "quick-math";

/// The config's root key holding the bar's hotkey. Wraex's own: neither reference has a bar.
pub const CONFIG_KEY: &str = "quickMathKey";

/// The hotkey a config without [`CONFIG_KEY`] gets. No default hotkey of either reference uses it.
pub const DEFAULT_KEY: &str = "Alt + M";

/// Give a config the bar's default hotkey when it has no [`CONFIG_KEY`] at all. An explicit
/// `null` or `""` is the user clearing it (the settings recorder writes `null` on Backspace),
/// and is kept.
pub fn fill_defaults(config: &mut serde_json::Value) {
    if let Some(object) = config.as_object_mut() {
        object.entry(CONFIG_KEY.to_owned()).or_insert_with(|| serde_json::Value::String(DEFAULT_KEY.to_owned()));
    }
}

/// The configured hotkey, if the config has a non-empty one.
pub fn configured_key(config: &serde_json::Value) -> Option<String> {
    config.get(CONFIG_KEY).and_then(serde_json::Value::as_str).filter(|key| !key.is_empty()).map(str::to_owned)
}

/// How many submitted expressions ArrowUp/ArrowDown walk; older ones fall off the front.
pub const HISTORY: usize = 20;

/// The card's width: 24 rem, the item search's minimum too (`theme::rem(24.0)`, which is not
/// `const`).
pub const WIDTH: f32 = 24.0 * theme::REM;

/// The card's height: its content at [`WIDTH`] — the 0.75 rem padding twice (24 pt), the XL
/// field's line (28 pt), the 0.5 rem gap (8 pt), the LG result line (28 pt) and the 1 pt border
/// on each side, inside the box (`border_layout`).
pub const HEIGHT: f32 = 90.0;

/// What the empty field hints at: Enter copies the value, Escape closes the bar.
const HINT: &str = "Enter copies \u{b7} Esc closes";

/// What the empty field shows before anything is typed.
const PLACEHOLDER: &str = "1.5k / 3 + 20";

/// The bar's state: whether it is open, what is typed, and the history.
#[derive(Debug, Clone, Default)]
pub struct QuickMath {
    /// Whether the card is drawn at all.
    pub open: bool,
    /// The expression field.
    pub input: rune::TextEdit,
    /// The submitted expressions, oldest first, consecutive duplicates collapsed and capped at
    /// [`HISTORY`].
    history: Vec<String>,
    /// The index into [`QuickMath::history`] ArrowUp/Down walk, while they walk it.
    browsing: Option<usize>,
    /// What was typed before the walk started; walking past the newest entry restores it.
    draft: String,
    /// What Enter produced, for the shell to copy. [`QuickMath::take_copied`] hands it over.
    copied: Option<String>,
}

impl QuickMath {
    /// Summon the bar: open it with an empty field. The history is kept. The field takes the
    /// keyboard as soon as the window has it: Rune focuses a text field through its model, which
    /// is what makes the first typed key land in the bar rather than the game.
    pub fn open(&mut self) {
        self.open = true;
        self.input.focused = true;
        self.input.replace_text("");
        self.stop_browsing();
    }

    /// Dismiss the bar. The field keeps what was typed, as a hidden widget's state does.
    pub fn close(&mut self) {
        self.open = false;
    }

    /// `open ? close : open`.
    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open();
        }
    }

    /// Replace the field's contents (scenes and tests), with the caret at the end.
    pub fn set_input(&mut self, text: &str) {
        self.input.replace_text(text);
        self.stop_browsing();
    }

    /// What is typed.
    pub fn text(&self) -> &str {
        &self.input.text
    }

    /// The field's value, or why it is not a number yet.
    pub fn result(&self) -> Result<f64, Error> {
        evaluate(self.text())
    }

    /// Enter: on a value, record the expression, hand [`format_plain`] to the shell, close, and
    /// return it. Anything else changes nothing.
    ///
    /// [`format_plain`]: wraex_core::quick_math::format_plain
    pub fn submit(&mut self) -> Option<String> {
        let Ok(value) = self.result() else {
            return None;
        };
        let expression = self.text().trim().to_owned();
        if self.history.last() != Some(&expression) {
            if self.history.len() == HISTORY {
                self.history.remove(0);
            }
            self.history.push(expression);
        }
        self.stop_browsing();
        self.close();
        let copied = wraex_core::quick_math::format_plain(value);
        self.copied = Some(copied.clone());
        Some(copied)
    }

    /// The value Enter produced, once.
    pub fn take_copied(&mut self) -> Option<String> {
        self.copied.take()
    }

    /// ArrowUp: the previous expression into the field. The first Up saves the draft; Up at the
    /// oldest entry does nothing and answers false.
    pub fn history_back(&mut self) -> bool {
        let next = match self.browsing {
            Some(index) if index > 0 => index - 1,
            Some(_) => return false,
            None if self.history.is_empty() => return false,
            None => {
                self.draft = self.text().to_owned();
                self.history.len() - 1
            }
        };
        self.browsing = Some(next);
        let expression = self.history[next].clone();
        self.input.replace_text(expression);
        true
    }

    /// ArrowDown: the next expression, or the draft after the newest one. False while the walk
    /// has not started.
    pub fn history_forward(&mut self) -> bool {
        let Some(index) = self.browsing else {
            return false;
        };
        if index + 1 < self.history.len() {
            self.browsing = Some(index + 1);
            let expression = self.history[index + 1].clone();
            self.input.replace_text(expression);
        } else {
            self.browsing = None;
            let draft = std::mem::take(&mut self.draft);
            self.input.replace_text(draft);
        }
        true
    }

    /// The submitted expressions, oldest first.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// End a history walk without touching the field, as typing does.
    fn stop_browsing(&mut self) {
        self.browsing = None;
        self.draft.clear();
    }
}

/// The one place an expression is evaluated: #9's snippet lookup and #7's units plug in here.
fn evaluate(text: &str) -> Result<f64, Error> {
    wraex_core::quick_math::evaluate(text)
}

/// The bar: the field on top, the live result under it. A Spotlight-like card, [`WIDTH`] wide
/// and content-high.
pub fn view<S: 'static>(state: &QuickMath, project: fn(&mut S) -> &mut QuickMath) -> El {
    let input = text_input(&state.input, type_::XL)
        .key("quick-math-input")
        .w_full()
        .min_w(0.0)
        .h(type_::XL.line_px())
        .single_line()
        // Upstream's fields are transparent on their card; only the card has a fill.
        .fill(Color::TRANSPARENT)
        .caret(theme::gray(300), theme::gray(400).alpha(0.35))
        .color(theme::gray(100))
        .label("Quick math")
        // The window's card is the only thing to type into, so it takes the keyboard when the
        // window gets focus.
        .autofocus()
        .placeholder(PLACEHOLDER, theme::gray(600))
        .placeholder_font(type_::XL)
        .on_edit(move |s: &mut S| &mut project(s).input)
        // Whatever changed the text — a key, a paste — ends a history walk.
        .on_input_change(move |s: &mut S, _tx: &mut rune::Tx| project(s).stop_browsing())
        .on_submit(move |s: &mut S, _tx: &mut rune::Tx| {
            project(s).submit();
        })
        .on_input_key(move |s: &mut S, event: &rune::KeyEvent, _tx: &mut rune::Tx| {
            let math = project(s);
            match &event.key {
                rune::Key::Up => math.history_back(),
                rune::Key::Down => math.history_forward(),
                rune::Key::Escape => {
                    math.close();
                    true
                }
                // Typing starts a new expression: the walk is over before the edit lands.
                rune::Key::Text(_) | rune::Key::Backspace | rune::Key::Delete => {
                    math.stop_browsing();
                    false
                }
                _ => false,
            }
        });

    col()
        .key(WM_TYPE)
        .w(WIDTH)
        .padding(theme::rem(0.75))
        .gap(theme::rem(0.5))
        .radius(theme::ROUNDED)
        .fill(theme::gray(800))
        .border(1.0, theme::gray(700))
        .border_layout()
        .child(input)
        // A fixed line keeps the card from resizing as the result appears, changes or fails.
        .child(row().key("quick-math-result-row").h(type_::LG.line_px()).items_center().child(result(state)))
}

/// The line under the field: the grouped value, a hint while the field is empty, an ellipsis
/// while the expression is still being typed, or the error.
fn result(state: &QuickMath) -> El {
    let key = "quick-math-result";
    let line = match state.result() {
        Ok(value) => text(format!("= {}", format_grouped(value))).key(key).font(type_::LG).color(theme::gray(100)),
        Err(Error::Empty) => text(HINT).key(key).font(type_::SM).color(theme::gray(500)),
        // `5 +` and `(1 + 2`: the user is mid-expression, which is not an error yet.
        Err(Error::UnexpectedEnd | Error::UnclosedParen { .. }) => {
            text("\u{2026}").key(key).font(type_::SM).color(theme::gray(500))
        }
        Err(error) => text(error.to_string()).key(key).font(type_::SM).color(theme::red(400)),
    };
    line.truncate().min_w(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bar that submitted `expressions`, oldest first.
    fn submitted(expressions: &[&str]) -> QuickMath {
        let mut bar = QuickMath::default();
        for expression in expressions {
            bar.set_input(expression);
            assert!(bar.submit().is_some(), "{expression:?} should evaluate");
        }
        bar
    }

    #[test]
    fn opening_clears_the_field_but_keeps_the_history() {
        let mut bar = submitted(&["1+1"]);
        bar.open();
        assert!(bar.open);
        assert_eq!(bar.text(), "");
        assert_eq!(bar.history(), ["1+1"]);
    }

    #[test]
    fn submit_copies_the_plain_value_and_closes() {
        let mut bar = QuickMath::default();
        bar.open();
        bar.set_input("1.5k / 3");
        assert_eq!(bar.submit().as_deref(), Some("500"));
        assert!(!bar.open);
        assert_eq!(bar.history(), ["1.5k / 3"]);
        assert_eq!(bar.take_copied().as_deref(), Some("500"));
        assert_eq!(bar.take_copied(), None, "handed over once");
    }

    #[test]
    fn submit_of_an_invalid_expression_changes_nothing() {
        let mut bar = QuickMath::default();
        bar.open();
        bar.set_input("5 / 0");
        assert_eq!(bar.submit(), None);
        assert!(bar.open, "the bar stays up so the expression can be fixed");
        assert!(bar.history().is_empty());
        assert_eq!(bar.take_copied(), None);

        bar.set_input("");
        assert_eq!(bar.submit(), None);
        assert!(bar.history().is_empty());
    }

    #[test]
    fn a_consecutive_duplicate_is_recorded_once() {
        let mut bar = QuickMath::default();
        for _ in 0..3 {
            bar.set_input("2+2");
            assert!(bar.submit().is_some());
        }
        assert_eq!(bar.history(), ["2+2"]);
    }

    #[test]
    fn the_history_is_capped_at_its_limit() {
        let expressions: Vec<String> = (0..HISTORY + 5).map(|n| format!("{n}+0")).collect();
        let mut bar = QuickMath::default();
        for expression in &expressions {
            bar.set_input(expression);
            assert!(bar.submit().is_some());
        }
        assert_eq!(bar.history().len(), HISTORY);
        assert_eq!(bar.history().first().map(String::as_str), Some("5+0"), "the oldest five fell off");
        assert_eq!(bar.history().last().map(String::as_str), Some("24+0"));
    }

    #[test]
    fn the_history_walk_restores_the_draft_after_the_newest() {
        let mut bar = submitted(&["1+1", "2+2"]);
        bar.open();
        bar.set_input("9+9");

        assert!(bar.history_back());
        assert_eq!(bar.text(), "2+2");
        assert!(bar.history_back());
        assert_eq!(bar.text(), "1+1");
        assert!(!bar.history_back(), "there is nothing older");
        assert_eq!(bar.text(), "1+1");

        assert!(bar.history_forward());
        assert_eq!(bar.text(), "2+2");
        assert!(bar.history_forward());
        assert_eq!(bar.text(), "9+9", "the draft came back");
        assert!(!bar.history_forward(), "not walking any more");
    }

    #[test]
    fn back_with_no_history_answers_false() {
        let mut bar = QuickMath::default();
        bar.open();
        assert!(!bar.history_back());
        assert_eq!(bar.text(), "");
    }

    #[test]
    fn forward_without_a_walk_answers_false() {
        let mut bar = submitted(&["1+1"]);
        bar.open();
        assert!(!bar.history_forward());
        assert_eq!(bar.text(), "");
    }

    #[test]
    fn opening_ends_a_walk() {
        let mut bar = submitted(&["1+1"]);
        bar.open();
        assert!(bar.history_back());
        assert_eq!(bar.text(), "1+1");
        bar.close();
        bar.open();
        assert_eq!(bar.text(), "");
        assert!(!bar.history_forward(), "the walk is over");
    }

    #[test]
    fn toggle_flips_the_open_state() {
        let mut bar = QuickMath::default();
        assert!(!bar.open, "closed by default");
        bar.toggle();
        assert!(bar.open);
        bar.toggle();
        assert!(!bar.open);
    }

    #[test]
    fn giving_a_config_the_default_key_leaves_a_cleared_one_alone() {
        // A config without the key gets the default.
        let mut config = serde_json::json!({ "overlayKey": "Shift + Space" });
        fill_defaults(&mut config);
        assert_eq!(configured_key(&config).as_deref(), Some(DEFAULT_KEY));
        assert_eq!(config[CONFIG_KEY], serde_json::json!(DEFAULT_KEY));

        // `null` and `""` are the user clearing it: both stay.
        let mut cleared = serde_json::json!({ CONFIG_KEY: null });
        fill_defaults(&mut cleared);
        assert_eq!(cleared[CONFIG_KEY], serde_json::Value::Null);
        assert_eq!(configured_key(&cleared), None);
        let mut empty = serde_json::json!({ CONFIG_KEY: "" });
        fill_defaults(&mut empty);
        assert_eq!(configured_key(&empty), None);

        // A configured key is kept, including one that is not the default.
        let mut custom = serde_json::json!({ CONFIG_KEY: "Ctrl + Q" });
        fill_defaults(&mut custom);
        assert_eq!(configured_key(&custom).as_deref(), Some("Ctrl + Q"));
        assert_eq!(configured_key(&serde_json::json!({})), None);
    }
}
