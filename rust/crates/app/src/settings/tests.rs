//! The settings window's headless tests: every scene renders its page, a control changes the
//! working copy, the recorder writes upstream's hotkey string, and Save round-trips through
//! `config::load`.

use rune::text::TextSystem;
use rune::{Env, Runtime};

use super::*;
use crate::App;
use crate::theme;

fn runtime(app: App) -> Runtime<App> {
    let text = TextSystem::new(theme::FONTS, theme::FAMILIES);
    let mut runtime = Runtime::with_text(app, crate::view, (WIDTH, HEIGHT), Env::new(true, 1.0), text);
    runtime.frame(0.0);
    runtime
}

fn scene(game: GameId, page: Page) -> Runtime<App> {
    runtime(App::settings_scene(game, page))
}

/// The selected page alone, in a tall window: the settings window's 38 rem height scrolls, and
/// a headless pointer reaches only what is on screen, so interaction tests draw the page
/// without the frame.
fn page_runtime(app: App) -> Runtime<App> {
    fn page_view(app: &App, cx: &mut rune::Cx<App>) -> rune::El {
        fn project(app: &mut App) -> &mut Settings {
            app.settings.as_mut().expect("the settings window")
        }
        let settings = app.settings.as_ref().expect("the settings window");
        cx.foreground(theme::gray(100));
        rune::col()
            .key("test-page")
            .w(WIDTH)
            .h_full()
            .fill(theme::gray(800))
            .scroll(rune::ScrollSpec { stick_bottom: false })
            .child(pages::view(settings, project))
    }
    let text = TextSystem::new(theme::FONTS, theme::FAMILIES);
    let mut runtime = Runtime::with_text(app, page_view, (WIDTH, 2000.0), Env::new(true, 1.0), text);
    runtime.frame(0.0);
    runtime
}

fn texts(runtime: &Runtime<App>) -> Vec<String> {
    runtime.elements().into_iter().filter_map(|element| element.text).collect()
}

fn has_text(runtime: &Runtime<App>, text: &str) -> bool {
    texts(runtime).iter().any(|found| found.contains(text))
}

fn text_of(runtime: &Runtime<App>, key: &str) -> Option<String> {
    runtime.elements().into_iter().find(|element| element.key.as_deref() == Some(key))?.text.clone()
}

