//! The shell's pure parts: which game a run picks, the window sizes, the overlay's mouse
//! wiring with fakes, and the fact that a scene never reaches the live path.
//!
//! The last tests drive the real path end to end — `App::poll_with` → `Live::poll`, with a Rune
//! handle and a `Panel` but no window — because every overlay bug that reached a Windows run
//! came from the shell asking `App` about a live state it had taken out of `App::live`.

#![expect(clippy::disallowed_macros, reason = "expected JSON written as literals")]

use super::*;
use crate::settings::Page;
use wraex_platform::overlay::{Modifiers, OverlayEvent, OverlayHandle, OverlayInput};
use wraex_platform::shortcuts::InputEvent;

/// One [`OverlayHandle`] call, recorded; the same shape the platform crate's tests use, which
/// is private to that crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlayCall {
    Show,
    ClickThrough(bool),
    FocusGame,
}

/// The overlay window as [`OverlayController`] drives it, with no Rune behind it.
#[derive(Debug, Default)]
struct FakeOverlay {
    visible: bool,
    click_through: Option<bool>,
    calls: Vec<OverlayCall>,
}

impl OverlayHandle for FakeOverlay {
    fn show(&mut self) {
        if !self.visible {
            self.visible = true;
            self.calls.push(OverlayCall::Show);
        }
    }

    fn hide(&mut self) {
        self.visible = false;
    }

    fn set_click_through(&mut self, click_through: bool) {
        self.click_through = Some(click_through);
        self.calls.push(OverlayCall::ClickThrough(click_through));
    }

    fn focus_game(&mut self) {
        self.calls.push(OverlayCall::FocusGame);
    }

    fn is_visible(&self) -> bool {
        self.visible
    }
}

/// The game's client area the tests place the overlay over, in physical pixels.
const BOUNDS: Bounds = Bounds { x: 0.0, y: 0.0, width: 1600.0, height: 1200.0 };

/// The screen the placement helpers read: [`BOUNDS`] at a 100 % DPI scale.
const SCREEN: Screen = Screen { bounds: BOUNDS, scale: 1.0 };

/// The shell's overlay side without Rune: the game window, the overlay state machine and the
/// fake window, driven in the order `Live::press` and `Live::poll` drive them.
struct Harness {
    overlay: OverlayController,
    window: GameWindow,
    handle: FakeOverlay,
}

impl Harness {
    /// A game that is in front, with the activation's events drained.
    fn playing() -> Self {
        let mut harness = Self {
            overlay: OverlayController::new(GameId::Poe1, wraex_core::game_files::Os::Windows),
            window: GameWindow::new(),
            handle: FakeOverlay::default(),
        };
        harness.feed(wraex_platform::window::GameWindowEvents::Focus);
        harness.overlay.take_events();
        harness
    }

    fn feed(&mut self, event: wraex_platform::window::GameWindowEvents) {
        if let Some(change) = self.window.feed(event) {
            self.overlay.on_window_event(&change, &self.window, &mut self.handle);
        }
    }

    /// A normal price-check hotkey: `Live::press`'s `showInactive` + `Live::sync_window`'s
    /// click-through and `track-area`.
    fn open_price_check(&mut self, cursor: Point, scale: f64) {
        // `showInactive()` and the click-through the overlay window is created with. No
        // `assert_overlay_active`: the game keeps the focus.
        self.handle.show();
        self.handle.set_click_through(true);
        let size = overlay_widget_size(GameId::Poe1, &Shown::PriceCheck, Screen { bounds: BOUNDS, scale })
            .expect("the price check has a size");
        let offset = widget_offset(Some(Placement::Cursor(cursor)), size, Screen { bounds: BOUNDS, scale });
        let area = track_area(offset, size, Screen { bounds: BOUNDS, scale }, cursor, price_check_hold_key(&config()));
        self.overlay.on_track_area(area);
    }

    /// One of `Live::poll_mouse`'s events.
    fn mouse(&mut self, event: MouseEvent) {
        match event {
            MouseEvent::Move { point, ctrl, alt } => {
                self.overlay.on_mouse_move(point, Modifiers { ctrl, alt }, &mut self.window, &mut self.handle);
            }
            MouseEvent::Down { point } => self.overlay.on_mouse_down(point, &mut self.window, &mut self.handle),
            MouseEvent::Wheel { .. } => {}
        }
    }

    /// The window's click-through after `Live::sync_window`'s reconciliation.
    fn reconcile_click_through(&mut self) {
        self.handle.set_click_through(!self.overlay.is_interactable());
    }
}

/// A config with upstream's price-check defaults (`hotkeyHold: Ctrl`).
fn config() -> serde_json::Value {
    crate::settings::default_config(GameId::Poe1)
}

/// The app the shell leaves for an overlay widget: `Overlay::live`'s state (no scene), with
/// nothing open yet. A unit test has no Rune window, so the overlay's `active` state is passed
/// to [`App::shown_in`] by hand.
fn overlay_app() -> crate::App {
    let mut app = crate::App::replay("currency-divine-orb");
    app.close_windows();
    app.widgets = Some(widgets::Overlay::live(GameId::Poe1, &config()));
    app
}

/// `OverlayWindow.vue`'s `visibilityState`: while the overlay is active every widget whose
/// `wmWants` is `show` is drawn, and the widget menu always is. The first Windows run's
/// overlay-key bug: `shown` said `None`, so `sync_window` hid the window the moment the overlay
/// key was pressed and nothing appeared.
#[test]
fn the_overlay_key_shows_the_widget_menu() {
    let app = overlay_app();
    assert_eq!(app.shown_in(false), Shown::None, "the menu carries `invisible-on-blur`");

    let shown = app.shown_in(true);
    assert_eq!(shown, Shown::Widget(widgets::Kind::Menu.wm_type().to_owned()));
    assert!(is_overlay(&shown), "the menu is drawn in the overlay window");
    assert!(
        !hides_window(&shown, Visibility { following_game: true, in_front: true }),
        "the window stays on screen over the game"
    );
    assert_eq!(window_size(GameId::Poe1, &shown), Some(widgets::scene_size(GameId::Poe1, widgets::Kind::Menu)));
}

/// The config's `wmId` for a `wmType` (the first instance, for the two stash searches).
fn widget_id(app: &crate::App, wm_type: &str) -> u32 {
    app.widgets
        .as_ref()
        .expect("the overlay")
        .widgets
        .iter()
        .find(|widget| widget.wm_type == wm_type)
        .unwrap_or_else(|| panic!("the config has no {wm_type} widget"))
        .wm_id
}

/// The menu's card offset, from the config's `anchor: { pos: 'tl', x: 5, y: 5 }`.
fn menu_offset() -> (f32, f32) {
    anchored_offset("tl", (5.0, 5.0), widgets::scene_size(GameId::Poe1, widgets::Kind::Menu), SCREEN)
}

/// `OverlayWindow.vue` draws every widget whose `wmWants` is `show`, each at its own anchor
/// (`Widget.vue`). The second Windows-run bug: clicking "Map rolling" in the menu replaced the
/// menu with the stash search, so the settings cog went with it; both must be on screen at once.
#[test]
fn the_overlay_draws_every_shown_widget_at_its_own_anchor() {
    let mut app = overlay_app();
    let menu = widget_id(&app, widgets::menu::WM_TYPE);
    let cards = app.cards_for(true, SCREEN, None);
    assert_eq!(cards.len(), 1, "the config's other widgets are hidden");
    assert_eq!(cards[0].shown, Shown::Widget(widgets::menu::WM_TYPE.to_owned()));
    assert_eq!(cards[0].wm_id, Some(menu));
    assert_eq!(cards[0].offset, menu_offset());
    assert_eq!(cards[0].size(), None, "a `Widget.vue` card is content-sized");

    // "Map rolling": the menu's button (or `wm.show`) makes the stash search visible; the menu
    // stays. `wmZorder` puts the numbered menu card last, as CSS paints a positive `z-index`
    // above the `auto` ones.
    let stash = widget_id(&app, widgets::Kind::StashSearch.wm_type());
    app.widgets.as_mut().expect("the overlay").show(stash);
    let cards = app.cards_for(true, SCREEN, None);
    assert_eq!(cards.len(), 2, "the menu and the stash search");
    assert_eq!(cards[0].wm_id, Some(stash), "the `auto` card paints first");
    assert_eq!(cards[1].wm_id, Some(menu), "the menu paints above it");
    let size = widgets::scene_size(GameId::Poe1, widgets::Kind::StashSearch);
    assert_eq!(cards[0].offset, anchored_offset("tl", (35.0, 46.0), size, SCREEN));
    assert_eq!(cards[1].offset, menu_offset(), "the menu keeps its own anchor");

    // The window's `shown` is still the menu (the focused widget), so the native window and the
    // debug log keep naming the overlay's card.
    assert_eq!(app.shown_in(true), Shown::Widget(widgets::menu::WM_TYPE.to_owned()));
}

