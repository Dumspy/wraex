//! The shell's pure parts: which game a run picks, the window sizes, the overlay's mouse
//! wiring with fakes, and the fact that a scene never reaches the live path.

use super::*;
use crate::settings::Page;
use wraex_platform::overlay::{OverlayEvent, OverlayHandle, OverlayInput};
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
        let size =
            overlay_widget_size(GameId::Poe1, &Shown::PriceCheck, BOUNDS, scale).expect("the price check has a size");
        let offset = widget_offset(Some(Placement::Cursor(cursor)), size, BOUNDS, scale);
        let area = track_area(offset, size, BOUNDS, scale, cursor, price_check_hold_key(&config()));
        self.overlay.on_track_area(area);
    }

    /// One of `Live::poll_mouse`'s events.
    fn mouse(&mut self, event: MouseEvent) {
        match event {
            MouseEvent::Move { point, ctrl, alt } => {
                self.overlay.on_mouse_move(point, ctrl, alt, &mut self.window, &mut self.handle);
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
    assert!(!hides_window(&shown, true, true), "the window stays on screen over the game");
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
    anchored_offset("tl", (5.0, 5.0), widgets::scene_size(GameId::Poe1, widgets::Kind::Menu), BOUNDS, 1.0)
}

/// `OverlayWindow.vue` draws every widget whose `wmWants` is `show`, each at its own anchor
/// (`Widget.vue`). The second Windows-run bug: clicking "Map rolling" in the menu replaced the
/// menu with the stash search, so the settings cog went with it; both must be on screen at once.
#[test]
fn the_overlay_draws_every_shown_widget_at_its_own_anchor() {
    let mut app = overlay_app();
    let menu = widget_id(&app, widgets::menu::WM_TYPE);
    let cards = app.cards_for(true, BOUNDS, 1.0, None);
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
    let cards = app.cards_for(true, BOUNDS, 1.0, None);
    assert_eq!(cards.len(), 2, "the menu and the stash search");
    assert_eq!(cards[0].wm_id, Some(stash), "the `auto` card paints first");
    assert_eq!(cards[1].wm_id, Some(menu), "the menu paints above it");
    let size = widgets::scene_size(GameId::Poe1, widgets::Kind::StashSearch);
    assert_eq!(cards[0].offset, anchored_offset("tl", (35.0, 46.0), size, BOUNDS, 1.0));
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
    let cards = app.cards_for(true, BOUNDS, 1.0, Some(Placement::Centred));
    assert_eq!(cards.len(), 1, "the exclusive settings window hides the menu");
    assert_eq!(cards[0].shown, Shown::Settings);
    assert_eq!(cards[0].offset, (0.0, 0.0));
    assert_eq!(cards[0].width, Some(BOUNDS.width as f32));
    assert_eq!(cards[0].height, Some(BOUNDS.height as f32));

    // Save: the working copy is written and the window closes, as `Live::poll` runs after it.
    app.settings.as_mut().expect("the settings window").save().expect("the scene's temp store");
    app.hide_settings_widget();
    let cards = app.cards_for(true, BOUNDS, 1.0, Some(Placement::Centred));
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].shown, Shown::Widget(widgets::menu::WM_TYPE.to_owned()));
    assert_eq!(cards[0].offset, menu_offset(), "the menu is at 5 %/5 %, not where the settings was");

    // Cancel is the same close (`Settings::cancel`), and the same menu.
    app.settings = Some(crate::settings::Settings::scene(GameId::Poe1, Page::General));
    app.settings.as_mut().expect("the settings window").cancel();
    app.hide_settings_widget();
    let cards = app.cards_for(true, BOUNDS, 1.0, Some(Placement::Centred));
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
    let cards = app.cards_for(true, BOUNDS, 1.0, None);
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
    let cards = app.cards_for(true, BOUNDS, 1.0, Some(Placement::Cursor(cursor)));
    assert_eq!(cards.len(), 1, "an exclusive widget hides the rest");
    assert_eq!(cards[0].shown, Shown::PriceCheck);
    let size = (crate::poe1::price_check::WIDTH, BOUNDS.height as f32);
    assert_eq!(cards[0].size(), Some(size), "`#price-window` is `h-full`");
    assert_eq!(cards[0].offset, widget_offset(Some(Placement::Cursor(cursor)), size, BOUNDS, 1.0));
    assert_eq!(app.shown_in(true), Shown::PriceCheck);
}