fn click(runtime: &mut Runtime<App>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("no element keyed `{key}`"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

/// A key release, with the physical code the native host would send alongside it.
fn release(runtime: &mut Runtime<App>, key: rune::KeyEvent) {
    runtime.key_up(key);
    runtime.frame(1.0 / 60.0);
}

/// Every page of both games renders its main labels.
#[test]
fn every_settings_page_shows_its_labels() {
    for game in [GameId::Poe1, GameId::Poe2] {
        // The menu is on every page.
        for page in Page::ALL {
            let runtime = scene(game, page);
            let menu = runtime
                .elements()
                .into_iter()
                .filter(|element| {
                    element
                        .key
                        .as_deref()
                        .is_some_and(|key| key.starts_with("settings-menu-") && key.ends_with("-label"))
                })
                .count();
            assert_eq!(menu, Page::ALL.len(), "{game:?}/{page:?}: menu entries");
            assert!(runtime.find("titlebar-title").is_some(), "{game:?}/{page:?}: title bar");
            assert!(runtime.find("settings-save").is_some(), "{game:?}/{page:?}: Save");
            assert!(runtime.find("settings-cancel").is_some(), "{game:?}/{page:?}: Cancel");
        }

        let general = scene(game, Page::General);
        for label in ["Language", "Font size", "Restore clipboard", "PoE log file", "PoE config file"] {
            assert!(has_text(&general, label), "{game:?}: general: {label}");
        }
        assert!(has_text(&general, "PoE window title"), "{game:?}: general: window title");
        assert!(general.find("general-language-select").is_some(), "{game:?}: language select");
        assert!(general.find("general-font-size-input").is_some(), "{game:?}: font size field");

        let hotkeys = scene(game, Page::Hotkeys);
        assert!(has_text(&hotkeys, "You can clear hotkey"), "{game:?}: hotkeys banner");
        assert!(has_text(&hotkeys, "Auto-hide Mode"), "{game:?}: price check hotkey row");
        assert!(hotkeys.find("hotkeys-overlay-input").is_some(), "{game:?}: overlay key field");
        assert!(text_of(&hotkeys, "hotkeys-price-check-ctrl-label").as_deref() == Some("Ctrl"));
        assert!(has_text(&hotkeys, "Stash tab scrolling"), "{game:?}: stash scroll");

        let chat = scene(game, Page::Chat);
        assert!(has_text(&chat, "press Enter"), "{game:?}: chat send toggle");
        assert!(chat.find("chat-add").is_some(), "{game:?}: Add command");
        assert!(chat.find("chat-command-0").is_some(), "{game:?}: first command");

        let price_check = scene(game, Page::PriceCheck);
        for label in ["League", "Account name", "Show seller", "Fill stat values"] {
            assert!(has_text(&price_check, label), "{game:?}: price check: {label}");
        }

        let maps = scene(game, Page::Maps);
        assert!(has_text(&maps, "Only selected"), "{game:?}: maps only-selected");
        assert!(has_text(&maps, "Stat (found: "), "{game:?}: maps stat column");
        assert!(maps.find("maps-0").is_some(), "{game:?}: maps rows");

        let item_check = scene(game, Page::ItemCheck);
        assert!(has_text(&item_check, "Open item on wiki"), "{game:?}: wiki hotkey");
        assert!(has_text(&item_check, "Find in Stash"), "{game:?}: stash hotkey");
        assert!(item_check.find("item-check-stash-search-input").is_some(), "{game:?}: stash field");

        let debug = scene(game, Page::Debug);
        assert!(has_text(&debug, "Record all key presses"), "{game:?}: debug toggle");

        let about = scene(game, Page::About);
        assert!(has_text(&about, "Wraex"), "{game:?}: about name");
        assert!(has_text(&about, "Version "), "{game:?}: about version");
        assert!(has_text(&about, "Check now"), "{game:?}: about update action");
    }
}

/// A checkbox writes the working copy (`configModelValue`'s setter), not the saved config.
#[test]
fn toggling_a_setting_changes_the_working_copy() {
    let mut runtime = scene(GameId::Poe1, Page::General);
    let settings = runtime.state.settings.as_ref().expect("the settings window");
    assert!(!settings.bool(Field::Root("restoreClipboard")));
    assert!(!settings.saved["restoreClipboard"].as_bool().unwrap());

    click(&mut runtime, "general-restore-clipboard");
    let settings = runtime.state.settings.as_ref().expect("the settings window");
    assert!(settings.bool(Field::Root("restoreClipboard")));
    assert!(!settings.saved["restoreClipboard"].as_bool().unwrap(), "not saved yet");

    click(&mut runtime, "general-restore-clipboard");
    assert!(!runtime.state.settings.as_ref().unwrap().bool(Field::Root("restoreClipboard")));
}

/// A radio writes the field it stands for, and a text field its trimmed text.
#[test]
fn radios_and_fields_write_the_config() {
    let mut runtime = scene(GameId::Poe1, Page::General);
    // No overlay background colour or Transparent radio (Wraex draws no dim); the
    // background-click checkbox is always there.
    assert!(runtime.find("general-overlay-background-input").is_none());
    assert!(runtime.find("general-overlay-background-none").is_none());
    assert!(runtime.find("general-overlay-background-close").is_some());

    click(&mut runtime, "general-language-select");
    assert_eq!(runtime.state.settings.as_ref().unwrap().select_open, Some("general-language-select"));
    assert!(runtime.find("general-language-select-list").is_some());
    let (ox, oy) = (240.0f32, 130.0f32);
    for e in runtime.elements() {
        let [x, y, w, h] = e.rect;
        if x <= ox && ox <= x + w && y <= oy && oy <= y + h {
            eprintln!("contains: {:?} {:?} role={}", e.key, e.rect, e.role);
        }
    }
    click(&mut runtime, "general-language-select-option-ru");
    let settings = runtime.state.settings.as_ref().unwrap();
    assert_eq!(settings.string(Field::Root("language")), "ru");
    assert!(settings.select_open.is_none(), "picking closes the list");
    // The international-site box appears for a non-English client on the global realm.
    assert!(runtime.find("general-use-intl").is_some());
}

/// `HotkeyInput.vue`: click the field, press Ctrl+Alt+D, and upstream's string lands in the
/// config — `hotkeyToString`'s "Ctrl + Alt + D".
#[test]
fn recording_a_hotkey_produces_upstreams_string() {
    let mut runtime = scene(GameId::Poe1, Page::Hotkeys);
    assert_eq!(runtime.state.settings.as_ref().unwrap().string(Field::Root("overlayKey")), "Shift + Space");

    click(&mut runtime, "hotkeys-overlay-input");
    assert!(runtime.state.settings.as_ref().unwrap().recording.is_some(), "the field is armed");
    release(
        &mut runtime,
        rune::KeyEvent {
            key: rune::Key::Text("D".to_owned()),
            mods: rune::Mods { ctrl: true, alt: true, ..rune::Mods::default() },
        },
    );
    let settings = runtime.state.settings.as_ref().unwrap();
    assert_eq!(settings.string(Field::Root("overlayKey")), "Ctrl + Alt + D");
    assert!(settings.recording.is_none(), "one release ends the capture");
    assert_eq!(text_of(&runtime, "hotkeys-overlay-input-value").as_deref(), Some("Ctrl + Alt + D"));

    // Backspace clears a field that isn't required.
    click(&mut runtime, "hotkeys-delve-grid-input");
    release_named(&mut runtime, "Backspace");
    assert!(
        runtime.state.settings.as_ref().unwrap().config["widgets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["wmType"] == "delve-grid")
            .unwrap()["toggleKey"]
            .is_null()
    );
    // The Overlay key is required: Backspace leaves it alone (and keeps capturing).
    click(&mut runtime, "hotkeys-overlay-input");
    release_named(&mut runtime, "Backspace");
    assert_eq!(runtime.state.settings.as_ref().unwrap().string(Field::Root("overlayKey")), "Ctrl + Alt + D");

    // F12 is never a hotkey, as `if (code.includes('F12')) return` skips it.
    release(
        &mut runtime,
        rune::KeyEvent { key: rune::Key::Function(12), mods: rune::Mods { ctrl: true, ..rune::Mods::default() } },
    );
    assert_eq!(runtime.state.settings.as_ref().unwrap().string(Field::Root("overlayKey")), "Ctrl + Alt + D");
}

/// A release of one named key, with no modifiers.
fn release_named(runtime: &mut Runtime<App>, name: &str) {
    let key = match name {
        "Backspace" => rune::Key::Backspace,
        other => rune::Key::Text(other.to_owned()),
    };
    release(runtime, rune::KeyEvent { key, mods: rune::Mods::default() });
}

/// The split key's modifier buttons write `hotkeyHold`, and its input `noModKeys`.
#[test]
fn the_split_hotkey_takes_a_modifier_and_a_bare_key() {
    let mut runtime = scene(GameId::Poe1, Page::Hotkeys);
    click(&mut runtime, "hotkeys-price-check-alt");
    assert_eq!(runtime.state.settings.as_ref().unwrap().string(Field::Widget("price-check", "hotkeyHold")), "Alt");
    click(&mut runtime, "hotkeys-price-check-input");
    release(
        &mut runtime,
        rune::KeyEvent {
            key: rune::Key::Text("Q".to_owned()),
            mods: rune::Mods { alt: true, ..rune::Mods::default() },
        },
    );
    // `noModKeys`: a chord is refused, so the field keeps its old key.
    assert_eq!(runtime.state.settings.as_ref().unwrap().string(Field::Widget("price-check", "hotkey")), "D");
    click(&mut runtime, "hotkeys-price-check-input");
    release(&mut runtime, rune::KeyEvent { key: rune::Key::Text("Q".to_owned()), mods: rune::Mods::default() });
    assert_eq!(runtime.state.settings.as_ref().unwrap().string(Field::Widget("price-check", "hotkey")), "Q");
}

/// `settings.chat_cmd_add` and `Remove` change the command list, and the text fields follow.
#[test]
fn the_chat_commands_are_added_and_removed() {
    let mut runtime = scene(GameId::Poe1, Page::Chat);
    let count = runtime.state.settings.as_ref().unwrap().commands().len();
    assert_eq!(count, 6);

    click(&mut runtime, "chat-add");
    assert_eq!(runtime.state.settings.as_ref().unwrap().commands().len(), count + 1);
    assert!(runtime.find(&format!("chat-command-{count}")).is_some(), "the new row's field");

    click(&mut runtime, "chat-command-0-remove");
    assert_eq!(runtime.state.settings.as_ref().unwrap().commands().len(), count);
    assert_eq!(runtime.state.settings.as_ref().unwrap().commands()[0]["text"].as_str(), Some("/exit"));

    // A command's send toggle writes the command's `send`.
    click(&mut runtime, "chat-command-0-send");
    assert!(!runtime.state.settings.as_ref().unwrap().commands()[0]["send"].as_bool().unwrap());
}

/// `save()` writes the compact JSON `saveConfig` sends, and `config::load` reads it back
/// identically — a temp store, never the user's file.
#[test]
fn save_writes_json_that_load_reads_back() {
    let dir = std::env::temp_dir().join(format!("wraex-settings-save-{}", std::process::id()));
    let store = ConfigStore::new(dir.join("apt-data/config.json"));
    let mut settings =
        Settings::new(GameId::Poe1, Page::General, default_config(GameId::Poe1), store.clone(), Mode::Replay);
    settings.set_bool(Field::Root("restoreClipboard"), true);
    settings.set_f64(Field::Root("fontSize"), 18.0);
    settings.set_string(Field::Root("windowTitle"), "Path of Exile 2");
    settings.save().expect("the temp store is writable");
    assert!(!settings.open, "Save hides the window, as `wm.hide` does");

    let written = std::fs::read_to_string(store.path()).expect("the file Save wrote");
    assert!(!written.contains('\n'), "`JSON.stringify(AppConfig())` is compact");
    let loaded = load(GameId::Poe1, Some(&written));
    assert!(!loaded.save_temporary);
    assert_eq!(loaded.config, settings.saved, "the file reads back identically");
    assert_eq!(loaded.config["restoreClipboard"], serde_json::json!(true));
    assert_eq!(loaded.config["fontSize"], serde_json::json!(18.0));

    // Cancel throws a working copy away; the saved config stays.
    settings.reopen();
    settings.set_bool(Field::Root("restoreClipboard"), false);
    settings.cancel();
    settings.reopen();
    assert!(settings.bool(Field::Root("restoreClipboard")), "the saved value is back");

    std::fs::remove_dir_all(&dir).unwrap();
}

/// EE2's general page has its own boxes: the preferred trade site, the overlay-close
/// options, Client.txt reading and the experimental features.
#[test]
fn ee2s_general_page_shows_its_extra_settings() {
    let mut runtime = page_runtime(App::settings_scene(GameId::Poe2, Page::General));
    for label in [
        "Always fully close the overlay",
        "Fully hide overlay when it loses focus",
        "Allow reading of Client.txt file",
        "Enable experimental features",
    ] {
        assert!(has_text(&runtime, label), "poe2 general: {label}");
    }
    assert!(runtime.find("general-preferred-trade-site").is_none(), "English hides it");
    click(&mut runtime, "general-language-select");
    click(&mut runtime, "general-language-select-option-ja");
    assert!(runtime.find("general-preferred-trade-site-select").is_some(), "a non-English client shows it");
    assert!(has_text(&runtime, "jp.pathofexile.com"), "the language's trade site");

    click(&mut runtime, "general-enable-alphas");
    assert!(runtime.state.settings.as_ref().unwrap().bool(Field::Root("enableAlphas")));
    assert!(runtime.find("general-library-alpha").is_some());
    click(&mut runtime, "general-library-alpha");
    assert_eq!(runtime.state.settings.as_ref().unwrap().config["alphas"], serde_json::json!(["library"]));
}

/// The maps page's decisions: a row's radio writes the profile's letter, Remove clears it.
#[test]
fn a_map_stat_takes_a_decision() {
    let mut runtime = page_runtime(App::settings_scene(GameId::Poe1, Page::Maps));
    assert!(has_text(&runtime, "Pack Size"), "the area mods load from the game data");

    // The default config's first decided stat is a warning for profile 1.
    let selected = |runtime: &Runtime<App>, matcher: &str| -> Option<String> {
        runtime.state.settings.as_ref().unwrap().config["widgets"]
            .as_array()
            .unwrap()
            .iter()
            .find(|w| w["wmType"] == "item-check")
            .unwrap()["maps"]["selectedStats"]
            .as_array()
            .unwrap()
            .iter()
            .find(|stat| stat["matcher"] == matcher)
            .map(|stat| stat["decision"].as_str().unwrap().to_owned())
    };
    let matcher = "Players have #% to all maximum Resistances";
    assert_eq!(selected(&runtime, matcher).as_deref(), Some("w--"));

    // The row's "deadly" radio.
    let key = runtime
        .elements()
        .into_iter()
        .filter_map(|element| element.key)
        .find(|key| {
            key.strip_prefix("maps-")
                .and_then(|rest| rest.strip_suffix("-deadly"))
                .is_some_and(|index| !index.is_empty() && index.bytes().all(|b| b.is_ascii_digit()))
        })
        .expect("a row's deadly radio");
    click(&mut runtime, &key);
    let row = key.trim_end_matches("-deadly");
    let text = runtime
        .elements()
        .into_iter()
        .find(|element| element.key.as_deref() == Some(&format!("{row}-matcher")))
        .and_then(|element| element.text)
        .expect("the row's stat text");
    assert_eq!(selected(&runtime, &text).as_deref(), Some("d--"));
}

/// `findAllAreaMods()` reads every area modifier from the game data.
#[test]
fn the_area_mods_load_from_the_game_data() {
    for game in [GameId::Poe1, GameId::Poe2] {
        let mods = pages::maps::area_mods(game, Mode::Replay);
        eprintln!("{game:?}: {} area mods", mods.len());
        assert!(mods.len() > 100, "{game:?}: {} area mods", mods.len());
        assert!(mods.iter().any(|m| m.matcher.contains("Pack Size")), "{game:?}");
    }
}

/// Cancel hides the window and the game's price check shows again.
#[test]
fn cancel_returns_to_the_price_check() {
    let mut runtime = scene(GameId::Poe1, Page::General);
    click(&mut runtime, "settings-cancel");
    assert!(!runtime.state.settings.as_ref().unwrap().open);
    assert_eq!(text_of(&runtime, "titlebar-title").as_deref(), Some("Standard"));
    assert!(runtime.find("price-check").is_some(), "the price check is back");
    assert!(runtime.find("settings-window").is_none());
}

/// Save on the footer writes the working copy through the scene's temp store, and the file
/// `config::load` reads back holds the change.
#[test]
fn save_through_the_window_writes_the_working_copy() {
    let mut runtime = scene(GameId::Poe1, Page::General);
    click(&mut runtime, "general-restore-clipboard");
    click(&mut runtime, "settings-save");
    assert!(!runtime.state.settings.as_ref().unwrap().open, "Save hides the window");
    let store = temp_store(GameId::Poe1);
    let written = std::fs::read_to_string(store.path()).expect("the scene's temp store holds the save");
    let loaded = load(GameId::Poe1, Some(&written));
    assert_eq!(loaded.config["restoreClipboard"], serde_json::json!(true));
    std::fs::remove_file(store.path()).ok();
}

/// `settings-patrons`: upstream's sidebar is as wide as its "Support development on" block;
/// the port draws the thank-you box in its place, and it must stay inside the menu column
/// (the box used to size itself to the label and spill over the page).
#[test]
fn the_thank_you_box_fits_the_menu_column() {
    let runtime = scene(GameId::Poe1, Page::Hotkeys);
    let elements = runtime.elements();
    let find = |key: &str| {
        elements.iter().find(|element| element.key.as_deref() == Some(key)).unwrap_or_else(|| panic!("no `{key}`"))
    };
    let menu = find("settings-menu");
    let box_ = find("settings-thank-you");
    let label = find("settings-thank-you-label");
    assert!(box_.rect[0] >= menu.rect[0], "{:?} starts inside {:?}", box_.rect, menu.rect);
    assert!(
        box_.rect[0] + box_.rect[2] <= menu.rect[0] + menu.rect[2] + 0.5,
        "the box ends at {} but the menu at {}",
        box_.rect[0] + box_.rect[2],
        menu.rect[0] + menu.rect[2]
    );
    // The label wraps inside the box rather than drawing past it.
    assert!(label.rect[2] <= box_.rect[2] + 0.5, "{:?} vs {:?}", label.rect, box_.rect);
    assert!(label.rect[1] + label.rect[3] <= box_.rect[1] + box_.rect[3] + 0.5);
    assert!(label.rect[3] > theme::text::SM.line_px(), "the long label wraps onto a second line");
}

/// The about page's "Check now" hands the request to a worker: the press shows upstream's
/// checking state at once (no blocking HTTPS call), the page draws it, and the answer lands
/// through [`Settings::poll`].
#[test]
fn check_now_runs_on_a_worker() {
    let mut app = App::settings_scene(GameId::Poe1, Page::About);
    let settings = app.settings.as_mut().expect("the settings window");
    assert_eq!(settings.update, UpdateInfo::Initial);

    settings.begin_update_check();
    assert_eq!(settings.update, UpdateInfo::CheckingForUpdate, "the press returns before the check");

    // The page shows what `about.vue` shows while checking.
    let runtime = page_runtime(app);
    assert!(has_text(&runtime, "Checking for updates"), "{:?}", texts(&runtime));
    assert!(has_text(&runtime, "Please wait"), "{:?}", texts(&runtime));

    // The answer is in the `Latest`, not in the state: only `poll` applies it. A replay has
    // no recorded answer for api.github.com, so the offline error is the expected result.
    let mut app = runtime.state;
    let settings = app.settings.as_mut().expect("the settings window");
    let mut polled = false;
    for _ in 0..400 {
        if settings.poll() {
            polled = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(polled, "the worker's answer lands in poll");
    assert!(matches!(settings.update, UpdateInfo::Error { .. }), "{:?}", settings.update);
}