/// Settings is `wmZorder: 'exclusive'` (`SettingsWindow.vue`): it is drawn alone, and its root
/// covers the overlay (`position: absolute; inset: 0`). Save or Cancel closes it and the menu is
/// back at *its own* anchor — the first Windows-run bug was the menu returning in the middle of
/// the screen, inheriting the placement the settings window had claimed.
#[test]
fn save_returns_to_the_menu_at_its_own_anchor() {
    let mut app = overlay_app();
    app.settings = Some(crate::settings::Settings::scene(GameId::Poe1, Page::Hotkeys));
    // `App::open_settings` asks for a centred window; over the game the settings card covers the
    // overlay whatever the placement says.
    let cards = app.cards_for(true, SCREEN, Some(Placement::Centred));
    assert_eq!(cards.len(), 1, "the exclusive settings window hides the menu");
    assert_eq!(cards[0].shown, Shown::Settings);
    assert_eq!(cards[0].offset, (0.0, 0.0));
    assert_eq!(cards[0].width, Some(f64_to_f32(BOUNDS.width)));
    assert_eq!(cards[0].height, Some(f64_to_f32(BOUNDS.height)));

    // Save: the working copy is written and the window closes, as `Live::poll` runs after it.
    app.settings.as_mut().expect("the settings window").save().expect("the scene's temp store");
    app.hide_settings_widget();
    let cards = app.cards_for(true, SCREEN, Some(Placement::Centred));
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].shown, Shown::Widget(widgets::menu::WM_TYPE.to_owned()));
    assert_eq!(cards[0].offset, menu_offset(), "the menu is at 5 %/5 %, not where the settings was");

    // Cancel is the same close (`Settings::cancel`), and the same menu.
    app.settings = Some(crate::settings::Settings::scene(GameId::Poe1, Page::General));
    app.settings.as_mut().expect("the settings window").cancel();
    app.hide_settings_widget();
    let cards = app.cards_for(true, SCREEN, Some(Placement::Centred));
    assert_eq!(cards[0].shown, Shown::Widget(widgets::menu::WM_TYPE.to_owned()));
    assert_eq!(cards[0].offset, menu_offset());
}

/// After the overlay key's on state with a settings window that never came: the config's
/// settings widget is `show` (a `wm.show` that did not open the window) and must not draw a
/// second card, or an overlay of the settings widget alone.
#[test]
fn the_settings_widget_card_needs_the_settings_window() {
    let mut app = overlay_app();
    let settings = widget_id(&app, widgets::SETTINGS);
    app.widgets.as_mut().expect("the overlay").show(settings);
    let cards = app.cards_for(true, SCREEN, None);
    assert!(
        cards.iter().all(|card| card.shown != Shown::Settings && card.wm_id != Some(settings)),
        "the settings widget is drawn by the settings window"
    );
    assert_eq!(cards.len(), 1, "the menu is still the only card");
}

/// The price check is `wmZorder: 'exclusive'` with `hide-on-blur`: while it is open the menu and
/// every other widget are gone, and it fills the overlay's height at the cursor.
#[test]
fn the_price_check_draws_alone() {
    let mut app = overlay_app();
    let stash = widget_id(&app, widgets::Kind::StashSearch.wm_type());
    app.widgets.as_mut().expect("the overlay").show(stash);
    app.price_check.open = true;
    let cursor = Point { x: 400.0, y: 300.0 };
    let cards = app.cards_for(true, SCREEN, Some(Placement::Cursor(cursor)));
    assert_eq!(cards.len(), 1, "an exclusive widget hides the rest");
    assert_eq!(cards[0].shown, Shown::PriceCheck);
    let size = (crate::poe1::price_check::WIDTH, f64_to_f32(BOUNDS.height));
    assert_eq!(cards[0].size(), Some(size), "`#price-window` is `h-full`");
    assert_eq!(cards[0].offset, widget_offset(Some(Placement::Cursor(cursor)), size, SCREEN));
    assert_eq!(app.shown_in(true), Shown::PriceCheck);
}

/// `visibilityState`'s `invisible-on-blur`: the menu goes with the overlay, while a widget whose
/// flag was cleared (a running stopwatch) stays drawn over the game.
#[test]
fn hidden_widgets_are_not_drawn() {
    let mut app = overlay_app();
    assert!(app.cards_for(false, SCREEN, None).is_empty(), "the menu is `invisible-on-blur`");

    let timer = app.widgets.as_mut().expect("the overlay").create(widgets::Kind::Stopwatch.wm_type());
    app.widgets.as_mut().expect("the overlay").timer_start(timer, 0.0);
    let cards = app.cards_for(false, SCREEN, None);
    assert_eq!(cards.len(), 1, "the running stopwatch is the only card off-overlay");
    assert_eq!(cards[0].wm_id, Some(timer));
    // The timer's own anchor is the centre (`cc`), which its card is shifted by.
    let size = widgets::scene_size(GameId::Poe1, widgets::Kind::Stopwatch);
    assert_eq!(cards[0].offset, anchored_offset("cc", (50.0, 50.0), size, SCREEN));
    assert_eq!(app.shown_in(false), Shown::Widget(widgets::Kind::Stopwatch.wm_type().to_owned()));
}

/// The settings window as the overlay draws it: `SettingsWindow.vue` is a widget whose root is
/// `position: absolute; inset: 0`, with its 50 × 38 rem `.window` centred horizontally at the
/// top. Its box takes the clicks — below the window too — so a click anywhere inside it never
/// reaches the overlay's background layer (the bug where the settings closed on its first click).
#[test]
#[expect(clippy::float_cmp_const, reason = "the drawn rect is exactly the window size it was laid out at")]
fn the_settings_window_covers_the_overlay_and_takes_its_clicks() {
    let mut app = overlay_app();
    app.settings = Some(crate::settings::Settings::scene(GameId::Poe1, Page::Hotkeys));
    let size = (f64_to_f32(BOUNDS.width), f64_to_f32(BOUNDS.height));
    let text = rune::text::TextSystem::new(crate::theme::FONTS, crate::theme::FAMILIES);
    let mut runtime = rune::Runtime::with_text(app, game_overlay, size, rune::Env::new(true, 1.0), text);
    runtime.frame(0.0);

    assert_eq!(
        rect_of(&runtime, "settings-window"),
        [400.0, 0.0, crate::settings::WIDTH, crate::settings::HEIGHT],
        "centred horizontally, at the top of the game"
    );
    assert!(runtime.find("settings-menu-hotkeys").is_some(), "the pages are drawn");
    assert!(runtime.find("titlebar-close").is_some(), "the title bar's close is there");
    // Below the window, still inside the settings widget's root: the widget's own box.
    let picked = runtime.element_at(100.0, 1000.0).and_then(|picked| picked.key().map(str::to_owned));
    assert_ne!(picked.as_deref(), Some("overlay-background"), "a click inside the settings window is the widget's");
    assert!(
        matches!(picked.as_deref(), Some("settings" | "card-settings")),
        "the settings card's own box takes it: {picked:?}"
    );
}

/// The Windows run's whole sequence, as the shell's decisions see it: the overlay key draws the
/// menu at its anchor; "Map rolling" adds the stash search and keeps the menu; the cog's settings
/// window is alone; Save brings the menu back at *its* anchor, beside the stash search; the
/// background click gives the game its focus back and the overlay draws nothing.
#[test]
fn the_windows_runs_sequence_keeps_every_card_in_place() {
    let mut app = overlay_app();
    let menu = widget_id(&app, widgets::menu::WM_TYPE);
    let stash = widget_id(&app, widgets::Kind::StashSearch.wm_type());
    let settings = widget_id(&app, widgets::SETTINGS);

    // The overlay key: the menu, at 5 %/5 %.
    let cards = app.cards_for(true, SCREEN, None);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].wm_id, Some(menu));
    assert_eq!(cards[0].offset, menu_offset());

    // "Map rolling": the menu's button is `wm.toggle`, which shows the stash search.
    app.widgets.as_mut().expect("the overlay").toggle(stash);
    let cards = app.cards_for(true, SCREEN, None);
    assert_eq!(cards.len(), 2, "both cards: the image strip must not hide the menu");
    assert!(cards.iter().any(|card| card.wm_id == Some(menu) && card.offset == menu_offset()));
    assert!(cards.iter().any(|card| card.wm_id == Some(stash)));

    // The cog: `wm.show(settings)` and the settings window opens. It is exclusive, so it is the
    // only card, covering the overlay.
    app.widgets.as_mut().expect("the overlay").toggle(settings);
    app.settings = Some(crate::settings::Settings::scene(GameId::Poe1, Page::Hotkeys));
    let cards = app.cards_for(true, SCREEN, Some(Placement::Centred));
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].shown, Shown::Settings);

    // Save: the settings widget goes back to `hide`, and the menu is where it belongs.
    app.settings.as_mut().expect("the settings window").save().expect("the scene's temp store");
    app.hide_settings_widget();
    let cards = app.cards_for(true, SCREEN, Some(Placement::Centred));
    let menu_card = cards.iter().find(|card| card.wm_id == Some(menu)).expect("the menu is back");
    assert_eq!(menu_card.offset, menu_offset(), "the menu, not the settings window's placement");
    assert!(cards.iter().any(|card| card.wm_id == Some(stash)), "the stash search stayed up");

    // The background click: the game gets the focus back, and the overlay draws nothing.
    apply_overlay_event(OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }, &mut app);
    assert!(app.cards_for(false, SCREEN, None).is_empty(), "the menu and the stash search are `invisible-on-blur`");
    assert_eq!(app.shown_in(false), Shown::None);
}

/// The overlay as [`crate::view`] draws it over a game: every visible widget at its own anchor,
/// in one window (`OverlayWindow.vue`'s `visibilityState`). The live shell places the same cards
/// (`App::cards_for`); this renders them from the app's own state, as a `--snap` of a scene does.
fn game_overlay(app: &App, cx: &mut rune::Cx<App>) -> El {
    let cards = app.cards_for(true, SCREEN, None);
    crate::shell::overlay_root(cards, true, |card| crate::card_at(app, cx, card))
}

/// The drawn rect of the element keyed `key`, in points.
fn rect_of(runtime: &rune::Runtime<App>, key: &str) -> [f32; 4] {
    runtime
        .elements()
        .iter()
        .find(|element| element.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("no element keyed `{key}`"))
        .rect
}