/// `visibilityState`'s `invisible-on-blur`: the menu goes with the overlay, while a widget whose
/// flag was cleared (a running stopwatch) stays drawn over the game.
#[test]
fn hidden_widgets_are_not_drawn() {
    let mut app = overlay_app();
    assert!(app.cards_for(false, BOUNDS, 1.0, None).is_empty(), "the menu is `invisible-on-blur`");

    let timer = app.widgets.as_mut().expect("the overlay").create(widgets::Kind::Stopwatch.wm_type());
    app.widgets.as_mut().expect("the overlay").timer_start(timer, 0.0);
    let cards = app.cards_for(false, BOUNDS, 1.0, None);
    assert_eq!(cards.len(), 1, "the running stopwatch is the only card off-overlay");
    assert_eq!(cards[0].wm_id, Some(timer));
    // The timer's own anchor is the centre (`cc`), which its card is shifted by.
    let size = widgets::scene_size(GameId::Poe1, widgets::Kind::Stopwatch);
    assert_eq!(cards[0].offset, anchored_offset("cc", (50.0, 50.0), size, BOUNDS, 1.0));
    assert_eq!(app.shown_in(false), Shown::Widget(widgets::Kind::Stopwatch.wm_type().to_owned()));
}

/// The settings window as the overlay draws it: `SettingsWindow.vue` is a widget whose root is
/// `position: absolute; inset: 0`, with its 50 × 38 rem `.window` centred horizontally at the
/// top. Its box takes the clicks — below the window too — so a click anywhere inside it never
/// reaches the overlay's background layer (the bug where the settings closed on its first click).
#[test]
fn the_settings_window_covers_the_overlay_and_takes_its_clicks() {
    let mut app = overlay_app();
    app.settings = Some(crate::settings::Settings::scene(GameId::Poe1, Page::Hotkeys));
    let size = (BOUNDS.width as f32, BOUNDS.height as f32);
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
        matches!(picked.as_deref(), Some("settings") | Some("card-settings")),
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
    let cards = app.cards_for(true, BOUNDS, 1.0, None);
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].wm_id, Some(menu));
    assert_eq!(cards[0].offset, menu_offset());

    // "Map rolling": the menu's button is `wm.toggle`, which shows the stash search.
    app.widgets.as_mut().expect("the overlay").toggle(stash);
    let cards = app.cards_for(true, BOUNDS, 1.0, None);
    assert_eq!(cards.len(), 2, "both cards: the image strip must not hide the menu");
    assert!(cards.iter().any(|card| card.wm_id == Some(menu) && card.offset == menu_offset()));
    assert!(cards.iter().any(|card| card.wm_id == Some(stash)));

    // The cog: `wm.show(settings)` and the settings window opens. It is exclusive, so it is the
    // only card, covering the overlay.
    app.widgets.as_mut().expect("the overlay").toggle(settings);
    app.settings = Some(crate::settings::Settings::scene(GameId::Poe1, Page::Hotkeys));
    let cards = app.cards_for(true, BOUNDS, 1.0, Some(Placement::Centred));
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].shown, Shown::Settings);

    // Save: the settings widget goes back to `hide`, and the menu is where it belongs.
    app.settings.as_mut().expect("the settings window").save().expect("the scene's temp store");
    app.hide_settings_widget();
    let cards = app.cards_for(true, BOUNDS, 1.0, Some(Placement::Centred));
    let menu_card = cards.iter().find(|card| card.wm_id == Some(menu)).expect("the menu is back");
    assert_eq!(menu_card.offset, menu_offset(), "the menu, not the settings window's placement");
    assert!(cards.iter().any(|card| card.wm_id == Some(stash)), "the stash search stayed up");

    // The background click: the game gets the focus back, and the overlay draws nothing.
    apply_overlay_event(&OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }, &mut app);
    assert!(
        app.cards_for(false, BOUNDS, 1.0, None).is_empty(),
        "the menu and the stash search are `invisible-on-blur`"
    );
    assert_eq!(app.shown_in(false), Shown::None);
}

