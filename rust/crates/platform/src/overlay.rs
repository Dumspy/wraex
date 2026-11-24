//! The transparent, click-through overlay that follows the game window.
//!
//! Ported from `main/src/windowing/{OverlayWindow,OverlayVisibility,WidgetAreaTracker}.ts`.
//! Those three files are the overlay's whole state machine; the rendering is the
//! renderer's. [`OverlayController`] keeps the state and emits [`OverlayEvent`]s
//! for the renderer:
//!
//! - **Game focused / overlay active.** The overlay is *interactable* when a widget
//!   takes the mouse: it is shown, click-through is off and the game loses focus
//!   ([`OverlayController::assert_overlay_active`]); [`OverlayController::assert_game_active`]
//!   gives the focus back. The overlay key toggles between them.
//! - **The events the renderer sees.** `focus-change` (`game`, `overlay`,
//!   `usingHotkey`), `overlay-attached`, `visibility` (hold Alt to look at the game
//!   underneath) and `hide-exclusive-widget`.
//! - **The widget area.** After a price check, the renderer reports the widget's
//!   rectangle; the mouse entering it activates the overlay, and moving away from
//!   the press position closes the exclusive widget (`WidgetAreaTracker.ts`,
//!   folded into [`OverlayController::on_track_area`] and friends).
//! - **Keys inside the overlay.** Escape and Ctrl+W give the game its focus back,
//!   and the overlay key toggles ([`OverlayController::on_overlay_input`]).
//! - **EE2's hide-on-blur.** EE2 can hide the overlay window while the game is
//!   unfocused, and its renderer reports whether it has anything to show
//!   (`render-state`); both live behind [`GameId`] and are off for PoE1, whose
//!   renderer has neither.
//!
//! What stays OS-specific, behind [`OverlayHandle`]: Rune owns the overlay surface
//! (`WindowOptions::overlay` and the raw window handle) and the click-through
//! toggle; the window's position comes from [`crate::window`]. Upstream's
//! `electron-overlay-window` also draws the frame and follows the game window;
//! following it is ours (Win32 `SetWinEventHook`, X11, Wayland layer-shell, which
//! cannot do interactive overlays at all — PLAN.md notes that loss). The
//! administrator-rights check that produces [`OverlayEvent::NoAccess`] is Win32's
//! (`OpenProcess`/`GetTokenInformation` on the game's process); the dialog itself
//! is the app's.

use wraex_core::GameId;
use wraex_core::game_files::Os;
use wraex_core::trade::api::Clock;
use wraex_core::trade::rate_limit::Millis;

use crate::shortcuts::{HostConfig, InputEvent};
use crate::window::{Bounds, GameWindow, Point, WindowEvent};

/// The overlay's controls, as the OS layer provides them (upstream's
/// `OverlayController.activateOverlay`/`focusTarget` and the window's visibility).
pub trait OverlayHandle: std::fmt::Debug {
    /// Make the overlay visible if it is not (`showInactive` + always on top).
    /// Idempotent.
    fn show(&mut self);
    /// Hide the overlay window. Idempotent.
    fn hide(&mut self);
    /// `true` lets the game take the mouse and keyboard again; `false` makes the
    /// overlay interactable (`OverlayController.activateOverlay`).
    fn set_click_through(&mut self, click_through: bool);
    /// Give the focus back to the game window (`OverlayController.focusTarget`).
    fn focus_game(&mut self);
    fn is_visible(&self) -> bool;
}

/// How long `OverlayVisibility` waits before hiding the overlay when the game has
/// the focus, and when the overlay is interactable.
pub const VISIBILITY_HIDE_DELAY: Millis = 275;
pub const VISIBILITY_HIDE_DELAY_INTERACTABLE: Millis = 85;

/// What the renderer is told, in upstream's `IpcEvent` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayEvent {
    /// `MAIN->OVERLAY::overlay-attached`.
    Attached,
    /// The game runs with administrator rights and cannot be reached; upstream
    /// shows an error box (`PoE window - No access`).
    NoAccess,
    /// `MAIN->OVERLAY::focus-change`: who has the focus, and whether the overlay
    /// key caused it.
    FocusChange { game: bool, overlay: bool, using_hotkey: bool },
    /// `MAIN->OVERLAY::hide-exclusive-widget`: the cursor moved away from the
    /// widget.
    HideExclusiveWidget,
    /// `MAIN->OVERLAY::visibility`: the Alt peek.
    Visibility { is_visible: bool },
}