/// The second Windows-run bug, end to end: "Map rolling" from the menu must leave the menu on
/// screen (its cog included) *and* draw the stash search, each at its own anchor, in one overlay
/// window.
#[test]
fn the_menu_and_a_stash_search_draw_together() {
    let mut app = overlay_app();
    let menu = widget_id(&app, widgets::menu::WM_TYPE);
    let settings = widget_id(&app, widgets::SETTINGS);
    let stash = widget_id(&app, widgets::Kind::StashSearch.wm_type());
    app.widgets.as_mut().expect("the overlay").toggle(stash);

    let size = (f64_to_f32(BOUNDS.width), f64_to_f32(BOUNDS.height));
    let text = rune::text::TextSystem::new(crate::theme::FONTS, crate::theme::FAMILIES);
    let mut runtime = rune::Runtime::with_text(app, game_overlay, size, rune::Env::new(true, 1.0), text);
    runtime.frame(0.0);

    assert_eq!(rect_of(&runtime, "overlay-background"), [0.0, 0.0, size.0, size.1]);
    // The menu at 5 %/5 % of 1600 × 1200, and the stash search at 35 %/46 %.
    let menu_card = rect_of(&runtime, &format!("card-{menu}"));
    assert_eq!([menu_card[0], menu_card[1]], [80.0, 60.0]);
    let stash_card = rect_of(&runtime, &format!("card-{stash}"));
    assert_eq!([stash_card[0], stash_card[1]], [560.0, 552.0]);
    // Both cards are drawn, and the menu still offers its cog: the widget menu did not go.
    assert!(rect_of(&runtime, "widget-menu")[2] > 0.0, "the menu card is drawn");
    assert!(rect_of(&runtime, &format!("widget-{stash}"))[2] > 0.0, "the stash search card is drawn");
    assert!(runtime.find(&format!("widget-menu-entry-{settings}")).is_some(), "the cog is still there");
    // The window is one overlay: the two cards do not overlap, and neither is cut off.
    let menu_rect = rect_of(&runtime, "widget-menu");
    let stash_rect = rect_of(&runtime, &format!("widget-{stash}"));
    assert!(menu_rect[0] + menu_rect[2] < stash_rect[0], "the menu and the stash search are side by side");
    assert!(stash_rect[1] + stash_rect[3] <= size.1);
}

/// The game taking the focus back closes what `invisible-on-blur` hid: the menu is not drawn
/// any more, so the shell hides the window, and the next activation brings it back.
#[test]
fn the_game_taking_the_focus_hides_the_widget_menu() {
    let mut app = overlay_app();
    assert_eq!(app.shown_in(true), Shown::Widget(widgets::Kind::Menu.wm_type().to_owned()));

    apply_overlay_event(OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }, &mut app);

    let shown = app.shown_in(false);
    assert_eq!(shown, Shown::None);
    assert!(
        hides_window(&shown, Visibility { following_game: true, in_front: true }),
        "nothing to draw, so the window hides even with the game in front"
    );
    assert_eq!(app.shown_in(true), Shown::Widget(widgets::Kind::Menu.wm_type().to_owned()));
}

/// The overlay key with a price check open: the card is `wmZorder: 'exclusive'` upstream and
/// wins over the menu both ways — it stays when the overlay comes on, and its `hide-on-blur`
/// flag closes it when the key gives the game the focus back.
#[test]
fn the_overlay_key_with_a_price_check_open_keeps_the_card() {
    let mut app = overlay_app();
    app.price_check.open = true;
    assert_eq!(app.shown_in(true), Shown::PriceCheck, "an exclusive widget wins over the menu");

    apply_overlay_event(OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }, &mut app);
    assert!(!app.price_check.open, "`hide-on-blur`");
    assert_eq!(app.shown_in(false), Shown::None);
}

/// Settings is `invisible-on-blur` upstream: the game taking the focus closes it, the tray or
/// the menu only having asked for it.
#[test]
fn the_game_taking_the_focus_closes_settings() {
    let mut app = crate::App::settings_scene(GameId::Poe1, Page::Hotkeys);
    // The scene opens over a price check; nothing is under the settings window in a live run,
    // because `wm.show` on an exclusive widget hides the other one.
    app.price_check.open = false;
    assert_eq!(app.shown(), Shown::Settings);

    app.hide_widgets_on_blur();
    assert!(!app.settings.as_ref().is_some_and(|settings| settings.open));
    assert_eq!(app.shown(), Shown::None);
}

/// `WidgetSettings.vue`'s close hides its widget, so the menu lists the cog again.
#[test]
fn closing_settings_lists_it_in_the_menu_again() {
    let mut app = overlay_app();
    let id = app
        .widgets
        .as_ref()
        .and_then(|overlay| overlay.widgets.iter().find(|widget| widget.wm_type == widgets::SETTINGS))
        .expect("the config's settings widget")
        .wm_id;
    app.widgets.as_mut().expect("the overlay").show(id);
    app.widgets.as_mut().expect("the overlay").take_actions();
    assert_eq!(app.widgets.as_ref().unwrap().widget(id).unwrap().wm_wants, widgets::Wants::Show);

    app.hide_settings_widget();
    assert_eq!(app.widgets.as_ref().unwrap().widget(id).unwrap().wm_wants, widgets::Wants::Hide);
}

/// `Widget.vue`'s `anchor`: the percentages of the overlay, shifted by the `translate` the
/// `pos` picks. The menu's `anchor: { pos: 'tl', x: 5, y: 5 }` puts it at 5 %/5 % of the game.
#[test]
fn a_widgets_anchor_places_it_in_the_overlay() {
    let bounds = Bounds { x: 0.0, y: 0.0, width: 1600.0, height: 1200.0 };
    let size = widgets::scene_size(GameId::Poe1, widgets::Kind::Menu);
    assert_eq!(anchored_offset("tl", (5.0, 5.0), size, Screen { bounds, ..SCREEN }), (80.0, 60.0));
    // `tc` centres the card on its x, `br` puts its bottom-right corner on the anchor.
    assert_eq!(anchored_offset("tc", (50.0, 10.0), (200.0, 100.0), Screen { bounds, ..SCREEN }), (700.0, 120.0));
    assert_eq!(anchored_offset("br", (100.0, 100.0), (200.0, 100.0), Screen { bounds, ..SCREEN }), (1400.0, 1100.0));
    // A 150 % window: the anchor is a percentage of the game in points, not physical pixels.
    assert_eq!(
        anchored_offset("tl", (5.0, 5.0), size, Screen { bounds, scale: 1.5 }),
        (f64_to_f32(1600.0 / 1.5 * 0.05), f64_to_f32(1200.0 / 1.5 * 0.05))
    );
}

/// Every `anchor.pos` of `Widget.vue`'s `widgetPosition`: the corner or edge the `translate`
/// names sits on the anchor, which is the point a move handle writes.
#[test]
fn every_anchor_pos_puts_its_named_corner_on_the_anchor() {
    let bounds = Bounds { x: 0.0, y: 0.0, width: 1600.0, height: 1200.0 };
    let size = (200.0, 100.0);
    let anchor = (30.0, 40.0);
    let (x, y) = (480.0, 480.0);
    let card = |pos: &str| anchored_offset(pos, anchor, size, Screen { bounds, ..SCREEN });
    // `tl` = no translate, `tc` = half the card to the left, `br` = all of it, ...
    assert_eq!(card("tl"), (x, y));
    assert_eq!(card("tc"), (x - 100.0, y));
    assert_eq!(card("tr"), (x - 200.0, y));
    assert_eq!(card("cr"), (x - 200.0, y - 50.0));
    assert_eq!(card("br"), (x - 200.0, y - 100.0));
    assert_eq!(card("bc"), (x - 100.0, y - 100.0));
    assert_eq!(card("bl"), (x, y - 100.0));
    assert_eq!(card("cl"), (x, y - 50.0));
    assert_eq!(card("cc"), (x - 100.0, y - 50.0));

    // And the pointer that wrote the anchor puts the same named corner under the cursor: the
    // percentages a drag saves are the ones this placement reads back.
    let pointer = (480.0, 480.0);
    let written = widgets::pointer_percent(pointer.into(), (1600.0, 1200.0));
    assert!((written.0 - anchor.0).abs() < 1e-4 && (written.1 - anchor.1).abs() < 1e-4, "{written:?}");
    for pos in ["tl", "tc", "tr", "cr", "br", "bc", "bl", "cl", "cc"] {
        let (left, top) = anchored_offset(pos, written, size, Screen { bounds, ..SCREEN });
        let mut axis = pos.chars();
        let corner_y = match axis.next() {
            Some('t') => top,
            Some('c') => top + size.1 / 2.0,
            _ => top + size.1,
        };
        let corner_x = match axis.next() {
            Some('l') => left,
            Some('c') => left + size.0 / 2.0,
            _ => left + size.0,
        };
        assert!(
            (corner_x - pointer.0).abs() < 0.01 && (corner_y - pointer.1).abs() < 0.01,
            "{pos}: {corner_x}, {corner_y}"
        );
    }
}

