//! `ref/apt/renderer/src/web/stopwatch/WidgetTimer.vue` in Rune: the overlay's stopwatch.
//!
//! `HH:MM:SS` in the mono face, a `paused` badge while it is stopped, and play/pause and reset
//! buttons that only show while the pointer is over the widget (the CSS module's
//! `.wrapper:not(:hover) .controls { display: none }`). A running stopwatch keeps its
//! `invisible-on-blur` flag cleared, so the overlay may lose focus without hiding it; a stopped
//! one under a second re-arms it, as `stop()` does.
//!
//! `toggleKey` and `resetKey` are bound by the platform; the events they produce reach
//! [`crate::widgets::Overlay::widget_action`] as `stopwatch-start-stop:<wmId>` and
//! `stopwatch-reset:<wmId>`, which this widget's state answers.
//!
//! The clock is the runtime's: `AppSpec::poll` hands the app `clock` (virtual in headless runs),
//! and the widget advances `millis` by the difference, exactly as `Date.now()` does upstream;
//! the buttons take the same clock through `Tx::clock()`. A test can therefore start it and
//! `frame(1.0)` to see `00:00:01`.

use rune::tok::TextStyle;
use rune::{Color, El, col, row, text};

use crate::theme::{self, text as type_};
use crate::widgets::{Overlay, Project, Widget, icon};

/// `WidgetSpec.type`; the config calls the widget `timer` and the menu calls it Stopwatch.
pub const WM_TYPE: &str = "timer";

/// `WidgetTimer.vue`'s state: `isRunning`, `millis` and `prevTick`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Timer {
    pub running: bool,
    /// `millis`: milliseconds accumulated while running (the unit `Date.now()` returns).
    pub millis: f64,
    /// `prevTick`: the clock reading the last span was measured from, in milliseconds.
    pub prev_tick: f64,
}

impl Timer {
    /// `start()`: run from `now`.
    pub fn start(&mut self, now_ms: f64) {
        self.running = true;
        self.prev_tick = now_ms;
    }

    /// `reset()`: zero it, or restart at one second while running (`(isRunning) ? 1000 : 0`).
    pub fn reset(&mut self, now_ms: f64) {
        self.prev_tick = now_ms;
        self.millis = if self.running { 1000.0 } else { 0.0 };
    }

    /// `updateTime()`: add the time since the last tick. True when a displayed second changed
    /// (upstream re-renders `formatted` from a one-second interval; the port redraws on the
    /// second, not on every frame).
    pub fn update(&mut self, now_ms: f64) -> bool {
        if !self.running {
            return false;
        }
        let before = self.seconds();
        // The runtime's clock is seconds as `f64`, so a frame's milliseconds can land a hair
        // under a whole one (59.999… s after sixty 1 s frames); `Date.now()`'s are integers, so
        // round to the millisecond the reference would have had.
        self.millis = (self.millis + (now_ms - self.prev_tick).max(0.0)).round();
        self.prev_tick = now_ms;
        self.seconds() != before
    }

    /// The whole seconds counted, `Duration.fromMillis(millis)`'s `h * 3600 + m * 60 + s`.
    fn seconds(&self) -> u64 {
        (self.millis / 1000.0).floor().max(0.0) as u64
    }

    /// `formatted`: `(hours, minutes, seconds)`, each `String(...).padStart(2, '0')` (the seconds
    /// floored, as `Math.floor(dur.seconds)` does).
    pub fn formatted(&self) -> (String, String, String) {
        let total = self.seconds();
        (format!("{:02}", total / 3600), format!("{:02}", (total % 3600) / 60), format!("{:02}", total % 60))
    }
}