/// The area the renderer wants tracked (`OVERLAY->MAIN::track-area`, CSS pixels).
#[derive(Debug, Clone, PartialEq)]
pub struct TrackArea {
    /// The modifier the user must hold for the widget to stay open
    /// (`priceCheck.hotkeyHold`). `None` is JavaScript's `undefined` — a hotkey
    /// with no hold key at all; an empty string is not the same thing.
    pub hold_key: Option<String>,
    /// How far the cursor may move from `from` before the widget closes.
    pub close_threshold: f64,
    /// The item's position, where the hotkey fired.
    pub from: Point,
    pub area: Bounds,
    /// `window.devicePixelRatio`, for the Win32 conversion below.
    pub dpr: f64,
}

/// The tracked area in screen coordinates.
#[derive(Debug, Clone, PartialEq)]
struct TrackedArea {
    hold_key: Option<String>,
    close_threshold: f64,
    from: Point,
    area: Bounds,
}

impl TrackArea {
    /// Win32 reports mouse positions in physical pixels while the renderer's
    /// rectangle is in CSS pixels; upstream converts on `win32` only
    /// (`screen.dipToScreenPoint`/`dipToScreenRect`, and Electron rounds the
    /// rectangle first). On macOS and Linux Electron is already in screen
    /// coordinates. The OS layer may also convert itself and pass `dpr: 1.0`.
    fn into_screen_space(self, os: Os) -> TrackedArea {
        let TrackArea { hold_key, close_threshold, from, area, dpr } = self;
        if os != Os::Windows {
            return TrackedArea { hold_key, close_threshold, from, area };
        }
        TrackedArea {
            hold_key,
            close_threshold: close_threshold * dpr,
            from: Point { x: from.x * dpr, y: from.y * dpr },
            area: Bounds {
                x: area.x.round() * dpr,
                y: area.y.round() * dpr,
                width: area.width.round() * dpr,
                height: area.height.round() * dpr,
            },
        }
    }
}

/// The keys pressed inside the overlay window, from Electron's
/// `before-input-event`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayInput {
    /// Electron's `input.code`: `Escape`, `KeyW`, `Digit1`, `Space`, ...
    pub code: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl OverlayInput {
    /// The hotkey string `handleExtraCommands` builds: the `Key`/`Digit` prefix
    /// is dropped and the modifiers go in front, in upstream's order (which has
    /// no `Ctrl + Shift + Alt` case).
    pub fn hotkey(&self) -> String {
        let code = self.code.strip_prefix("Key").or_else(|| self.code.strip_prefix("Digit")).unwrap_or(&self.code);
        if self.shift && self.alt {
            format!("Shift + Alt + {code}")
        } else if self.ctrl && self.shift {
            format!("Ctrl + Shift + {code}")
        } else if self.ctrl && self.alt {
            format!("Ctrl + Alt + {code}")
        } else if self.alt {
            format!("Alt + {code}")
        } else if self.ctrl {
            format!("Ctrl + {code}")
        } else if self.shift {
            format!("Shift + {code}")
        } else {
            code.to_owned()
        }
    }
}

/// `OverlayVisibility.ts`: hold Alt to look at the game, release it (or move the
/// mouse) to bring the overlay back. The hide waits, so tapping Alt does not
/// flicker the widgets.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Visibility {
    is_visible: bool,
    /// The pending hide's deadline (`setTimeout`'s id).
    hide_at: Option<Millis>,
}

impl Visibility {
    fn new() -> Self {
        Self { is_visible: true, hide_at: None }
    }

    /// `makeVisible`: `Some(true)` when the overlay comes back now, `None` when it
    /// already was visible or a pending hide was only cancelled.
    fn make_visible(&mut self) -> Option<bool> {
        if self.is_visible && self.hide_at.is_none() {
            return None;
        }
        if self.hide_at.take().is_some() {
            return None;
        }
        self.is_visible = true;
        Some(true)
    }

    /// `makeInvisible`: schedule the hide, unless one is pending already or the
    /// overlay is hidden.
    fn make_invisible(&mut self, interactable: bool, clock: &dyn Clock) {
        if !self.is_visible || self.hide_at.is_some() {
            return;
        }
        let delay = if interactable { VISIBILITY_HIDE_DELAY_INTERACTABLE } else { VISIBILITY_HIDE_DELAY };
        self.hide_at = Some(clock.now() + delay);
    }
}

/// `WidgetAreaTracker.ts`: the widget's rectangle and the mouse that moves in and
/// out of it.
#[derive(Debug, Clone, Default, PartialEq)]
struct AreaTracker {
    tracked: Option<TrackedArea>,
}

impl AreaTracker {
    fn track(&mut self, area: TrackArea, os: Os) {
        self.tracked = Some(area.into_screen_space(os));
    }

    fn stop(&mut self) {
        self.tracked = None;
    }
}