/// Any card is clamped inside the game, as `Widget.vue`'s `max-width`/`max-height` keep it:
/// the card never starts left of the game or ends right of it.
#[test]
fn a_card_stays_inside_the_overlay_at_every_edge() {
    let bounds = Bounds { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    let size = (200.0, 100.0);
    for pos in ["tl", "tc", "tr", "cr", "br", "bc", "bl", "cl", "cc"] {
        for anchor in [(0.0, 0.0), (100.0, 100.0), (0.0, 100.0), (100.0, 0.0), (50.0, 50.0)] {
            let (left, top) = anchored_offset(pos, anchor, size, Screen { bounds, ..SCREEN });
            assert!(left >= 0.0 && top >= 0.0, "{pos} at {anchor:?}: {left},{top}");
            assert!(left + size.0 <= f64_to_f32(bounds.width) + 0.01, "{pos} at {anchor:?}: {left}");
            assert!(top + size.1 <= f64_to_f32(bounds.height) + 0.01, "{pos} at {anchor:?}: {top}");
        }
    }
}

/// `AppConfig().overlayBackgroundClose` decides whether the overlay's background click gives
/// the game its focus back; a config without it is upstream's default, `true`.
#[test]
fn the_overlay_background_closes_on_a_click_by_default() {
    assert!(overlay_background_close(&config()));
    let mut config = config();
    config["overlayBackgroundClose"] = serde_json::json!(false);
    assert!(!overlay_background_close(&config));
    assert!(overlay_background_close(&serde_json::json!({})), "upstream's `true`");
}

#[test]
fn the_game_comes_from_the_flag() {
    let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    assert_eq!(game_from_args(&args(&[])), GameId::Poe1);
    assert_eq!(game_from_args(&args(&["--game", "poe2"])), GameId::Poe2);
    assert_eq!(game_from_args(&args(&["--game=poe2"])), GameId::Poe2);
    assert_eq!(game_from_args(&args(&["--scene", "x", "--game", "poe1"])), GameId::Poe1);
    assert_eq!(game_from_args(&args(&["--game", "nonsense"])), GameId::Poe1);
    // `--game` without a value is not a game; the next flag must not be eaten.
    assert_eq!(game_from_args(&args(&["--game", "--snap", "out.png"])), GameId::Poe1);
}

#[test]
fn a_window_is_sized_for_what_it_draws() {
    assert_eq!(window_size(GameId::Poe1, &Shown::None), None);
    assert_eq!(window_size(GameId::Poe1, &Shown::Settings), Some((crate::settings::WIDTH, crate::settings::HEIGHT)));
    assert_eq!(window_size(GameId::Poe1, &Shown::PriceCheck), Some((crate::poe1::price_check::WIDTH, HEIGHT)));
    assert_eq!(window_size(GameId::Poe1, &Shown::Poe2), Some((crate::poe2::price_check::WIDTH, HEIGHT)));
    assert_eq!(
        window_size(GameId::Poe1, &Shown::Widget(widgets::Kind::Menu.name().to_owned())),
        Some(widgets::scene_size(GameId::Poe1, widgets::Kind::Menu))
    );
    // A widget's card is its own size in each game: EE2's menu is wider than APT's.
    assert_eq!(
        window_size(GameId::Poe2, &Shown::Widget(widgets::Kind::Menu.name().to_owned())),
        Some(widgets::scene_size(GameId::Poe2, widgets::Kind::Menu))
    );
    assert_ne!(
        widgets::scene_size(GameId::Poe2, widgets::Kind::Menu).0,
        widgets::scene_size(GameId::Poe1, widgets::Kind::Menu).0
    );
    // A widget the port does not draw has no size rather than a wrong one.
    assert_eq!(window_size(GameId::Poe1, &Shown::Widget("unknown".to_owned())), None);
}

/// The settings window is upstream's ordinary window, not the overlay: it must never hide
/// with the game (that is what the overlay library's blur rule does to widgets).
#[test]
fn only_widgets_are_the_overlay() {
    assert!(is_overlay(&Shown::PriceCheck));
    assert!(is_overlay(&Shown::Poe2));
    assert!(is_overlay(&Shown::ItemCheck));
    assert!(is_overlay(&Shown::Widget(widgets::Kind::Menu.name().to_owned())));
    assert!(!is_overlay(&Shown::Settings));
    assert!(!is_overlay(&Shown::None));
}

/// `OverlayVisibility`'s Alt peek hides the widget the overlay draws, and only that: the
/// settings window is an ordinary window upstream and stays.
#[test]
fn the_alt_peek_hides_the_widget_but_not_the_settings_window() {
    assert!(peek_hides(&Shown::PriceCheck, false));
    assert!(peek_hides(&Shown::Poe2, false));
    assert!(peek_hides(&Shown::ItemCheck, false));
    assert!(peek_hides(&Shown::Widget(widgets::Kind::Menu.name().to_owned()), false));
    assert!(!peek_hides(&Shown::Settings, false), "the settings window is not the overlay");
    assert!(!peek_hides(&Shown::None, false), "there is nothing to hide");
    assert!(!peek_hides(&Shown::PriceCheck, true), "a visible overlay hides nothing");
}

/// A widget placed inside the overlay: at the cursor, or centred, always inside the card's
/// bounds of the game's client area, in points rather than physical pixels.
#[test]
fn the_widget_is_placed_inside_the_game_in_points() {
    let bounds = Bounds { x: 100.0, y: 50.0, width: 1600.0, height: 1200.0 };
    let size = (400.0, 300.0);

    // 12 px below and right of the cursor, relative to the game's client area.
    assert_eq!(
        widget_offset(Some(Placement::Cursor(Point { x: 300.0, y: 250.0 })), size, Screen { bounds, ..SCREEN }),
        (212.0, 212.0)
    );
    // At 150 % the game is 1600 physical px wide but the widget is laid out in points.
    assert_eq!(
        widget_offset(Some(Placement::Cursor(Point { x: 400.0, y: 350.0 })), size, Screen { bounds, scale: 1.5 }),
        (212.0, 212.0)
    );
    // A cursor near the bottom right corner: the card is pulled back inside the game.
    assert_eq!(
        widget_offset(Some(Placement::Cursor(Point { x: 1690.0, y: 1240.0 })), size, Screen { bounds, ..SCREEN }),
        (1200.0, 900.0)
    );
    // Above and left of the game: still inside it.
    assert_eq!(
        widget_offset(Some(Placement::Cursor(Point { x: 0.0, y: 0.0 })), size, Screen { bounds, ..SCREEN }),
        (0.0, 0.0)
    );
    // Centred for a window the tray opened.
    assert_eq!(widget_offset(Some(Placement::Centred), size, Screen { bounds, ..SCREEN }), (600.0, 450.0));
    assert_eq!(widget_offset(None, size, Screen { bounds, ..SCREEN }), (600.0, 450.0));
    // A card taller than the game starts at the top rather than at a negative offset.
    assert_eq!(widget_offset(None, (400.0, 1500.0), Screen { bounds, ..SCREEN }), (600.0, 0.0));
}

/// The live window is the overlay on Windows and the port's ordinary window elsewhere.
#[test]
fn the_windows_window_is_the_overlay() {
    let options = window_options();
    let windows = cfg!(target_os = "windows");
    assert_eq!(options.transparent, windows);
    assert_eq!(options.borderless, windows);
    assert_eq!(options.click_through, windows);
    assert_eq!(options.always_on_top, windows);
    assert_eq!(options.min_size, windows.then_some((0.0, 0.0)));
    if !windows {
        assert_eq!(options.min_size, None, "other platforms keep Rune's minimum");
    }
}

/// The price check's width is the same in both games (`#price-window`'s 28.75 rem), so one
/// window serves both.
#[test]
#[expect(clippy::float_cmp_const, reason = "both widths are the same `28.75 * REM` constant")]
fn both_games_price_checks_are_the_same_width() {
    assert_eq!(crate::poe1::price_check::WIDTH, crate::poe2::price_check::WIDTH);
}

#[test]
fn a_scene_has_no_live_shell() {
    let app = App::settings_scene(GameId::Poe1, Page::Hotkeys);
    assert!(app.live.is_none());
    assert_eq!(app.shown(), Shown::Settings);
    let app = App::replay("currency-divine-orb");
    assert!(app.live.is_none());
    assert_eq!(app.shown(), Shown::PriceCheck);
    let app = App::widgets_scene(GameId::Poe1, widgets::Kind::Menu);
    assert_eq!(app.shown(), Shown::Widget(widgets::Kind::Menu.wm_type().to_owned()));
}

/// The live app starts with nothing to draw, which is what keeps the window hidden until a
/// hotkey opens a widget.
#[test]
fn a_live_app_starts_idle() {
    let dir = std::env::temp_dir().join(format!("wraex-shell-idle-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    let startup = config::load(GameId::Poe1, &dir);
    // `Live` needs a Rune handle, which a unit test has no app for: the pieces the idle state
    // depends on are the config and the shown window, and `App::live` is exercised by the run.
    assert_eq!(startup.source, config::Source::Defaults);
    assert!(startup.folder().is_some());
    drop(std::fs::remove_dir_all(&dir));
}

#[test]
fn the_leagues_default_is_upstreams() {
    assert_eq!(default_league(GameId::Poe1), "Standard");
    assert_eq!(default_league(GameId::Poe2), "Standard");
}

/// A config whose `widgets` are broken still yields a host config (an empty hotkey list)
/// rather than failing the app's start.
#[test]
fn a_broken_config_falls_back_to_an_empty_host_config() {
    let host = host_config(GameId::Poe1, &serde_json::json!({}));
    assert_eq!(host.shortcuts, Vec::new());
    assert_eq!(host.overlay_key, "Shift + Space");
    assert_eq!(host.language, "en");
}

/// The price-check hotkey's whole effect: `MAIN->CLIENT::item-text` opens the window on the
/// item, in the selected league. Driven with the recorded answers, as a scene is.
#[test]
fn the_price_check_hotkey_opens_the_window_on_the_item() {
    let mut app = App::item_check_scene(GameId::Poe1, "currency-divine-orb");
    app.close_windows();
    assert_eq!(app.shown(), Shown::None);

    let text = crate::corpus("currency-divine-orb");
    app.open_price_check_for(PriceCheckRequest {
        game: GameId::Poe1,
        league: "Standard",
        mode: Mode::Replay,
        text: &text,
        warm: None,
        overviews: None,
    });
    assert_eq!(app.shown(), Shown::PriceCheck);
    assert!(app.price_check.open);
    assert!(app.price_check.item.is_ok(), "the item parsed");
    assert_eq!(app.price_check.league, "Standard", "the league the trade site lists");
    assert_eq!(app.game, GameId::Poe1);
    // The replay's prices are the app's, so the title bar shows its rate.
    assert!(app.price_check.stable_orb_cost().is_some());

    // A league the recordings do not cover still opens the window, in that league.
    app.close_windows();
    app.open_price_check_for(PriceCheckRequest {
        game: GameId::Poe1,
        league: "Covens",
        mode: Mode::Replay,
        text: &text,
        warm: None,
        overviews: None,
    });
    assert_eq!(app.price_check.league, "Covens");
    assert!(app.price_check.stable_orb_cost().is_none(), "no prices for an unrecorded league");
}

/// The warm prices the shell fetched at launch are the window's, not a second fetch.
#[test]
fn the_startup_prices_become_the_windows() {
    let mut warm = crate::poe1::price_check::prices::PriceList::new(Mode::Replay);
    warm.queue("Standard", false);
    assert!(warm.xchg_rate().is_some());

    let mut app = App::replay("currency-divine-orb");
    app.close_windows();
    app.open_price_check_for(PriceCheckRequest {
        game: GameId::Poe1,
        league: "Standard",
        mode: Mode::Replay,
        text: &crate::corpus("currency-divine-orb"),
        warm: Some(warm),
        overviews: None,
    });
    assert_eq!(app.price_check.stable_orb_cost(), Some(891.0));
}

/// The item-check hotkey opens its widget, and the widget's own hotkeys press their button.
#[test]
fn the_item_check_hotkey_opens_its_widget_and_page() {
    let text = crate::corpus("adv-rare-ring-mirrored-catalyst");
    let config = crate::settings::default_config(GameId::Poe1);
    let mut app = App::replay("currency-divine-orb");
    app.close_windows();

    app.open_item_check_for(&ItemCheckRequest {
        game: GameId::Poe1,
        config: &config,
        mode: Mode::Replay,
        text: &text,
        action: None,
    });
    assert_eq!(app.shown(), Shown::ItemCheck);
    assert!(app.item_check.as_ref().is_some_and(|check| check.open));
    assert!(app.item_check.as_ref().unwrap().actions.is_empty(), "a plain item is just shown");

    app.open_item_check_for(&ItemCheckRequest {
        game: GameId::Poe1,
        config: &config,
        mode: Mode::Replay,
        text: &text,
        action: Some(ItemAction::Wiki),
    });
    let actions = &app.item_check.as_ref().unwrap().actions;
    assert!(matches!(&actions[0], crate::poe1::item_check::Action::Open(url) if url.contains("poewiki.net")));

    // A text that is not an item opens nothing.
    let mut app = App::replay("currency-divine-orb");
    app.close_windows();
    app.open_item_check_for(&ItemCheckRequest {
        game: GameId::Poe1,
        config: &config,
        mode: Mode::Replay,
        text: "not an item",
        action: None,
    });
    assert_eq!(app.shown(), Shown::None);
}

/// The PoE2 window, with the league the PoE2 trade site lists.
#[test]
fn the_poe2_hotkey_opens_its_own_window() {
    let text = crate::corpus_poe2("currency-djinn-barya");
    let mut app = App::replay("currency-divine-orb");
    app.close_windows();
    app.open_price_check_for(PriceCheckRequest {
        game: GameId::Poe2,
        league: "Dawn of the Hunt",
        mode: Mode::Replay,
        text: &text,
        warm: None,
        overviews: None,
    });
    assert_eq!(app.game, GameId::Poe2);
    assert_eq!(app.shown(), Shown::Poe2);
    assert_eq!(app.poe2.as_ref().unwrap().league, "Dawn of the Hunt");
    assert!(app.poe2.as_ref().unwrap().item.is_ok());
}

/// The price-check hotkey's overlay is `showInactive()` + click-through: the game keeps the
/// keyboard and the mouse, and the card is on screen. This is the first Windows test's bug:
/// `open()`/focus made every price check alt-tab the player out of the game.
#[test]
fn a_hotkey_widget_is_shown_inactive_and_keeps_the_game_focused() {
    let mut harness = Harness::playing();
    harness.open_price_check(Point { x: 800.0, y: 600.0 }, 1.0);

    assert!(harness.window.is_active(), "the game still has the focus");
    assert!(!harness.overlay.is_interactable(), "the card is not taking the mouse yet");
    assert!(harness.handle.is_visible(), "the card is on screen");
    assert_eq!(harness.handle.click_through, Some(true), "clicks go through to the game");
    assert_eq!(harness.handle.calls, vec![OverlayCall::Show, OverlayCall::ClickThrough(true)]);
}

/// `WidgetAreaTracker`'s `mousemove`: with `hotkeyHold` down, moving onto the card takes the
/// overlay (click-through off) and the focus, as `activateOverlay` does.
#[test]
fn the_mouse_onto_the_widget_activates_the_overlay() {
    let mut harness = Harness::playing();
    harness.open_price_check(Point { x: 800.0, y: 600.0 }, 1.0);
    let calls = harness.handle.calls.len();

    // Ctrl is still held (`keepModKeys`), and the point is inside the full-height card at
    // x = 812 … 1272, y = 0 … 1200.
    harness.mouse(MouseEvent::Move { point: Point { x: 900.0, y: 700.0 }, ctrl: true, alt: false });

    assert!(harness.overlay.is_interactable());
    assert_eq!(harness.handle.click_through, Some(false));
    assert!(!harness.window.is_active(), "the overlay took the focus");
    assert_eq!(
        harness.handle.calls[calls..],
        [OverlayCall::ClickThrough(false)],
        "the already-visible window is not shown again"
    );
    assert_eq!(
        harness.overlay.take_events(),
        vec![OverlayEvent::FocusChange { game: false, overlay: true, using_hotkey: false }]
    );

    // A focus change to the overlay alone does not close the card.
    let mut app = App::replay("currency-divine-orb");
    apply_overlay_event(OverlayEvent::FocusChange { game: false, overlay: true, using_hotkey: false }, &mut app);
    assert_eq!(app.shown(), Shown::PriceCheck);
}

/// Moving away from the press position without the hold key closes the price check:
/// `WidgetAreaTracker` sends `hide-exclusive-widget`, which only `PriceCheckWindow.vue` listens
/// to. This is the Windows-test bug where the card stayed on screen forever.
#[test]
fn moving_away_from_the_widget_closes_it() {
    let mut harness = Harness::playing();
    harness.open_price_check(Point { x: 800.0, y: 600.0 }, 1.0);
    // Ctrl released (`keepModKeys` only leaves it down while the user holds it).
    harness.mouse(MouseEvent::Move { point: Point { x: 100.0, y: 100.0 }, ctrl: false, alt: false });

    let events = harness.overlay.take_events();
    assert_eq!(events, vec![OverlayEvent::HideExclusiveWidget]);
    assert!(!harness.overlay.is_tracking_area(), "the listeners are removed");

    let mut app = App::replay("currency-divine-orb");
    assert_eq!(app.shown(), Shown::PriceCheck);
    apply_overlay_event(events[0], &mut app);
    assert_eq!(app.shown(), Shown::None);
}

/// Escape while the overlay has the keyboard (`OverlayWindow.handleExtraCommands`): the game
/// gets the focus back, and the `focus-change` closes the price check through its
/// `hide-on-blur` flag, so nothing is left on screen unreachable.
#[test]
fn escape_closes_the_card_and_gives_the_game_its_focus() {
    let mut harness = Harness::playing();
    harness.open_price_check(Point { x: 800.0, y: 600.0 }, 1.0);
    harness.mouse(MouseEvent::Move { point: Point { x: 900.0, y: 700.0 }, ctrl: true, alt: false });
    assert!(harness.overlay.is_interactable());
    harness.overlay.take_events();

    let escape = OverlayInput { code: "Escape".into(), ctrl: false, shift: false, alt: false };
    assert!(harness.overlay.on_overlay_input(&escape, &mut harness.window, &mut harness.handle));
    assert!(harness.window.is_active(), "the game has the focus again");
    assert!(!harness.overlay.is_interactable());
    harness.reconcile_click_through();
    assert_eq!(harness.handle.click_through, Some(true));

    let events = harness.overlay.take_events();
    assert_eq!(events, vec![OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }]);
    let mut app = App::replay("currency-divine-orb");
    apply_overlay_event(events[0], &mut app);
    assert_eq!(app.shown(), Shown::None);
}

/// The game taking the focus back (the player clicked into it) closes a non-pinned widget, as
/// `OverlayWindow.vue`'s `focus-change` handler does.
#[test]
fn the_game_taking_the_focus_back_closes_the_card() {
    let mut harness = Harness::playing();
    harness.open_price_check(Point { x: 800.0, y: 600.0 }, 1.0);
    harness.mouse(MouseEvent::Move { point: Point { x: 900.0, y: 700.0 }, ctrl: true, alt: false });
    harness.overlay.take_events();

    // The OS reports the game as the foreground window again.
    harness.feed(wraex_platform::window::GameWindowEvents::Focus);
    assert!(harness.window.is_active());
    assert!(!harness.overlay.is_interactable());

    let events = harness.overlay.take_events();
    assert_eq!(events, vec![OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }]);
    let mut app = App::replay("currency-divine-orb");
    apply_overlay_event(events[0], &mut app);
    assert_eq!(app.shown(), Shown::None);
}

/// The tracker area is what upstream's renderer reports: the card in screen DIPs, the press
/// position, the hold key and the DPI scale for Win32's conversion.
#[test]
fn the_track_area_is_the_card_in_screen_points() {
    let bounds = Bounds { x: 100.0, y: 50.0, width: 1600.0, height: 1200.0 };
    let screen = Screen { bounds, scale: 2.0 };
    let size = overlay_widget_size(GameId::Poe1, &Shown::PriceCheck, screen).expect("a size");
    assert_eq!(size, (crate::poe1::price_check::WIDTH, 600.0), "the price check is h-full");
    assert_eq!(
        overlay_widget_size(GameId::Poe1, &Shown::Widget(widgets::Kind::Menu.name().to_owned()), screen),
        Some(widgets::scene_size(GameId::Poe1, widgets::Kind::Menu)),
        "other cards keep their own size"
    );

    let offset = (10.0, 20.0);
    let area = track_area(offset, size, screen, Point { x: 300.0, y: 250.0 }, Some("Ctrl".to_owned()));
    assert_eq!(area.hold_key.as_deref(), Some("Ctrl"));
    assert_eq!(area.close_threshold, 2.5 * f64::from(crate::theme::REM), "2.5 rem in points");
    assert_eq!(area.from, Point { x: 150.0, y: 125.0 }, "physical pixels to DIPs");
    assert_eq!(
        area.area,
        Bounds { x: 60.0, y: 45.0, width: f64::from(crate::poe1::price_check::WIDTH), height: 600.0 }
    );
    assert_eq!(area.dpr, 2.0);
    // `TrackArea::into_screen_space` doubles it back on Windows, landing on the card's
    // physical rectangle.
    assert_eq!(area.area.x * 2.0, 120.0);
}

/// The hold key comes from the config's price-check widget (`priceCheck.hotkeyHold`).
#[test]
fn the_price_check_hold_key_comes_from_the_config() {
    fn set_hold(config: &mut serde_json::Value, hold: serde_json::Value) {
        let price_check = config["widgets"]
            .as_array_mut()
            .expect("widgets")
            .iter_mut()
            .find(|widget| widget["wmType"] == "price-check")
            .expect("the price-check widget");
        price_check["hotkeyHold"] = hold;
    }

    assert_eq!(price_check_hold_key(&config()).as_deref(), Some("Ctrl"));
    let mut config = config();
    set_hold(&mut config, serde_json::json!("Alt"));
    assert_eq!(price_check_hold_key(&config).as_deref(), Some("Alt"));
    set_hold(&mut config, serde_json::Value::Null);
    assert_eq!(price_check_hold_key(&config), None, "`undefined` is not a key");
    assert_eq!(price_check_hold_key(&serde_json::json!({})), None);
}

/// The overlay's Escape/Ctrl+W commands belong to the overlay window; the settings window is
/// an ordinary window upstream and keeps its keys and clicks while it is open.
#[test]
fn overlay_commands_do_not_touch_the_settings_window() {
    assert!(overlay_commands_apply(&Shown::PriceCheck, true));
    assert!(overlay_commands_apply(&Shown::Widget("menu".to_owned()), true));
    assert!(!overlay_commands_apply(&Shown::PriceCheck, false), "the overlay does not hold the keys");
    assert!(!overlay_commands_apply(&Shown::Settings, true), "the settings window keeps its escape");
}

/// One `OverlayVisibility` peek's timer: time only moves when a test advances it.
#[derive(Debug, Default)]
struct TestClock(std::cell::Cell<wraex_core::trade::rate_limit::Millis>);

impl TestClock {
    fn advance(&self, millis: wraex_core::trade::rate_limit::Millis) {
        self.0.set(self.0.get().saturating_add(millis));
    }
}

impl Clock for TestClock {
    fn now(&self) -> wraex_core::trade::rate_limit::Millis {
        self.0.get()
    }

    fn sleep(&self, millis: wraex_core::trade::rate_limit::Millis) {
        self.advance(millis);
    }
}

/// The global keyboard hook's Alt peek, as `Live::poll_keyboard` feeds it to `OverlayVisibility`:
/// holding Alt alone hides the widget after the 275 ms delay, and the release brings it back.
#[test]
fn the_alt_peek_hides_the_widget_until_alt_is_released() {
    let mut harness = Harness::playing();
    harness.open_price_check(Point { x: 800.0, y: 600.0 }, 1.0);
    let clock = TestClock::default();
    let shortcuts = Shortcuts::new(GameId::Poe1, wraex_core::game_files::Os::Windows);
    let alt = |alt| InputEvent { key: Some("Alt"), ctrl: false, shift: false, alt };

    apply_key_event(&mut harness.overlay, &shortcuts, KeyEvent::Down(alt(true)), &clock);
    assert_eq!(
        harness.overlay.visibility_deadline(),
        Some(wraex_platform::overlay::VISIBILITY_HIDE_DELAY),
        "a tap does not flicker the widget: the hide waits"
    );
    assert!(harness.overlay.is_visible());

    clock.advance(wraex_platform::overlay::VISIBILITY_HIDE_DELAY);
    harness.overlay.poll(&clock);
    assert!(!harness.overlay.is_visible(), "holding Alt hides the widgets");
    assert_eq!(harness.overlay.take_events(), vec![OverlayEvent::Visibility { is_visible: false }]);

    apply_key_event(&mut harness.overlay, &shortcuts, KeyEvent::Up(alt(false)), &clock);
    assert!(harness.overlay.is_visible(), "releasing Alt brings them back");
    assert_eq!(harness.overlay.take_events(), vec![OverlayEvent::Visibility { is_visible: true }]);
}

/// `OverlayVisibility`'s other keydown branch: a key pressed while Alt is held ends the peek, so
/// Alt+Tab or Alt+F4 are not a peek.
#[test]
fn a_key_pressed_while_alt_is_held_ends_the_peek() {
    let mut harness = Harness::playing();
    let clock = TestClock::default();
    let shortcuts = Shortcuts::new(GameId::Poe1, wraex_core::game_files::Os::Windows);
    let controller = &mut harness.overlay;

    let alt = InputEvent { key: Some("Alt"), ctrl: false, shift: false, alt: true };
    apply_key_event(controller, &shortcuts, KeyEvent::Down(alt), &clock);
    assert!(controller.visibility_deadline().is_some());

    let alt_tab = InputEvent { key: Some("Tab"), ctrl: false, shift: false, alt: true };
    apply_key_event(controller, &shortcuts, KeyEvent::Down(alt_tab), &clock);
    assert_eq!(controller.visibility_deadline(), None, "the peek is cancelled");
    assert!(controller.is_visible());

    // The Alt release is a no-op then: the widget never went away.
    apply_key_event(controller, &shortcuts, KeyEvent::Up(InputEvent { alt: false, ..alt }), &clock);
    assert!(controller.is_visible());
    assert_eq!(controller.take_events(), Vec::new());
}

/// A shell a unit test drives the way Rune does: [`Live::for_tests`] gives it a real (never-run)
/// `AppHandle` and a `Panel` with no native window behind it, the config lives in a temp folder,
/// and the app's trade requests go to the recordings (`Mode::Replay`).
fn live_app(name: &str, clipboard: TestClipboard) -> App {
    let dir = live_dir(name);
    live_app_in(&dir, clipboard)
}

/// A fresh temp folder for a test's `appData`, so the config a test saves can be read back.
fn live_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("wraex-shell-{name}-{}", std::process::id()));
    drop(std::fs::remove_dir_all(&dir));
    dir
}