/// `.wrapper` with the timer, the `paused` badge and (while hovered) the controls.
pub fn view<S: 'static>(overlay: &Overlay, widget: &Widget, project: Project<S>) -> El {
    let id = widget.wm_id;
    let timer = overlay.timer(id);
    let running = timer.is_some_and(|timer| timer.running);
    let (hours, minutes, seconds) = timer.map(Timer::formatted).unwrap_or_default();
    let hovered = overlay.hover == Some(id);
    let moving = overlay.moving == Some(id);

    // `.timer`: `font-size: 2rem; line-height: 1; @apply font-mono; text-shadow`.
    //
    // `font-mono` is the platform's monospaced face, so every glyph — the two colons included —
    // advances one cell. Each character is centred in a cell as wide as the reference's advance
    // (19.27 pt at 32 pt, Menlo's), so the clock keeps the reference's width whatever face the
    // platform resolves.
    const DIGITS: f32 = 38.53;
    const COLON: f32 = 19.27;
    let time = TextStyle::new(32.0, 400.0).lh(32.0).family(theme::FAMILY_SYSTEM_MONO);
    let digits = |key: &str, value: String| {
        text(value).key(key.to_owned()).w(DIGITS).align(rune::Align::Center).font(time).color(Color::WHITE)
    };
    let colon =
        |key: &str| text(":").key(key.to_owned()).w(COLON).align(rune::Align::Center).font(time).color(Color::WHITE);
    let wrapper = row()
        .key("stopwatch")
        .px(theme::rem(0.5))
        .py(theme::rem(0.25))
        .radius(theme::ROUNDED)
        .fill(theme::gray(300).alpha(0.3))
        .child(
            row()
                .key("stopwatch-time")
                .w(154.13)
                .child(digits("stopwatch-time-hours", hours))
                .child(colon("stopwatch-time-colon-0"))
                .child(digits("stopwatch-time-minutes", minutes))
                .child(colon("stopwatch-time-colon-1"))
                .child(digits("stopwatch-time-seconds", seconds)),
        );

    // The card is the `.widget` box: `paused` and `controls` are `position: absolute` against it
    // (`top: 0; right: 0` and `inset: 0`), not against the padded wrapper.
    let mut card = col().key("stopwatch-card").child(wrapper);
    if !running {
        // `_paused_`: `bg-orange-700 text-white px-2 rounded shadow`, pinned top right.
        card = card.child(
            col()
                .key("stopwatch-paused")
                .pin(None, Some(0.0), Some(0.0), None)
                .px(theme::rem(0.5))
                .radius(theme::ROUNDED)
                .fill(theme::orange(700))
                .shadow(1.0, 3.0, Color::BLACK.alpha(0.35))
                .child(
                    text(crate::settings::t(overlay.game, "stopwatch.paused"))
                        .key("stopwatch-paused-label")
                        .font(theme::leading_none(type_::SM))
                        .color(Color::WHITE),
                ),
        );
    }
    if !moving && hovered {
        card = card.child(controls(overlay, id, running, project));
    }
    card
}

/// `.controls`: the play/pause and reset buttons, 2 × 2 rem, centred over the timer.
fn controls<S: 'static>(overlay: &Overlay, id: u32, running: bool, project: Project<S>) -> El {
    /// `button`: `_button_`'s `background: rgba(29, 29, 29, 0.863)`, 2 × 2 rem, `mx-1`.
    fn button<S: 'static>(key: &str, glyph: El, press: impl Fn(&mut S, &mut rune::Tx) + 'static) -> El {
        row()
            .key(key)
            .size(theme::rem(2.0), theme::rem(2.0))
            .mx(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .fill(rune::color::hex("#1d1d1d").expect("#1d1d1d").alpha(0.863))
            .center()
            .focusable(true)
            .role("button")
            .on_press_tx(move |s: &mut S, tx: &mut rune::Tx| press(s, tx))
            .child(glyph)
    }
    let (play_pause_key, play_pause_icon) = if running {
        ("stopwatch-pause", icon::pause(type_::SM.size, Color::WHITE))
    } else {
        ("stopwatch-play", icon::play(type_::SM.size, Color::WHITE))
    };
    row()
        .key("stopwatch-controls")
        .pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0))
        .center()
        .child(
            button(play_pause_key, play_pause_icon, move |s: &mut S, tx: &mut rune::Tx| {
                let now = tx.clock() * 1000.0;
                project(s).timer_toggle(id, now);
            })
            .label(crate::settings::t(overlay.game, "stopwatch.toggle_key")),
        )
        .child(
            button("stopwatch-reset", icon::redo(type_::SM.size, Color::WHITE), move |s: &mut S, tx: &mut rune::Tx| {
                let now = tx.clock() * 1000.0;
                project(s).timer_reset(id, now);
            })
            .label(crate::settings::t(overlay.game, "stopwatch.reset_key")),
        )
}