/// The overlay as [`crate::view`] draws it over a game: every visible widget at its own anchor,
/// in one window (`OverlayWindow.vue`'s `visibilityState`). The live shell places the same cards
/// (`App::cards_for`); this renders them from the app's own state, as a `--snap` of a scene does.
fn game_overlay(app: &App, cx: &mut rune::Cx<App>) -> El {
    let cards = app.cards_for(true, BOUNDS, 1.0, None);
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

    let size = (BOUNDS.width as f32, BOUNDS.height as f32);
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

    apply_overlay_event(&OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }, &mut app);

    let shown = app.shown_in(false);
    assert_eq!(shown, Shown::None);
    assert!(hides_window(&shown, true, true), "nothing to draw, so the window hides even with the game in front");
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

    apply_overlay_event(&OverlayEvent::FocusChange { game: true, overlay: false, using_hotkey: false }, &mut app);
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
    assert_eq!(anchored_offset("tl", (5.0, 5.0), size, bounds, 1.0), (80.0, 60.0));
    // `tc` centres the card on its x, `br` puts its bottom-right corner on the anchor.
    assert_eq!(anchored_offset("tc", (50.0, 10.0), (200.0, 100.0), bounds, 1.0), (700.0, 120.0));
    assert_eq!(anchored_offset("br", (100.0, 100.0), (200.0, 100.0), bounds, 1.0), (1400.0, 1100.0));
    // A 150 % window: the anchor is a percentage of the game in points, not physical pixels.
    assert_eq!(
        anchored_offset("tl", (5.0, 5.0), size, bounds, 1.5),
        ((1600.0 / 1.5 * 0.05) as f32, (1200.0 / 1.5 * 0.05) as f32)
    );
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
    assert_eq!(widget_offset(Some(Placement::Cursor(Point { x: 300.0, y: 250.0 })), size, bounds, 1.0), (212.0, 212.0));
    // At 150 % the game is 1600 physical px wide but the widget is laid out in points.
    assert_eq!(widget_offset(Some(Placement::Cursor(Point { x: 400.0, y: 350.0 })), size, bounds, 1.5), (212.0, 212.0));
    // A cursor near the bottom right corner: the card is pulled back inside the game.
    assert_eq!(
        widget_offset(Some(Placement::Cursor(Point { x: 1690.0, y: 1240.0 })), size, bounds, 1.0),
        (1200.0, 900.0)
    );
    // Above and left of the game: still inside it.
    assert_eq!(widget_offset(Some(Placement::Cursor(Point { x: 0.0, y: 0.0 })), size, bounds, 1.0), (0.0, 0.0));
    // Centred for a window the tray opened.
    assert_eq!(widget_offset(Some(Placement::Centred), size, bounds, 1.0), (600.0, 450.0));
    assert_eq!(widget_offset(None, size, bounds, 1.0), (600.0, 450.0));
    // A card taller than the game starts at the top rather than at a negative offset.
    assert_eq!(widget_offset(None, (400.0, 1500.0), bounds, 1.0), (600.0, 0.0));
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
    let _ = std::fs::remove_dir_all(&dir);
    let startup = config::load(GameId::Poe1, &dir);
    // `Live` needs a Rune handle, which a unit test has no app for: the pieces the idle state
    // depends on are the config and the shown window, and `App::live` is exercised by the run.
    assert_eq!(startup.source, config::Source::Defaults);
    assert!(startup.folder().is_some());
    let _ = std::fs::remove_dir_all(&dir);
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
    assert!(host.shortcuts.is_empty());
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
    app.open_price_check_for(GameId::Poe1, "Standard", Mode::Replay, &text, None, None);
    assert_eq!(app.shown(), Shown::PriceCheck);
    assert!(app.price_check.open);
    assert!(app.price_check.item.is_ok(), "the item parsed");
    assert_eq!(app.price_check.league, "Standard", "the league the trade site lists");
    assert_eq!(app.game, GameId::Poe1);
    // The replay's prices are the app's, so the title bar shows its rate.
    assert!(app.price_check.stable_orb_cost().is_some());

    // A league the recordings do not cover still opens the window, in that league.
    app.close_windows();
    app.open_price_check_for(GameId::Poe1, "Covens", Mode::Replay, &text, None, None);
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
    app.open_price_check_for(
        GameId::Poe1,
        "Standard",
        Mode::Replay,
        &crate::corpus("currency-divine-orb"),
        Some(warm),
        None,
    );
    assert_eq!(app.price_check.stable_orb_cost(), Some(891.0));
}