/// [`live_app`] with the folder given, for a test that restarts from the config it saved.
fn live_app_in(dir: &std::path::Path, clipboard: TestClipboard) -> App {
    let startup = config::load(GameId::Poe1, dir);
    App::live(Live::for_tests(startup, Box::new(clipboard)))
}

/// One frame of the real path: exactly what `AppSpec::poll` is wired to in [`run`].
fn poll(app: &mut App) -> bool {
    let mut tx = Tx::default();
    app.poll_with(0.0, &mut tx)
}

/// The live overlay in a Rune window of the game's size, drawn the way [`crate::view`] draws it
/// over the game (`App::cards_for`). The shell's poll is left to the test (`poll`), so the
/// overlay keeps the size [`Live::sync_window`] would read from the game.
fn overlay_runtime(app: App) -> rune::Runtime<App> {
    let text = rune::text::TextSystem::new(crate::theme::FONTS, crate::theme::FAMILIES);
    let size = (f64_to_f32(BOUNDS.width), f64_to_f32(BOUNDS.height));
    let mut runtime = rune::Runtime::with_text(app, game_overlay, size, rune::Env::new(true, 1.0), text);
    runtime.frame(0.0);
    runtime
}

/// The `wmId`s of the cards the overlay draws over the game, in paint order.
fn card_ids(app: &App) -> Vec<Option<u32>> {
    app.cards_for(true, SCREEN, None).iter().map(|card| card.wm_id).collect()
}

