//! Fakes for the platform state machines' tests: each OS trait as a recording.
//!
//! Not part of the crate's API: the tests build a sequence of calls and assert it
//! exactly, so the order and the delays the ported logic uses are the test data.

use std::cell::Cell;
use std::collections::VecDeque;

use wraex_core::trade::api::Clock;
use wraex_core::trade::rate_limit::Millis;

use crate::clipboard::Clipboard;
use crate::overlay::OverlayHandle;
use crate::shortcuts::KeySender;

/// A clock that moves forward when something sleeps.
#[derive(Debug, Default)]
pub struct FakeClock(Cell<Millis>);

impl FakeClock {
    /// The time the logic waited, in milliseconds.
    pub fn elapsed(&self) -> Millis {
        self.0.get()
    }

    /// Lets a test fire a timer (`setTimeout`) at the right moment.
    pub fn advance(&self, millis: Millis) {
        self.0.set(self.0.get() + millis);
    }
}

impl Clock for FakeClock {
    fn now(&self) -> Millis {
        self.0.get()
    }

    fn sleep(&self, millis: Millis) {
        self.advance(millis);
    }
}

/// A scripted clipboard: reads take the next text and the last one repeats, writes
/// are recorded.
#[derive(Debug, Default)]
pub struct FakeClipboard {
    texts: VecDeque<String>,
    written: Vec<String>,
}

impl FakeClipboard {
    pub fn scripted(texts: &[&str]) -> Self {
        Self { texts: texts.iter().map(|text| (*text).to_owned()).collect(), written: Vec::new() }
    }

    /// Everything `writeText` wrote, in order.
    pub fn written(&self) -> Vec<String> {
        self.written.clone()
    }
}

impl Clipboard for FakeClipboard {
    fn read_text(&mut self) -> String {
        if self.texts.len() > 1 {
            self.texts.pop_front().expect("checked")
        } else {
            self.texts.front().cloned().unwrap_or_default()
        }
    }

    fn write_text(&mut self, text: &str) {
        self.written.push(text.to_owned());
    }
}

/// One [`KeySender`] call, recorded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyStep {
    Down(String),
    Up(String),
    /// `keyTap`: the modifiers are applied around the key.
    Tap {
        key: String,
        modifiers: Vec<String>,
    },
    /// Upstream's `setTimeout` between steps.
    Sleep(Millis),
}

/// Records the synthetic keys the ported logic sends.
#[derive(Debug, Default)]
pub struct FakeKeySender {
    steps: Vec<KeyStep>,
}

impl FakeKeySender {
    pub fn steps(&self) -> Vec<KeyStep> {
        self.steps.clone()
    }
}

impl KeySender for FakeKeySender {
    fn key_down(&mut self, key: &str) {
        self.steps.push(KeyStep::Down(key.to_owned()));
    }

    fn key_up(&mut self, key: &str) {
        self.steps.push(KeyStep::Up(key.to_owned()));
    }

    fn key_tap(&mut self, key: &str, modifiers: &[&str]) {
        self.steps.push(KeyStep::Tap {
            key: key.to_owned(),
            modifiers: modifiers.iter().map(|modifier| (*modifier).to_owned()).collect(),
        });
    }

    fn sleep(&mut self, millis: Millis) {
        self.steps.push(KeyStep::Sleep(millis));
    }
}

/// One [`OverlayHandle`] call, recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayCall {
    Show,
    Hide,
    ClickThrough(bool),
    FocusGame,
}

/// Records the overlay's controls. `show`/`hide` are idempotent, like the real
/// window, so only a change is recorded.
#[derive(Debug, Default)]
pub struct FakeOverlay {
    visible: bool,
    calls: Vec<OverlayCall>,
}

impl FakeOverlay {
    pub fn calls(&self) -> Vec<OverlayCall> {
        self.calls.clone()
    }

    /// Forgets the calls so far, for the next phase of a test.
    pub fn clear(&mut self) {
        self.calls.clear();
    }
}

impl OverlayHandle for FakeOverlay {
    fn show(&mut self) {
        if !self.visible {
            self.visible = true;
            self.calls.push(OverlayCall::Show);
        }
    }

    fn hide(&mut self) {
        if self.visible {
            self.visible = false;
            self.calls.push(OverlayCall::Hide);
        }
    }

    fn set_click_through(&mut self, click_through: bool) {
        self.calls.push(OverlayCall::ClickThrough(click_through));
    }

    fn focus_game(&mut self) {
        self.calls.push(OverlayCall::FocusGame);
    }

    fn is_visible(&self) -> bool {
        self.visible
    }
}