/// `OverlayWindow.ts`'s state machine plus `OverlayVisibility.ts`'s Alt peek and
/// `WidgetAreaTracker.ts`'s follow-the-cursor behaviour.
#[derive(Debug, Clone, PartialEq)]
pub struct OverlayController {
    game: GameId,
    os: Os,
    /// `isInteractable`: the overlay takes the mouse and keyboard.
    is_interactable: bool,
    /// `wasUsedRecently`: whether the last window the user used was the overlay.
    /// A fresh copy may then focus the widget without stealing focus from a game
    /// the user just came back to.
    was_used_recently: bool,
    /// The key that toggles the overlay from inside it (`overlayKey`).
    overlay_key: String,
    /// `isOverlayKeyUsed`: whether this focus change came from that key.
    is_overlay_key_used: bool,
    /// EE2's `shouldShowOverlay`, from the renderer's `render-state`.
    should_show_overlay: bool,
    /// EE2's `hideOverlayOnBlur`: hide the window while the game is unfocused.
    hide_overlay_on_blur: bool,
    tracker: AreaTracker,
    visibility: Visibility,
    events: Vec<OverlayEvent>,
}

impl OverlayController {
    pub fn new(game: GameId, os: Os) -> Self {
        Self {
            game,
            os,
            is_interactable: false,
            was_used_recently: true,
            overlay_key: "Shift + Space".to_owned(),
            is_overlay_key_used: false,
            should_show_overlay: true,
            hide_overlay_on_blur: false,
            tracker: AreaTracker::default(),
            visibility: Visibility::new(),
            events: Vec::new(),
        }
    }

    /// `isInteractable`: a widget is taking the mouse.
    pub fn is_interactable(&self) -> bool {
        self.is_interactable
    }

    /// `wasUsedRecently`.
    pub fn was_used_recently(&self) -> bool {
        self.was_used_recently
    }

    /// `CLIENT->MAIN::used-recently`: the renderer reports whether the user's last
    /// window was the overlay or the game.
    pub fn set_was_used_recently(&mut self, is_overlay: bool) {
        self.was_used_recently = is_overlay;
    }

    /// `overlayKey`.
    pub fn overlay_key(&self) -> &str {
        &self.overlay_key
    }

    /// `OverlayVisibility`'s current state.
    pub fn is_visible(&self) -> bool {
        self.visibility.is_visible
    }

    /// When the Alt peek's timer should call [`Self::poll`].
    pub fn visibility_deadline(&self) -> Option<Millis> {
        self.visibility.hide_at
    }

    /// Everything that happened since the last call, for the renderer and the
    /// app's wiring.
    pub fn take_events(&mut self) -> Vec<OverlayEvent> {
        std::mem::take(&mut self.events)
    }

    /// `updateOpts`: the overlay key, and EE2's hide-on-blur.
    pub fn update_options(&mut self, overlay_key: &str, hide_overlay_on_blur: bool) {
        self.overlay_key = overlay_key.to_owned();
        self.hide_overlay_on_blur = hide_overlay_on_blur;
    }

    /// The host config's `updateOpts` call (`overlay.updateOpts(...)`). The window
    /// title it also takes is the OS layer's ([`crate::window`]).
    pub fn update_from(&mut self, config: &HostConfig) {
        self.update_options(&config.overlay_key, config.hide_overlay_on_blur);
    }

    /// `assertOverlayActive`: the overlay takes the focus — it is shown, takes the
    /// mouse, and the game loses its focus (which emits `focus-change`).
    pub fn assert_overlay_active(&mut self, window: &mut GameWindow, handle: &mut dyn OverlayHandle) {
        if self.is_interactable {
            return;
        }
        self.is_interactable = true;
        // `onOverlayActive` (EE2) then `OverlayController.activateOverlay`.
        handle.show();
        handle.set_click_through(false);
        if window.set_active(false).is_some() {
            self.on_game_active_change(false, window, handle);
        }
    }

    /// `assertGameActive`: the game takes the focus back and the overlay becomes
    /// click-through again.
    pub fn assert_game_active(&mut self, window: &mut GameWindow, handle: &mut dyn OverlayHandle) {
        if !self.is_interactable {
            return;
        }
        self.is_interactable = false;
        handle.set_click_through(true);
        handle.focus_game();
        if window.set_active(true).is_some() {
            self.on_game_active_change(true, window, handle);
        }
    }

    /// `toggleActiveState`: the overlay key.
    pub fn toggle_active_state(&mut self, window: &mut GameWindow, handle: &mut dyn OverlayHandle) {
        self.is_overlay_key_used = true;
        if self.is_interactable {
            self.assert_game_active(window, handle);
        } else {
            self.assert_overlay_active(window, handle);
        }
    }