/// The saved config, read the way the next start reads it.
fn saved_config(dir: &std::path::Path) -> serde_json::Value {
    let path = crate::settings::config_path_in(GameId::Poe1, dir);
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The saved widget object for `wm_id`.
fn saved_widget(config: &serde_json::Value, wm_id: u32) -> serde_json::Value {
    let saved = SavedWidgets::deserialize(config).unwrap_or_default();
    saved
        .widgets
        .into_iter()
        .find(|widget| SavedWidgetId::deserialize(widget).is_ok_and(|saved| saved.wm_id == wm_id))
        .unwrap_or_else(|| panic!("no saved widget {wm_id}"))
}

/// A saved config's `widgets`, as the file has them.
#[derive(Debug, Default, Deserialize)]
struct SavedWidgets {
    #[serde(default)]
    widgets: Vec<serde_json::Value>,
}

/// One saved widget's `wmId`, for finding it in the config's `widgets`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedWidgetId {
    wm_id: u32,
}

/// The config's `toggle-overlay` hotkey (upstream's `overlayKey`, `Shift + Space`).
fn overlay_key(app: &App) -> ShortcutAction {
    let live = app.live.as_ref().expect("the live shell");
    live.shortcuts
        .actions()
        .iter()
        .find(|entry| entry.action == Action::ToggleOverlay)
        .cloned()
        .expect("the config's overlay key")
}

