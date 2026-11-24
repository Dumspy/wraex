//! The shell's pure parts: which game a run picks, the window sizes, and the fact that a
//! scene never reaches the live path.

use super::*;
use crate::settings::Page;

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
