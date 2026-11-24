//! The shell's pure parts: which game a run picks, the window sizes, the overlay's mouse
//! wiring with fakes, and the fact that a scene never reaches the live path.

use super::*;
use crate::settings::Page;
use wraex_platform::overlay::{OverlayEvent, OverlayHandle, OverlayInput};

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
        let size = overlay_widget_size(&Shown::PriceCheck, BOUNDS, scale).expect("the price check has a size");
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
    assert_eq!(window_size(&Shown::None), None);
    assert_eq!(window_size(&Shown::Settings), Some((crate::settings::WIDTH, crate::settings::HEIGHT)));
    assert_eq!(window_size(&Shown::PriceCheck), Some((crate::poe1::price_check::WIDTH, HEIGHT)));
    assert_eq!(window_size(&Shown::Poe2), Some((crate::poe2::price_check::WIDTH, HEIGHT)));
    assert_eq!(
        window_size(&Shown::Widget(widgets::Kind::Menu.name().to_owned())),
        Some(widgets::scene_size(GameId::Poe1, widgets::Kind::Menu))
    );
    // A widget the port does not draw keeps the price check's size rather than failing.
    assert_eq!(window_size(&Shown::Widget("unknown".to_owned())), None);
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
    let size = overlay_widget_size(&Shown::PriceCheck, bounds, 2.0).expect("a size");
    assert_eq!(size, (crate::poe1::price_check::WIDTH, 600.0), "the price check is h-full");
    assert_eq!(
        overlay_widget_size(&Shown::Widget(widgets::Kind::Menu.name().to_owned()), bounds, 2.0),
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