/// The config's plain price-check hotkey (`copy-item` to `price-check`, `focusOverlay: false`).
fn price_check_hotkey(app: &App) -> ShortcutAction {
    let live = app.live.as_ref().expect("the live shell");
    live.shortcuts
        .actions()
        .iter()
        .find(|entry| matches!(&entry.action, Action::CopyItem { target, .. } if target == "price-check"))
        .cloned()
        .expect("the config's price-check hotkey")
}

/// The config's chat-command hotkey that types `text` (`paste-in-chat`): `/hideout` on F5.
fn chat_hotkey(app: &App, text: &str) -> ShortcutAction {
    let live = app.live.as_ref().expect("the live shell");
    live.shortcuts
        .actions()
        .iter()
        .find(|entry| matches!(&entry.action, Action::PasteInChat { text: command, .. } if command.as_str() == text))
        .cloned()
        .unwrap_or_else(|| panic!("no `{text}` chat hotkey"))
}

/// A hotkey press as Rune delivers it: the handler `Live::register_hotkeys` installs, which is
/// `App::on_hotkey` around `Live::press`.
fn press(app: &mut App, of: fn(&App) -> ShortcutAction) {
    let entry = of(app);
    app.on_hotkey(&entry);
}

/// The OS window hook's report, fed through the overlay controller as `Live::poll_game_window`
/// does. A test's shell has no `Watcher`, so the queued `focus-change` is what the next
/// `Live::poll` drains.
fn game_window(app: &mut App, event: window::GameWindowEvents) {
    let live = app.live.as_mut().expect("the live shell");
    let mut panel = OverlayPanel::new(Arc::clone(&live.panel));
    if let Some(change) = live.window.feed(event) {
        live.overlay.on_window_event(&change, &live.window, &mut panel);
    }
}

/// What the overlay window is doing, as the shell recorded it: the window's `Panel` (the
/// `AppHandle` is Rune's and never shown in a test) and the cards its last poll drew.
fn drawn(app: &App) -> Vec<Shown> {
    app.live.as_ref().expect("the live shell").drawn.clone()
}

fn window_visible(app: &App) -> bool {
    app.live.as_ref().expect("the live shell").panel.visible()
}

/// The clipboard the shell reads, without an OS behind it.
#[derive(Debug, Default)]
struct TestClipboard {
    /// What `read_text` answers next.
    text: String,
    /// What the game writes over it when the app's copy key clears it.
    copies: Option<String>,
    /// Everything the app wrote, shared so a test keeps it after the shell takes the clipboard.
    written: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
}

impl TestClipboard {
    /// The item already on the clipboard: what the no-synthesis path prices.
    fn holding(item: &str) -> Self {
        Self { text: item.to_owned(), copies: None, written: Arc::default() }
    }

    /// An empty clipboard the game fills with `item` once the copy key clears it.
    fn copy_of(item: &str) -> Self {
        Self { text: String::new(), copies: Some(item.to_owned()), written: Arc::default() }
    }
}

impl Clipboard for TestClipboard {
    fn read_text(&mut self) -> String {
        let text = std::mem::take(&mut self.text);
        if let Some(item) = &self.copies {
            self.text = item.clone();
        }
        text
    }

    fn write_text(&mut self, text: &str) {
        self.text = text.to_owned();
        self.written.lock().expect("the write log").push(text.to_owned());
    }
}

/// The first in-game bug, through the real path: the overlay key activates the overlay, and the
/// shell's own poll has to draw the widget menu and keep the window on screen.
///
/// The old shell asked `App::shown()` from inside `Live::sync_window`, where `App::poll_with`
/// has taken the shell out of `App::live`; that answer is a headless scene's, so the overlay key
/// activated the overlay and the window hid again at once.
#[test]
fn the_overlay_key_through_poll_with_draws_the_menu() {
    let mut app = live_app("overlay-key", TestClipboard::default());

    press(&mut app, overlay_key);
    poll(&mut app);

    let live = app.live.as_ref().expect("the live shell");
    assert!(live.overlay.is_interactable(), "the overlay key took the mouse and keyboard");
    assert_eq!(
        drawn(&app),
        vec![Shown::Widget(widgets::menu::WM_TYPE.to_owned())],
        "the active overlay draws its menu"
    );
    assert!(window_visible(&app), "the overlay window stays on screen");
    let shown = app.shown_state(OverlayMode::Active);
    assert!(
        !hides_window(&shown, Visibility { following_game: false, in_front: true }),
        "nothing hides a window with a card to draw"
    );

    // And the other side of the same read: the game takes the focus back, and the menu is
    // `invisible-on-blur`, so the overlay draws nothing and its window hides. A shell that
    // asked `App::shown()` would fall back to the scene case here and keep drawing the menu.
    game_window(&mut app, window::GameWindowEvents::Focus);
    poll(&mut app);
    assert!(drawn(&app).is_empty(), "an inactive overlay draws nothing: {:?}", drawn(&app));
    assert!(!window_visible(&app), "the overlay window hides with nothing to draw");
}

/// The widget state is the shell's to build: the first poll has to create it, with the app not
/// knowing it is live at all (`App::poll_with` has it out). The second in-game bug guarded the
/// build on `self.live.is_some()`, so there was never a widget list to draw from.
#[test]
fn the_first_poll_with_builds_the_overlay_state() {
    let mut app = live_app("widget-state", TestClipboard::default());
    assert!(app.widgets.is_none(), "a fresh app has no widget state");

    poll(&mut app);

    let overlay = app.widgets.as_ref().expect("the first poll builds the widget list");
    assert!(
        overlay.widgets.iter().any(|widget| widget.wm_type == widgets::menu::WM_TYPE),
        "the config's menu is in it"
    );
}

/// The third in-game bug: opening the settings from the tray bounces the game's foreground back
/// before the overlay's own focus lands (the tray menu closing), and that bounce used to cancel
/// the window at once. Only a focus after the overlay has held it closes the settings.
#[test]
fn the_settings_focus_bounce_keeps_the_window_open() {
    let mut app = live_app("settings-bounce", TestClipboard::default());
    // The tray's "Settings/League" click, as the native menu callback queues it.
    app.live
        .as_mut()
        .expect("the live shell")
        .tray_actions
        .lock()
        .expect("the tray queue")
        .push(TrayAction::ShowSettingsHint);
    poll(&mut app);
    let live = app.live.as_ref().expect("the live shell");
    assert!(app.settings.as_ref().is_some_and(|settings| settings.open), "the tray opened the settings");
    assert!(!live.settings_state.armed, "the overlay's focus has not landed yet");

    // The tray menu closing hands the foreground back to the game; the overlay then focuses.
    // The bounce is not the user going back to the game.
    game_window(&mut app, window::GameWindowEvents::Focus);
    poll(&mut app);
    assert!(
        app.settings.as_ref().is_some_and(|settings| settings.open),
        "the focus bounce keeps the settings window open"
    );

    // The overlay's focus lands (`AppSpec::on_focus`), and only now does a game focus count.
    let mut tx = Tx::default();
    app.on_focus(true, &mut tx);
    assert!(app.live.as_ref().expect("the live shell").settings_state.armed, "the overlay held the focus");
    game_window(&mut app, window::GameWindowEvents::Blur);
    game_window(&mut app, window::GameWindowEvents::Focus);
    poll(&mut app);

    assert!(
        !app.settings.as_ref().is_some_and(|settings| settings.open),
        "a real game focus closes the settings window"
    );
    assert!(drawn(&app).is_empty(), "and nothing is left to draw: {:?}", drawn(&app));
    assert!(!window_visible(&app), "the window is hidden");
}

/// The price-check hotkey through the real path: the copied item's card is drawn, and the game
/// taking the focus back closes it (`PriceCheckWindow.vue`'s `hide-on-blur`). Both halves are
/// the shell's own state, read while `App::poll_with` has `App::live` out of the app.
#[test]
fn the_price_check_hotkey_draws_its_card_until_the_game_takes_the_focus() {
    let text = crate::corpus("currency-divine-orb");
    let mut app = live_app("price-check", TestClipboard::holding(&text));
    if app.live.as_ref().is_some_and(|live| live.keys.can_synthesize()) {
        // A build that can press the game's copy key reads the clipboard after the press.
        app.live.as_mut().expect("the live shell").clipboard = Box::new(TestClipboard::copy_of(&text));
    }

    press(&mut app, price_check_hotkey);
    poll(&mut app);

    assert!(app.price_check.open, "the hotkey opened the price check");
    assert_eq!(drawn(&app), vec![Shown::PriceCheck], "the shell draws the price check's card");
    assert!(window_visible(&app), "the window is on screen");

    // The player clicks back into the game: the `focus-change` closes the card.
    game_window(&mut app, window::GameWindowEvents::Focus);
    poll(&mut app);

    assert!(!app.price_check.open, "`hide-on-blur`");
    assert!(drawn(&app).is_empty(), "nothing is drawn any more: {:?}", drawn(&app));
    assert!(!window_visible(&app), "the window is hidden");
}