/// The item-check hotkey opens its widget, and the widget's own hotkeys press their button.
#[test]
fn the_item_check_hotkey_opens_its_widget_and_page() {
    let text = crate::corpus("adv-rare-ring-mirrored-catalyst");
    let config = crate::settings::default_config(GameId::Poe1);
    let mut app = App::replay("currency-divine-orb");
    app.close_windows();

    app.open_item_check_for(GameId::Poe1, &config, Mode::Replay, &text, None);
    assert_eq!(app.shown(), Shown::ItemCheck);
    assert!(app.item_check.as_ref().is_some_and(|check| check.open));
    assert!(app.item_check.as_ref().unwrap().actions.is_empty(), "a plain item is just shown");

    app.open_item_check_for(GameId::Poe1, &config, Mode::Replay, &text, Some(ItemAction::Wiki));
    let actions = &app.item_check.as_ref().unwrap().actions;
    assert!(matches!(&actions[0], crate::poe1::item_check::Action::Open(url) if url.contains("poewiki.net")));

    // A text that is not an item opens nothing.
    let mut app = App::replay("currency-divine-orb");
    app.close_windows();
    app.open_item_check_for(GameId::Poe1, &config, Mode::Replay, "not an item", None);
    assert_eq!(app.shown(), Shown::None);
}

/// The PoE2 window, with the league the PoE2 trade site lists.
#[test]
fn the_poe2_hotkey_opens_its_own_window() {
    let text = crate::corpus_poe2("currency-djinn-barya");
    let mut app = App::replay("currency-divine-orb");
    app.close_windows();
    app.open_price_check_for(GameId::Poe2, "Dawn of the Hunt", Mode::Replay, &text, None, None);
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
    apply_overlay_event(&OverlayEvent::FocusChange { game: false, overlay: true, using_hotkey: false }, &mut app);
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
    apply_overlay_event(&events[0], &mut app);
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
    apply_overlay_event(&events[0], &mut app);
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
    apply_overlay_event(&events[0], &mut app);
    assert_eq!(app.shown(), Shown::None);
}

/// The tracker area is what upstream's renderer reports: the card in screen DIPs, the press
/// position, the hold key and the DPI scale for Win32's conversion.
#[test]
fn the_track_area_is_the_card_in_screen_points() {
    let bounds = Bounds { x: 100.0, y: 50.0, width: 1600.0, height: 1200.0 };
    let size = overlay_widget_size(GameId::Poe1, &Shown::PriceCheck, bounds, 2.0).expect("a size");
    assert_eq!(size, (crate::poe1::price_check::WIDTH, 600.0), "the price check is h-full");
    assert_eq!(
        overlay_widget_size(GameId::Poe1, &Shown::Widget(widgets::Kind::Menu.name().to_owned()), bounds, 2.0),
        Some(widgets::scene_size(GameId::Poe1, widgets::Kind::Menu)),
        "other cards keep their own size"
    );

    let offset = (10.0, 20.0);
    let area = track_area(offset, size, bounds, 2.0, Point { x: 300.0, y: 250.0 }, Some("Ctrl".to_owned()));
    assert_eq!(area.hold_key.as_deref(), Some("Ctrl"));
    assert_eq!(area.close_threshold, 2.5 * crate::theme::REM as f64, "2.5 rem in points");
    assert_eq!(area.from, Point { x: 150.0, y: 125.0 }, "physical pixels to DIPs");
    assert_eq!(area.area, Bounds { x: 60.0, y: 45.0, width: crate::poe1::price_check::WIDTH as f64, height: 600.0 });
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
        self.0.set(self.0.get() + millis);
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
    assert!(controller.take_events().is_empty());
}