    /// A window event from [`crate::window`]: the game's focus changes, the attach result and
    /// the game window going away all end up here.
    pub fn on_window_event(&mut self, event: &WindowEvent, window: &GameWindow, handle: &mut dyn OverlayHandle) {
        match event {
            WindowEvent::ActiveChanged { is_active } => self.on_game_active_change(*is_active, window, handle),
            WindowEvent::Attached { has_access } => self.on_attached(*has_access, window, handle),
            WindowEvent::Detached { was_active } => self.on_detached(*was_active, window, handle),
            WindowEvent::BoundsChanged { .. } => {}
        }
    }

    /// `electron-overlay-window`'s `detach`: the game's window is gone. The overlay keeps its
    /// state — Wraex stays in the tray — but a game that was in front reports losing it, and
    /// EE2's hide-on-blur has something to hide.
    fn on_detached(&mut self, was_active: bool, window: &GameWindow, handle: &mut dyn OverlayHandle) {
        if was_active {
            self.events.push(OverlayEvent::FocusChange {
                game: false,
                overlay: self.is_interactable,
                using_hotkey: self.is_overlay_key_used,
            });
            self.is_overlay_key_used = false;
        }
        self.sync_window_visibility(window, handle);
    }

    /// `handlePoeWindowActiveChange`: report who has the focus. A game that came
    /// back closes the overlay, without stealing the focus again.
    fn on_game_active_change(&mut self, is_active: bool, window: &GameWindow, handle: &mut dyn OverlayHandle) {
        if is_active && self.is_interactable {
            self.is_interactable = false;
        }
        self.events.push(OverlayEvent::FocusChange {
            game: is_active,
            overlay: self.is_interactable,
            using_hotkey: self.is_overlay_key_used,
        });
        self.is_overlay_key_used = false;
        self.sync_window_visibility(window, handle);
    }

    /// `handleOverlayAttached`: attached, or the game runs elevated and cannot be
    /// reached.
    fn on_attached(&mut self, has_access: Option<bool>, window: &GameWindow, handle: &mut dyn OverlayHandle) {
        if has_access == Some(false) {
            self.events.push(OverlayEvent::NoAccess);
        } else {
            self.events.push(OverlayEvent::Attached);
        }
        self.sync_window_visibility(window, handle);
    }

    /// `OVERLAY->MAIN::render-state` (EE2): the renderer has, or has not, anything
    /// to show. PoE1's renderer never reports it, and hiding on blur is EE2's
    /// feature, so this is ignored there.
    pub fn on_render_state(&mut self, should_show: bool, window: &GameWindow, handle: &mut dyn OverlayHandle) {
        if self.game != GameId::Poe2 {
            return;
        }
        self.should_show_overlay = should_show;
        self.sync_window_visibility(window, handle);
    }

    /// `syncWindowVisibility` (EE2): show the overlay while the overlay is
    /// interactable or the game is focused and there is something to show; hide it
    /// otherwise. Without `hideOverlayOnBlur` the window is left alone, which is
    /// PoE1's behaviour (it has no such setting).
    fn sync_window_visibility(&mut self, window: &GameWindow, handle: &mut dyn OverlayHandle) {
        // `hideOverlayOnBlur` is EE2's setting; PoE1's renderer has no visibility
        // to report and its host config has no such field, so the game's window is
        // left to `electron-overlay-window`/Rune there.
        if self.game != GameId::Poe2 || !self.hide_overlay_on_blur {
            return;
        }
        if self.is_interactable || (self.should_show_overlay && window.is_active()) {
            if !handle.is_visible() {
                handle.show();
            }
        } else if handle.is_visible() {
            handle.hide();
        }
    }

    /// `handleExtraCommands`: a key pressed in the overlay window. Escape and
    /// Ctrl+W give the game the focus back, and the overlay key toggles.
    /// `true` means the key is handled and the app should `preventDefault`;
    /// upstream defers the action to the next tick so the key event finishes
    /// first.
    pub fn on_overlay_input(
        &mut self,
        input: &OverlayInput,
        window: &mut GameWindow,
        handle: &mut dyn OverlayHandle,
    ) -> bool {
        match input.hotkey().as_str() {
            "Escape" | "Ctrl + W" => {
                self.assert_game_active(window, handle);
                true
            }
            hotkey if hotkey == self.overlay_key => {
                self.toggle_active_state(window, handle);
                true
            }
            _ => false,
        }
    }

    /// `OVERLAY->MAIN::track-area`: the widget the renderer wants followed.
    pub fn on_track_area(&mut self, area: TrackArea) {
        self.tracker.track(area, self.os);
    }

    /// `removeListeners`: forget the widget.
    pub fn stop_tracking_area(&mut self) {
        self.tracker.stop();
    }

    /// Whether a widget's area is being followed.
    pub fn is_tracking_area(&self) -> bool {
        self.tracker.tracked.is_some()
    }