/// `/hideout` twice through the real path: the first press writes the command and schedules the
/// clipboard restore (`RESTORE_AFTER`); the shell's poll has to run that timer, or the restore
/// stays pending and every later chat command is silently dropped by `restore_shortly`'s
/// throttle. With `restoreClipboard` on, the user's text comes back in between.
#[test]
fn a_chat_command_works_again_after_its_clipboard_restore() {
    let written = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let dir = live_dir("chat-restore");
    let mut startup = config::load(GameId::Poe1, &dir);
    startup.config["restoreClipboard"] = serde_json::json!(true);
    let clipboard =
        TestClipboard { text: "a password".to_owned(), written: Arc::clone(&written), ..TestClipboard::default() };
    let mut app = App::live(Live::for_tests(startup, Box::new(clipboard)));
    app.mode = Mode::Replay;

    let entry = chat_hotkey(&app, "/hideout");
    app.on_hotkey(&entry);
    assert_eq!(*written.lock().unwrap(), vec!["/hideout"], "the first press types the command");
    assert!(
        app.live.as_mut().expect("the live shell").shortcuts.clipboard_mut().pending_restore_deadline().is_some(),
        "the restore is scheduled"
    );

    // The live clock passes the deadline with no further key press; the shell's poll (which the
    // wake it armed brings) applies the restore and puts the previous clipboard text back.
    std::thread::sleep(std::time::Duration::from_millis(wraex_platform::clipboard::RESTORE_AFTER + 20));
    poll(&mut app);
    assert_eq!(*written.lock().unwrap(), vec!["/hideout", "a password"], "`restoreClipboard` puts the text back");
    assert!(
        app.live.as_mut().expect("the live shell").shortcuts.clipboard_mut().pending_restore_deadline().is_none(),
        "the restore ran"
    );

    // The bug: with the restore left pending, this second press was dropped.
    app.on_hotkey(&entry);
    assert_eq!(*written.lock().unwrap(), vec!["/hideout", "a password", "/hideout"], "the second press goes through");
}

/// `Widget.vue`'s move handles through the real path: a drag of the stopwatch's centre handle
/// takes the card with the pointer every frame, the release writes the new `anchor` into the
/// widget's config, and the next poll saves it (`saveConfig()`, which upstream runs on the
/// overlay's blur). A restart reads the same position and paint order back.
#[test]
fn dragging_a_widgets_handle_moves_it_and_saves_the_anchor() {
    let dir = live_dir("move");
    let mut app = live_app_in(&dir, TestClipboard::default());
    poll(&mut app);
    // The menu's "add widget…" (`wm.create`): a stopwatch, as `WidgetTimer.vue` mounts it.
    let timer = app.widgets.as_mut().expect("the overlay").create(widgets::Kind::Stopwatch.wm_type());
    poll(&mut app);
    let menu = widget_id(&app, widgets::menu::WM_TYPE);
    // The overlay over the game is the game's client area in points, which is what a drag's
    // percentages are of (`Live::sync_window` sets it from the game every frame).
    let overlay = app.widgets.as_mut().expect("the overlay");
    overlay.moving = Some(timer);
    overlay.size = (f64_to_f32(BOUNDS.width), f64_to_f32(BOUNDS.height));

    let mut runtime = overlay_runtime(app);
    let size = widgets::scene_size(GameId::Poe1, widgets::Kind::Stopwatch);
    assert_eq!(runtime.state.widgets.as_ref().unwrap().widget(timer).unwrap().anchor(), (50.0, 50.0));
    let key = format!("widget-{timer}-handle-cc");
    let (hx, hy) = runtime.find(&key).unwrap_or_else(|| panic!("no `{key}`"));
    runtime.pointer_move(hx, hy);
    runtime.pointer_down(hx, hy);
    runtime.frame(1.0 / 60.0);

    // A quarter into the overlay: the card follows the pointer every frame, no spring.
    runtime.pointer_move(400.0, 300.0);
    runtime.frame(1.0 / 60.0);
    assert_eq!(runtime.state.widgets.as_ref().unwrap().widget(timer).unwrap().anchor(), (25.0, 25.0));
    let cards = runtime.state.cards_for(true, SCREEN, None);
    let card = cards.iter().find(|card| card.wm_id == Some(timer)).expect("the timer's card");
    assert_eq!(card.offset, anchored_offset("cc", (25.0, 25.0), size, SCREEN));

    runtime.pointer_up(400.0, 300.0);
    runtime.frame(1.0 / 60.0);
    let mut app = runtime.state;
    poll(&mut app);

    let saved = saved_config(&dir);
    let widget = saved_widget(&saved, timer);
    assert_eq!(widget["anchor"], serde_json::json!({ "pos": "cc", "x": 25.0, "y": 25.0 }));
    assert_eq!(widget["wmZorder"], 2.0, "the press also raised it above the menu");

    // A restart from that file: the same anchor, and the same paint order.
    let mut restarted = live_app_in(&dir, TestClipboard::default());
    poll(&mut restarted);
    let overlay = restarted.widgets.as_ref().expect("the overlay");
    let widget = overlay.widget(timer).expect("the stopwatch survived the restart");
    assert_eq!(widget.anchor_pos(), "cc");
    assert_eq!(widget.anchor(), (25.0, 25.0));
    assert_eq!(widget.zorder(), Some(2.0));
    assert_eq!(card_ids(&restarted), vec![Some(menu), Some(timer)], "the menu, then the stopwatch");
    let cards = restarted.cards_for(true, SCREEN, None);
    assert_eq!(cards[1].offset, anchored_offset("cc", (25.0, 25.0), size, SCREEN));
}

/// `Widget.vue`'s `@mousedown`: a press on a lower card raises it (`wm.bringToTop`) and the new
/// `wmZorder` is saved, so the paint order survives a restart (`OverlayWindow.vue`'s
/// `visibilityState`: the config order for `auto` cards, then the numbered ones).
#[test]
fn clicking_a_lower_widget_raises_it_and_saves_the_order() {
    let dir = live_dir("raise");
    let path = crate::settings::config_path_in(GameId::Poe1, &dir);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    // A config that opens with the image strip shown: `wm.show` raises a widget, so a card that
    // has to be *clicked* to come to the front starts shown in the file, as `ConfigStore` has it.
    let mut config = crate::settings::default_config(GameId::Poe1);
    config["widgets"]
        .as_array_mut()
        .expect("widgets")
        .iter_mut()
        .find(|widget| widget["wmType"] == "image-strip")
        .expect("the image strip")["wmWants"] = serde_json::json!("show");
    std::fs::write(&path, config.to_string()).unwrap();

    let mut app = live_app_in(&dir, TestClipboard::default());
    poll(&mut app);
    let menu = widget_id(&app, widgets::menu::WM_TYPE);
    let strip = widget_id(&app, widgets::Kind::ImageStrip.wm_type());
    assert_eq!(card_ids(&app), vec![Some(strip), Some(menu)], "the `auto` card paints first");

    let mut runtime = overlay_runtime(app);
    let (x, y) = runtime.find("image-strip").expect("the strip's card");
    runtime.pointer_move(x, y);
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
    let mut app = runtime.state;
    assert_eq!(app.widgets.as_ref().unwrap().widget(strip).unwrap().zorder(), Some(2.0));
    assert_eq!(card_ids(&app), vec![Some(menu), Some(strip)], "the pressed card paints on top");
    poll(&mut app);

    let saved = saved_config(&dir);
    assert_eq!(saved_widget(&saved, strip)["wmZorder"], 2.0);

    let mut restarted = live_app_in(&dir, TestClipboard::default());
    poll(&mut restarted);
    assert_eq!(card_ids(&restarted), vec![Some(menu), Some(strip)], "the order survived the restart");
}

/// The guard: while `App::with_live` runs, `App::live` is empty, so an `App` method answering
/// from it would answer as a scene does. Asking anyway is a test failure, not a silent
/// fallback — that is how the class stayed hidden through three Windows runs.
#[test]
#[should_panic(expected = "the shell is out of `App::live` while `Live::poll` runs")]
fn asking_the_app_about_the_shell_from_inside_poll_panics() {
    let mut app = live_app("guard", TestClipboard::default());
    app.with_live(|app, _live| app.shown());
}

#[test]
fn reopening_an_item_reuses_the_trade_policy_and_cached_rows() {
    let text = crate::corpus("adv-rare-ring-mirrored-catalyst");
    let mut app = App::with_mode("", Mode::Replay);
    app.open_price_check_for(PriceCheckRequest {
        game: GameId::Poe1,
        league: "Standard",
        mode: Mode::Replay,
        text: &text,
        warm: None,
        overviews: None,
    });
    app.price_check.start_search();
    assert_eq!(app.price_check.trade.rows.len(), 20);
    let first = std::sync::Arc::clone(&app.price_check.trade.client);
    let policy = {
        let mut client = crate::trade::lock(&first);
        client.transport =
            crate::trade::TargetTransport::Replay(crate::replay::ReplayTransport::new("missing-recordings"));
        client.limits.search.clone()
    };
    app.open_price_check_for(PriceCheckRequest {
        game: GameId::Poe1,
        league: "Standard",
        mode: Mode::Replay,
        text: &text,
        warm: None,
        overviews: None,
    });
    assert!(std::sync::Arc::ptr_eq(&first, &app.price_check.trade.client));
    assert_eq!(crate::trade::lock(&app.price_check.trade.client).limits.search, policy);
    app.price_check.start_search();
    assert_eq!(app.price_check.trade.rows.len(), 20);
    assert!(app.price_check.trade.error.is_none());
}