    /// `handleMouseMove`: inside the widget's area the overlay takes the mouse;
    /// away from the press position the exclusive widget closes; leaving the area
    /// while interactable gives the game its focus back.
    pub fn on_mouse_move(
        &mut self,
        point: Point,
        ctrl: bool,
        alt: bool,
        window: &mut GameWindow,
        handle: &mut dyn OverlayHandle,
    ) {
        let Some(tracked) = self.tracker.tracked.clone() else {
            return;
        };
        // `modifier !== this.holdKey`: `undefined` only equals `undefined`, so a
        // widget without a hold key activates on hover like upstream, and an empty
        // hold key does not.
        let hold_key = tracked.hold_key.as_deref();
        let modifier = if ctrl {
            Some("Ctrl")
        } else if alt {
            Some("Alt")
        } else {
            None
        };

        if !self.is_interactable && modifier != hold_key {
            if point.distance_to(tracked.from) > tracked.close_threshold {
                self.events.push(OverlayEvent::HideExclusiveWidget);
                self.stop_tracking_area();
            }
        } else if point.is_inside(tracked.area) {
            self.assert_overlay_active(window, handle);
        } else if self.is_interactable {
            self.stop_tracking_area();
            self.assert_game_active(window, handle);
        }
    }

    /// `handleMouseDown`: a click on the widget's area activates the overlay.
    pub fn on_mouse_down(&mut self, point: Point, window: &mut GameWindow, handle: &mut dyn OverlayHandle) {
        let Some(tracked) = self.tracker.tracked.clone() else {
            return;
        };
        if point.is_inside(tracked.area) {
            self.stop_tracking_area();
            self.assert_overlay_active(window, handle);
        }
    }

    /// `OverlayVisibility`'s keydown: holding Alt alone hides the overlay.
    pub fn on_global_key_down(&mut self, event: &InputEvent, clock: &dyn Clock) {
        if event.alt && !event.shift && !event.ctrl && event.key == Some("Alt") {
            self.visibility.make_invisible(self.is_interactable, clock);
        } else {
            self.make_visible();
        }
    }

    /// `OverlayVisibility`'s keyup: releasing Alt brings the overlay back.
    pub fn on_global_key_up(&mut self, alt: bool) {
        if !alt {
            self.make_visible();
        }
    }

    /// `OverlayVisibility`'s mousemove: moving the mouse brings the overlay back.
    pub fn on_global_mouse_move(&mut self, alt: bool) {
        if !alt {
            self.make_visible();
        }
    }

    fn make_visible(&mut self) {
        if self.visibility.make_visible() == Some(true) {
            self.events.push(OverlayEvent::Visibility { is_visible: true });
        }
    }

    /// The pending hide's timer (see [`Self::visibility_deadline`]).
    pub fn poll(&mut self, clock: &dyn Clock) {
        let Some(hide_at) = self.visibility.hide_at else {
            return;
        };
        if clock.now() < hide_at {
            return;
        }
        self.visibility.hide_at = None;
        self.visibility.is_visible = false;
        self.events.push(OverlayEvent::Visibility { is_visible: false });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{FakeClock, FakeOverlay, OverlayCall};
    use crate::window::GameWindowEvents;

    const RECT: Bounds = Bounds { x: 100.0, y: 50.0, width: 1600.0, height: 1200.0 };

    fn fresh(game: GameId) -> (OverlayController, GameWindow, FakeOverlay, FakeClock) {
        (OverlayController::new(game, Os::Windows), GameWindow::new(), FakeOverlay::default(), FakeClock::default())
    }

    /// What the app's wiring does with an OS event: the window updates itself and
    /// the overlay reacts to what changed.
    fn forward(
        controller: &mut OverlayController,
        window: &mut GameWindow,
        handle: &mut dyn OverlayHandle,
        event: GameWindowEvents,
    ) {
        if let Some(event) = window.feed(event) {
            controller.on_window_event(&event, window, handle);
        }
    }

    /// A controller whose game already has the focus, with the activation's
    /// events drained.
    fn focused() -> (OverlayController, GameWindow, FakeOverlay, FakeClock) {
        let (mut controller, mut window, mut handle, clock) = fresh(GameId::Poe1);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        controller.take_events();
        (controller, window, handle, clock)
    }

    fn tracked(from: Point, hold_key: Option<&str>) -> TrackArea {
        TrackArea { hold_key: hold_key.map(str::to_owned), close_threshold: 2.5, from, area: RECT, dpr: 1.0 }
    }

    #[test]
    fn the_overlay_takes_the_focus_and_gives_it_back() {
        let (mut controller, mut window, mut handle, _clock) = focused();

        controller.assert_overlay_active(&mut window, &mut handle);
        assert!(controller.is_interactable());
        assert!(!window.is_active(), "the game lost its focus");
        assert_eq!(handle.calls(), vec![OverlayCall::Show, OverlayCall::ClickThrough(false)]);
        assert_eq!(
            controller.take_events(),
            vec![OverlayEvent::FocusChange { game: false, overlay: true, using_hotkey: false }]
        );

        controller.assert_game_active(&mut window, &mut handle);
        assert!(!controller.is_interactable());
        assert!(window.is_active());
        assert_eq!(handle.calls()[2..], [OverlayCall::ClickThrough(true), OverlayCall::FocusGame]);
        assert_eq!(
            controller.take_events(),
            vec![OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }]
        );

        // Already active is a no-op, the second time too.
        controller.assert_overlay_active(&mut window, &mut handle);
        let calls = handle.calls().len();
        controller.assert_overlay_active(&mut window, &mut handle);
        assert_eq!(handle.calls().len(), calls);
    }

    #[test]
    fn the_overlay_key_toggles_and_is_marked_in_the_focus_change() {
        let (mut controller, mut window, mut handle, _clock) = focused();

        controller.toggle_active_state(&mut window, &mut handle);
        assert!(controller.is_interactable());
        assert_eq!(
            controller.take_events(),
            vec![OverlayEvent::FocusChange { game: false, overlay: true, using_hotkey: true }]
        );

        controller.toggle_active_state(&mut window, &mut handle);
        assert!(!controller.is_interactable());
        assert_eq!(
            controller.take_events(),
            vec![OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: true }],
            "`usingHotkey` is reset after each change"
        );
    }

    #[test]
    fn a_game_that_comes_back_closes_the_overlay() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        controller.assert_overlay_active(&mut window, &mut handle);
        controller.take_events();

        // The user clicked the game: the OS reports focus, not the overlay.
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        assert!(!controller.is_interactable(), "the overlay gave way");
        assert_eq!(
            controller.take_events(),
            vec![OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }]
        );
    }

    #[test]
    fn a_closed_game_window_reports_the_focus_it_lost() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe1);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Attach { has_access: Some(true) });
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        controller.take_events();

        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Detach);
        assert!(!window.is_tracking());
        assert!(!window.is_active());
        assert_eq!(
            controller.take_events(),
            vec![OverlayEvent::FocusChange { game: false, overlay: false, using_hotkey: false }]
        );

        // The OS layer reports a destruction once; the overlay keeps its state either way.
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Detach);
        assert!(controller.take_events().is_empty());
    }

    #[test]
    fn focus_changes_are_reported_once() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe1);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        assert_eq!(
            controller.take_events(),
            vec![OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }]
        );
        // `set isActive` emits only on a change, so the second report is silent.
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        assert!(controller.take_events().is_empty());

        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Attach { has_access: Some(true) });
        assert_eq!(controller.take_events(), vec![OverlayEvent::Attached]);
    }

    #[test]
    fn attached_without_access_is_the_administrator_error() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe1);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Attach { has_access: Some(false) });
        assert_eq!(controller.take_events(), vec![OverlayEvent::NoAccess]);
    }

    #[test]
    fn ee2_hides_the_overlay_while_the_game_is_unfocused() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe2);
        controller.update_options("Shift + Space", true);

        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        assert!(handle.is_visible());
        controller.take_events();

        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Blur);
        assert!(!handle.is_visible(), "hidden with the game");
        assert_eq!(handle.calls(), vec![OverlayCall::Show, OverlayCall::Hide]);

        // An interactable overlay stays visible even without the game.
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        controller.assert_overlay_active(&mut window, &mut handle);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Blur);
        assert!(handle.is_visible());
    }

    #[test]
    fn poe1_never_manages_the_overlay_windows_visibility() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe1);
        // PoE1 has no `hideOverlayOnBlur` setting; even if the flag were set, the
        // feature is EE2's.
        controller.update_options("Shift + Space", true);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Blur);
        assert!(handle.calls().is_empty(), "the window is never shown or hidden");

        // The interactable path is the exception: a widget has to be visible.
        controller.assert_overlay_active(&mut window, &mut handle);
        assert_eq!(handle.calls(), vec![OverlayCall::Show, OverlayCall::ClickThrough(false)]);
    }

    #[test]
    fn ee2_hides_the_window_when_its_renderer_has_nothing_to_show() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe2);
        controller.update_options("Shift + Space", true);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        assert!(handle.is_visible());

        controller.on_render_state(false, &window, &mut handle);
        assert!(!handle.is_visible());
        controller.on_render_state(true, &window, &mut handle);
        assert!(handle.is_visible());
    }

    #[test]
    fn poe1_ignores_the_render_state_event() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe1);
        controller.update_options("Shift + Space", true);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        let calls = handle.calls().len();
        // PoE1's renderer has no `render-state`; nothing may be hidden on it.
        controller.on_render_state(false, &window, &mut handle);
        assert_eq!(handle.calls().len(), calls);
    }

    #[test]
    fn escape_and_ctrl_w_inside_the_overlay_give_the_game_its_focus() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        controller.assert_overlay_active(&mut window, &mut handle);
        let escape = OverlayInput { code: "Escape".into(), ctrl: false, shift: false, alt: false };
        assert!(controller.on_overlay_input(&escape, &mut window, &mut handle));
        assert!(window.is_active());
        assert!(!controller.is_interactable());

        controller.assert_overlay_active(&mut window, &mut handle);
        let ctrl_w = OverlayInput { code: "KeyW".into(), ctrl: true, shift: false, alt: false };
        assert!(controller.on_overlay_input(&ctrl_w, &mut window, &mut handle));
        assert!(window.is_active());
    }

    #[test]
    fn the_overlay_key_toggles_from_inside_the_overlay() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        controller.update_options("Shift + Space", false);
        let overlay_key = OverlayInput { code: "Space".into(), ctrl: false, shift: true, alt: false };
        assert_eq!(overlay_key.hotkey(), "Shift + Space");
        assert!(controller.on_overlay_input(&overlay_key, &mut window, &mut handle));
        assert!(controller.is_interactable());

        let other = OverlayInput { code: "KeyX".into(), ctrl: false, shift: false, alt: false };
        assert!(!controller.on_overlay_input(&other, &mut window, &mut handle));
        assert!(controller.is_interactable(), "an unhandled key changes nothing");
    }

    #[test]
    fn overlay_input_hotkey_strings_drop_the_key_and_digit_prefixes() {
        let input = |code: &str, ctrl, shift, alt| OverlayInput { code: code.into(), ctrl, shift, alt };
        assert_eq!(input("KeyW", true, false, false).hotkey(), "Ctrl + W");
        assert_eq!(input("Digit1", false, false, true).hotkey(), "Alt + 1");
        assert_eq!(input("Space", false, true, false).hotkey(), "Shift + Space");
        assert_eq!(input("Escape", false, false, false).hotkey(), "Escape");
        // Upstream's `handleExtraCommands` has no Ctrl + Shift + Alt case: it
        // matches Shift + Alt first.
        assert_eq!(input("KeyK", true, true, true).hotkey(), "Shift + Alt + K");
    }

    #[test]
    fn moving_away_from_the_item_closes_the_exclusive_widget() {
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe1);
        // A hold key the user is not holding: moving away from the item closes
        // the widget.
        controller.on_track_area(tracked(Point { x: 500.0, y: 500.0 }, Some("Ctrl")));
        assert!(controller.is_tracking_area());

        controller.on_mouse_move(Point { x: 510.0, y: 510.0 }, false, false, &mut window, &mut handle);
        assert_eq!(controller.take_events(), vec![OverlayEvent::HideExclusiveWidget]);
        assert!(!controller.is_tracking_area(), "the listeners are removed");
    }

    #[test]
    fn moving_into_the_widget_area_activates_the_overlay() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        // The item is where the cursor already is, so the distance check passes.
        controller.on_track_area(tracked(Point { x: 200.0, y: 200.0 }, None));
        controller.on_mouse_move(Point { x: 200.0, y: 200.0 }, false, false, &mut window, &mut handle);
        assert!(controller.is_interactable());
        assert!(controller.is_tracking_area());
    }

    #[test]
    fn clicking_the_widget_area_activates_the_overlay_and_stops_tracking() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        controller.on_track_area(tracked(Point { x: 2000.0, y: 2000.0 }, None));
        controller.on_mouse_down(Point { x: 500.0, y: 500.0 }, &mut window, &mut handle);
        assert!(controller.is_interactable());
        assert!(!controller.is_tracking_area());
    }

    #[test]
    fn leaving_the_widget_area_gives_the_game_its_focus_back() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        controller.on_track_area(tracked(Point { x: 500.0, y: 500.0 }, None));
        controller.on_mouse_down(Point { x: 500.0, y: 500.0 }, &mut window, &mut handle);
        assert!(controller.is_interactable());

        controller.on_track_area(tracked(Point { x: 2000.0, y: 2000.0 }, None));
        // Far away and outside the widget's rectangle.
        controller.on_mouse_move(Point { x: 2000.0, y: 2000.0 }, false, false, &mut window, &mut handle);
        assert!(!controller.is_interactable());
        assert!(window.is_active());
        assert!(!controller.is_tracking_area());
    }

    #[test]
    fn holding_the_configured_key_keeps_the_widget_open() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        controller.on_track_area(tracked(Point { x: 500.0, y: 500.0 }, Some("Ctrl")));
        // Away from the item and outside the widget, but Ctrl is held: the widget
        // stays, and nothing steals the focus back.
        controller.on_mouse_move(Point { x: 2000.0, y: 2000.0 }, true, false, &mut window, &mut handle);
        assert!(controller.is_tracking_area());
        assert!(controller.take_events().is_empty());
        assert!(window.is_active());
    }

    #[test]
    fn the_tracked_area_is_scaled_to_screen_pixels_on_windows() {
        let (mut controller, mut window, mut handle, _clock) = focused();
        controller.on_track_area(TrackArea {
            hold_key: None,
            close_threshold: 2.5,
            from: Point { x: 300.0, y: 200.0 },
            area: Bounds { x: 100.0, y: 50.0, width: 100.0, height: 100.0 },
            dpr: 2.0,
        });
        // 300,200 is outside the CSS rectangle but inside the doubled one.
        controller.on_mouse_move(Point { x: 300.0, y: 200.0 }, false, false, &mut window, &mut handle);
        assert!(controller.is_interactable());
    }

    #[test]
    fn holding_alt_hides_the_overlay_after_a_delay() {
        let (mut controller, _window, _handle, clock) = fresh(GameId::Poe1);
        let alt = InputEvent { key: Some("Alt"), ctrl: false, shift: false, alt: true };
        controller.on_global_key_down(&alt, &clock);
        assert_eq!(controller.visibility_deadline(), Some(VISIBILITY_HIDE_DELAY));
        assert!(controller.is_visible(), "not yet");

        clock.advance(VISIBILITY_HIDE_DELAY - 1);
        controller.poll(&clock);
        assert!(controller.is_visible());
        clock.advance(1);
        controller.poll(&clock);
        assert!(!controller.is_visible());
        assert_eq!(controller.take_events(), vec![OverlayEvent::Visibility { is_visible: false }]);

        controller.on_global_key_up(false);
        assert!(controller.is_visible());
        assert_eq!(controller.take_events(), vec![OverlayEvent::Visibility { is_visible: true }]);
    }

    #[test]
    fn an_interactable_overlay_hides_sooner_and_a_move_cancels_it() {
        let (mut controller, mut window, mut handle, clock) = focused();
        controller.assert_overlay_active(&mut window, &mut handle);
        controller.take_events();

        let alt = InputEvent { key: Some("Alt"), ctrl: false, shift: false, alt: true };
        controller.on_global_key_down(&alt, &clock);
        assert_eq!(controller.visibility_deadline(), Some(VISIBILITY_HIDE_DELAY_INTERACTABLE));

        // Moving the mouse cancels the pending hide; nothing is emitted, the
        // overlay never went away.
        controller.on_global_mouse_move(false);
        assert_eq!(controller.visibility_deadline(), None);
        clock.advance(VISIBILITY_HIDE_DELAY_INTERACTABLE);
        controller.poll(&clock);
        assert!(controller.is_visible());
        assert!(controller.take_events().is_empty());
    }

    #[test]
    fn alt_with_another_key_is_not_a_peek() {
        let (mut controller, _window, _handle, clock) = fresh(GameId::Poe1);
        let alt_a = InputEvent { key: Some("A"), ctrl: false, shift: false, alt: true };
        controller.on_global_key_down(&alt_a, &clock);
        assert_eq!(controller.visibility_deadline(), None);
        assert!(controller.is_visible());
    }

    #[test]
    fn the_host_config_sets_the_overlay_key_and_hide_on_blur() {
        let config = |hide_on_blur: bool| -> HostConfig {
            serde_json::from_value(serde_json::json!({
                "shortcuts": [],
                "restoreClipboard": false,
                "stashScroll": true,
                "logKeys": false,
                "overlayKey": "Alt + Q",
                "windowTitle": "Path of Exile 2",
                "language": "en",
                "hideOverlayOnBlur": hide_on_blur
            }))
            .unwrap()
        };

        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe2);
        controller.update_from(&config(true));
        assert_eq!(controller.overlay_key(), "Alt + Q");
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Blur);
        assert!(!handle.is_visible(), "the config turned hide-on-blur on");

        // PoE1's host config has no `hideOverlayOnBlur`: the field defaults off.
        let poe1: HostConfig = serde_json::from_value(serde_json::json!({
            "shortcuts": [],
            "restoreClipboard": false,
            "stashScroll": true,
            "logKeys": false,
            "overlayKey": "Shift + Space",
            "windowTitle": "Path of Exile",
            "language": "en"
        }))
        .unwrap();
        let (mut controller, mut window, mut handle, _clock) = fresh(GameId::Poe1);
        controller.update_from(&poe1);
        assert_eq!(controller.overlay_key(), "Shift + Space");
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Focus);
        forward(&mut controller, &mut window, &mut handle, GameWindowEvents::Blur);
        assert!(handle.calls().is_empty());
    }
}
