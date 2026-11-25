//! The overlay widgets' behaviour: what the menu toggles, what the stopwatch counts, what the
//! image strip draws and what the delve grid lays out.
//!
//! Each scene is also in `crate::SCENES` (`widget-menu`, `poe2-widget-stopwatch`, …), so
//! `cargo run -p wraex-app -- --snap out.png --scene widget-menu` renders the same window; these
//! tests drive the handlers and read the element tree.

use rune::runtime::ElementInfo;
use rune::text::TextSystem;
use rune::{Env, Runtime};
use wraex_core::GameId;

use crate::widgets::item_search::Tab;
use crate::widgets::{self, Action, Kind, Wants};
use crate::{App, theme, view};

fn runtime(kind: Kind) -> Runtime<App> {
    runtime_game(GameId::Poe1, kind)
}

fn runtime_game(game: GameId, kind: Kind) -> Runtime<App> {
    runtime_sized(game, kind, widgets::scene_size(game, kind))
}

/// A scene in a window of `size`, for a state whose card is wider than the default's.
fn runtime_sized(game: GameId, kind: Kind, size: (f32, f32)) -> Runtime<App> {
    let text = TextSystem::new(theme::FONTS, theme::FAMILIES);
    let mut runtime = Runtime::with_text(App::widgets_scene(game, kind), view, size, Env::new(true, 1.0), text);
    runtime.poll = Some(|app: &mut App, clock: f64, _tx: &mut rune::Tx| app.poll(clock));
    runtime.frame(0.0);
    runtime
}

fn element<'a>(elements: &'a [ElementInfo], key: &str) -> Option<&'a ElementInfo> {
    elements.iter().find(|element| element.key.as_deref() == Some(key))
}

fn text_of(runtime: &Runtime<App>, key: &str) -> Option<String> {
    let elements = runtime.elements();
    element(&elements, key)?.text.clone()
}

fn click(runtime: &mut Runtime<App>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("no element keyed `{key}`"));
    runtime.pointer_down(x, y);
    runtime.pointer_up(x, y);
    runtime.frame(1.0 / 60.0);
}

/// A scene with room to the right of its card, where `Widget.vue`'s actions panel hangs. A
/// scene's window is the card itself (that is what the screenshots crop), so a test that presses
/// the panel's buttons needs a window that also holds the panel.
fn runtime_panel(kind: Kind) -> Runtime<App> {
    let (width, height) = widgets::scene_size(GameId::Poe1, kind);
    runtime_sized(GameId::Poe1, kind, (width + 200.0, height))
}

/// Move the pointer onto an element, which runs `on_hover_change` on the next frame.
fn hover(runtime: &mut Runtime<App>, key: &str) {
    let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("no element keyed `{key}`"));
    runtime.pointer_move(x, y);
    runtime.frame(1.0 / 60.0);
}

fn overlay(runtime: &Runtime<App>) -> &widgets::Overlay {
    runtime.state.widgets.as_ref().expect("a widget scene")
}

fn overlay_mut(runtime: &mut Runtime<App>) -> &mut widgets::Overlay {
    runtime.state.widgets.as_mut().expect("a widget scene")
}

/// The actions the widgets asked the shell for, emptied.
fn actions(runtime: &mut Runtime<App>) -> Vec<Action> {
    overlay_mut(runtime).take_actions()
}

/// The four chrome widgets this module ports (`Kind::ALL` also holds the item and stash searches
/// and EE2's XP tracker, which have their own tests).
const PORTED: [Kind; 4] = [Kind::Menu, Kind::Stopwatch, Kind::ImageStrip, Kind::DelveGrid];

/// Every widget scene renders its card on both games, at the size its screenshot crops.
#[test]
fn every_widget_scene_draws_its_card() {
    for game in [GameId::Poe1, GameId::Poe2] {
        for kind in PORTED {
            let runtime = runtime_game(game, kind);
            let (width, height) = widgets::scene_size(game, kind);
            let key = match kind {
                Kind::Menu => "widget-menu",
                Kind::Stopwatch => "stopwatch-card",
                Kind::ImageStrip => "image-strip",
                Kind::DelveGrid => "delve-grid",
                Kind::ItemSearch => "item-search",
                Kind::StashSearch => "stash-search",
                Kind::XpTracker => "xp-tracker",
            };
            let elements = runtime.elements();
            let card = element(&elements, key).unwrap_or_else(|| panic!("{game:?}/{kind:?}: no `{key}`"));
            assert!(card.rect[2] > 0.0 && card.rect[3] > 0.0, "{game:?}/{kind:?}: {:?}", card.rect);
            // The window is the card: a card a little narrower than the reference leaves a gap
            // at the window's right edge rather than being stretched.
            assert!((card.rect[2] - width).abs() <= 8.0, "{game:?}/{kind:?}: {} vs {width}", card.rect[2]);
            assert!((card.rect[3] - height).abs() <= 8.0, "{game:?}/{kind:?}: {} vs {height}", card.rect[3]);
            assert!(runtime.find("titlebar").is_none(), "{game:?}/{kind:?}: no title bar");
        }
    }
}

/// `WidgetMenu.vue`: a button per hidden widget (`menu::skip` ones left out), the cog among
/// them, and pressing one shows the widget through an [`Action`].
#[test]
fn the_menu_lists_the_hidden_widgets() {
    let runtime = runtime(Kind::Menu);
    // The default config's hidden widgets: settings (cog), item search (search icon), the two
    // stash searches and the image strip. The price check, the item check and the delve grid are
    // `menu::skip`; the menu itself is shown.
    let expected =
        [(2, None), (3, None), (4, Some("Map rolling")), (5, Some("Dump sorting")), (8, Some("Cheat sheets"))];
    for (id, title) in expected {
        let key = format!("widget-menu-entry-{id}");
        assert!(runtime.find(&key).is_some(), "no entry for widget {id}");
        if let Some(title) = title {
            assert_eq!(text_of(&runtime, &format!("{key}-title")).as_deref(), Some(title));
            assert!(runtime.find(&format!("{key}-icon")).is_none(), "widget {id} has no icon");
        } else {
            assert!(runtime.find(&format!("{key}-icon")).is_some(), "widget {id} has an icon only");
            assert!(runtime.find(&format!("{key}-title")).is_none());
        }
    }
    for id in [1, 6, 7, 9] {
        assert!(runtime.find(&format!("widget-menu-entry-{id}")).is_none(), "widget {id} is menu::skip or shown");
    }
    // The paste box is the menu's second row (`t(':price_check')` as its placeholder).
    assert_eq!(text_of(&runtime, "widget-menu-paste-input").as_deref(), Some(""));
}

/// The cog shows the settings window (`wm.show`), and a hidden widget's button shows it, which
/// takes it out of the list until it is hidden again.
#[test]
fn the_menu_shows_settings_and_toggles_a_widget() {
    let mut runtime = runtime(Kind::Menu);
    // The settings widget is hidden in the default config, and its cog shows it.
    assert_eq!(overlay(&runtime).widget(2).map(|widget| widget.wm_wants), Some(Wants::Hide));
    click(&mut runtime, "widget-menu-entry-2");
    assert_eq!(actions(&mut runtime), vec![Action::Show(2)]);
    assert_eq!(overlay(&runtime).widget(2).map(|widget| widget.wm_wants), Some(Wants::Show));

    click(&mut runtime, "widget-menu-entry-8");
    let actions = actions(&mut runtime);
    assert_eq!(actions, vec![Action::Show(8)]);
    assert_eq!(overlay(&runtime).widget(8).map(|widget| widget.wm_wants), Some(Wants::Show));
    // A shown widget is no longer a button (`watch(wmWants)`/`alwaysShow` are off by default).
    assert!(runtime.find("widget-menu-entry-8").is_none());

    overlay_mut(&mut runtime).hide(8);
    runtime.invalidate();
    runtime.frame(1.0 / 60.0);
    assert!(runtime.find("widget-menu-entry-8").is_some(), "hidden again, listed again");
}

/// The edit mode's `Show button for active widgets` (`v-model="config.alwaysShow"`) adds the
/// shown widgets back to the list.
#[test]
fn the_menu_always_show_toggle_lists_the_shown_widgets() {
    let mut runtime = runtime_panel(Kind::Menu);
    // The image strip is hidden, so it is a button; showing it takes it out of the list.
    click(&mut runtime, "widget-menu-entry-8");
    assert!(runtime.find("widget-menu-entry-8").is_none());

    hover(&mut runtime, "widget-menu");
    click(&mut runtime, "widget-1-edit");
    assert_eq!(overlay(&runtime).editing, Some(1));
    assert!(runtime.find("widget-menu-always-show-toggle").is_some());
    assert!(runtime.find("widget-menu-paste").is_none(), "the box makes room for the toggle");

    click(&mut runtime, "widget-menu-always-show-toggle");
    runtime.frame(1.0 / 60.0);
    assert_eq!(overlay(&runtime).widget(1).map(|widget| widget.config["alwaysShow"].as_bool()), Some(Some(true)));
    // The shown image strip is listed now (its border shows it as shown); widgets with
    // `menu::skip` still are not, and the menu itself carries that flag.
    assert!(runtime.find("widget-menu-entry-8").is_some());
    assert!(runtime.find("widget-menu-entry-1").is_none());
    assert!(runtime.find("widget-menu-entry-6").is_none());
}

/// The paste box hands its text to the price check (`MAIN->CLIENT::item-text`) and clears.
#[test]
fn the_menu_paste_box_dispatches_a_price_check() {
    let mut runtime = runtime(Kind::Menu);
    click(&mut runtime, "widget-menu-paste-input");
    runtime.frame(1.0 / 60.0);
    runtime.paste("Rarity: Rare");
    runtime.frame(1.0 / 60.0);
    assert_eq!(actions(&mut runtime), vec![Action::PriceCheck("Rarity: Rare".to_owned())]);
    assert_eq!(text_of(&runtime, "widget-menu-paste-input").as_deref(), Some(""));
}

/// The `fa-ellipsis-h` popover: `t(':add')` and one entry per `multi` widget type; pressing one
/// creates and shows the widget (`wm.create`).
#[test]
fn the_add_popover_creates_a_stopwatch() {
    let mut runtime = runtime(Kind::Menu);
    assert!(runtime.find("widget-menu-add-panel").is_none(), "closed until pressed");
    click(&mut runtime, "widget-menu-add");
    assert!(overlay(&runtime).add_open);
    assert_eq!(text_of(&runtime, "widget-menu-add-header").as_deref(), Some("add widget…"));
    assert_eq!(text_of(&runtime, "widget-menu-add-timer-label").as_deref(), Some("Stopwatch"));
    assert_eq!(text_of(&runtime, "widget-menu-add-stash-search-label").as_deref(), Some("Stash search"));
    assert_eq!(text_of(&runtime, "widget-menu-add-image-strip-label").as_deref(), Some("Image strip"));
    assert!(runtime.find("widget-menu-add-notepad").is_none(), "PoE1 has no notepad");

    click(&mut runtime, "widget-menu-add-timer");
    assert!(!overlay(&runtime).add_open, "choosing an entry closes the popover");
    let timer =
        overlay(&runtime).widgets.iter().find(|widget| widget.wm_type == "timer").cloned().expect("the created timer");
    assert_eq!(timer.wm_id, 10, "the config's largest id + 1");
    assert_eq!(timer.wm_wants, Wants::Show, "a new widget shows itself");
    assert_eq!(actions(&mut runtime), vec![Action::Create("timer".to_owned()), Action::Show(10)]);
}

/// EE2's popover has its notepad as a fourth entry.
#[test]
fn the_poe2_add_popover_lists_the_notepad() {
    let mut runtime = runtime_game(GameId::Poe2, Kind::Menu);
    click(&mut runtime, "widget-menu-add");
    assert_eq!(text_of(&runtime, "widget-menu-add-notepad-label").as_deref(), Some("Notepad"));
}

/// `WidgetTimer.vue`: start, tick, pause, reset, on the runtime's clock, and the `paused` badge
/// and `invisible-on-blur` flag that go with each.
#[test]
fn the_stopwatch_counts_up_and_stops() {
    let mut runtime = runtime(Kind::Stopwatch);
    let id = overlay(&runtime).shown;
    assert_eq!(text_of(&runtime, "stopwatch-time-hours").as_deref(), Some("00"));
    assert_eq!(text_of(&runtime, "stopwatch-time-colon-0").as_deref(), Some(":"));
    assert_eq!(text_of(&runtime, "stopwatch-time-minutes").as_deref(), Some("00"));
    assert_eq!(text_of(&runtime, "stopwatch-time-colon-1").as_deref(), Some(":"));
    assert_eq!(text_of(&runtime, "stopwatch-time-seconds").as_deref(), Some("00"));
    assert_eq!(text_of(&runtime, "stopwatch-paused-label").as_deref(), Some("paused"));
    assert!(runtime.find("stopwatch-controls").is_none(), "the controls only show on hover");
    assert!(overlay(&runtime).widget(id).is_some_and(|widget| widget.has_flag("invisible-on-blur")));

    // The pointer over the widget shows the play and reset buttons.
    hover(&mut runtime, "stopwatch");
    assert!(runtime.find("stopwatch-play").is_some());
    assert!(runtime.find("stopwatch-reset").is_some());

    click(&mut runtime, "stopwatch-play");
    assert!(overlay(&runtime).timer(id).expect("the timer").running);
    assert!(
        !overlay(&runtime).widget(id).is_some_and(|widget| widget.has_flag("invisible-on-blur")),
        "it stays while running"
    );
    assert!(runtime.find("stopwatch-paused").is_none());

    // `poll` advances it by the frame's time: one second, then a minute, then an hour.
    runtime.frame(1.0);
    assert_eq!(text_of(&runtime, "stopwatch-time-hours").as_deref(), Some("00"));
    assert_eq!(text_of(&runtime, "stopwatch-time-minutes").as_deref(), Some("00"));
    assert_eq!(text_of(&runtime, "stopwatch-time-seconds").as_deref(), Some("01"));
    runtime.frame(59.0);
    assert_eq!(text_of(&runtime, "stopwatch-time-minutes").as_deref(), Some("01"));
    assert_eq!(text_of(&runtime, "stopwatch-time-seconds").as_deref(), Some("00"));
    runtime.frame(3600.0);
    assert_eq!(text_of(&runtime, "stopwatch-time-hours").as_deref(), Some("01"));
    assert_eq!(text_of(&runtime, "stopwatch-time-minutes").as_deref(), Some("01"));
    assert_eq!(text_of(&runtime, "stopwatch-time-seconds").as_deref(), Some("00"));

    // Pause: the `paused` badge is back, and the clock stops.
    hover(&mut runtime, "stopwatch");
    click(&mut runtime, "stopwatch-pause");
    assert!(!overlay(&runtime).timer(id).expect("the timer").running);
    assert_eq!(text_of(&runtime, "stopwatch-paused-label").as_deref(), Some("paused"));
    runtime.frame(30.0);
    assert_eq!(text_of(&runtime, "stopwatch-time-hours").as_deref(), Some("01"), "a paused clock does not tick");

    click(&mut runtime, "stopwatch-reset");
    assert_eq!(text_of(&runtime, "stopwatch-time-hours").as_deref(), Some("00"));
    assert_eq!(text_of(&runtime, "stopwatch-time-colon-0").as_deref(), Some(":"));
    assert_eq!(text_of(&runtime, "stopwatch-time-minutes").as_deref(), Some("00"));
    assert_eq!(text_of(&runtime, "stopwatch-time-colon-1").as_deref(), Some(":"));
    assert_eq!(text_of(&runtime, "stopwatch-time-seconds").as_deref(), Some("00"));
    assert!(
        overlay(&runtime).widget(id).is_some_and(|widget| widget.has_flag("invisible-on-blur")),
        "a stopped timer hides with the overlay"
    );
}

/// A reset while running restarts it at one second (`(isRunning) ? 1000 : 0`), and the hotkeys
/// (`MAIN->CLIENT::widget-action`) start, pause and reset it as the buttons do.
#[test]
fn the_stopwatch_resets_to_one_second_while_running_and_takes_hotkeys() {
    let mut runtime = runtime(Kind::Stopwatch);
    let id = overlay(&runtime).shown;
    overlay_mut(&mut runtime).widget_action(&format!("stopwatch-start-stop:{id}"), 0.0);
    runtime.invalidate();
    runtime.frame(1.0 / 60.0);
    assert!(overlay(&runtime).timer(id).expect("the timer").running);
    runtime.frame(5.0);
    assert_eq!(text_of(&runtime, "stopwatch-time-seconds").as_deref(), Some("05"));

    overlay_mut(&mut runtime).widget_action(&format!("stopwatch-reset:{id}"), 6_000.0);
    runtime.invalidate();
    runtime.frame(1.0 / 60.0);
    assert_eq!(text_of(&runtime, "stopwatch-time-seconds").as_deref(), Some("01"));
    assert!(overlay(&runtime).timer(id).expect("the timer").running, "a reset does not pause it");

    overlay_mut(&mut runtime).widget_action(&format!("stopwatch-start-stop:{id}"), 6_000.0);
    runtime.invalidate();
    runtime.frame(1.0 / 60.0);
    assert!(!overlay(&runtime).timer(id).expect("the timer").running);
}

/// `WidgetImageStrip.vue`: the default "Cheat sheets" strip with `syndicate.jpg` drawn
/// `object-fit: cover`, and the edit mode's title input, remove button and file card.
#[test]
fn the_image_strip_shows_its_configured_images() {
    let mut runtime = runtime_panel(Kind::ImageStrip);
    let id = overlay(&runtime).shown;
    assert_eq!(id, 8, "the default config's image-strip widget");
    assert_eq!(text_of(&runtime, "image-strip-title-text").as_deref(), Some("Cheat sheets"));
    let elements = runtime.elements();
    let card = element(&elements, "image-strip-image-0").expect("the first image's card");
    assert_eq!([card.rect[2], card.rect[3]], [192.0, 104.0], "`w-48 h-[6.5rem]`");
    let picture = element(&elements, "image-strip-picture-0-img").expect("the picture");
    assert_eq!(picture.role, "img");
    assert_eq!(picture.label.as_deref(), Some("/images/syndicate.jpg"));
    // `object-fit: cover`: the 2536 × 695 picture is scaled to the 184 × 96 box's height and
    // centred, so it is wider than the box and the box's clip crops it.
    assert!(picture.rect[2] > 184.0, "{:?}", picture.rect);
    assert!((picture.rect[3] - 96.0).abs() < 1.0, "{:?}", picture.rect);
    assert!(runtime.find("image-strip-image-1").is_none(), "one image");
    assert!(runtime.find("image-strip-choose-file").is_none(), "the file card is edit mode's");

    // Editing: the title becomes an input, and each card gets its remove button and drag handle.
    hover(&mut runtime, "image-strip");
    click(&mut runtime, &format!("widget-{id}-edit"));
    assert_eq!(overlay(&runtime).editing, Some(id));
    assert_eq!(text_of(&runtime, "image-strip-title-input").as_deref(), Some("Cheat sheets"));
    assert!(runtime.find("image-strip-remove-0").is_some());
    assert!(runtime.find("image-strip-drag-0").is_some());
    assert!(runtime.find("image-strip-choose-file").is_some());

    click(&mut runtime, "image-strip-remove-0");
    runtime.frame(1.0 / 60.0);
    assert!(
        overlay(&runtime).widget(id).and_then(|widget| widget.config["images"].as_array()).is_some_and(Vec::is_empty)
    );
    assert!(runtime.find("image-strip-image-0").is_none(), "the image's card is gone");
    assert!(runtime.find("image-strip-choose-file").is_some(), "the file card stays");

    click(&mut runtime, "image-strip-choose-file-button");
    assert_eq!(actions(&mut runtime), vec![Action::ImportImage(id)]);
}

/// A strip with two configured images is two cards, and the window has to be as wide as both
/// (the strip is content-sized, like upstream's).
#[test]
fn a_strip_shows_every_configured_image() {
    let mut runtime = runtime_sized(GameId::Poe1, Kind::ImageStrip, (396.0, 136.0));
    let id = overlay(&runtime).shown;
    overlay_mut(&mut runtime).widget_mut(id).expect("the strip").config["images"] =
        serde_json::json!([{ "id": 1, "url": "syndicate.jpg" }, { "id": 2, "url": "dust.png" }]);
    runtime.invalidate();
    runtime.frame(1.0 / 60.0);
    let elements = runtime.elements();
    let first = element(&elements, "image-strip-image-0").expect("the first card");
    let second = element(&elements, "image-strip-image-1").expect("the second card");
    assert_eq!([first.rect[2], first.rect[3]], [192.0, 104.0]);
    assert_eq!(second.rect[0], first.rect[0] + 192.0 + 4.0, "`gap-x-1`");
    assert_eq!(
        element(&elements, "image-strip-picture-1-img").and_then(|picture| picture.label.as_deref()),
        Some("/images/dust.png")
    );
    assert!(runtime.find("image-strip-image-2").is_none());
}

/// The strip's title input writes `config.wmTitle` (`v-model`).
#[test]
fn the_image_strip_title_input_renames_the_widget() {
    let mut runtime = runtime_panel(Kind::ImageStrip);
    let id = overlay(&runtime).shown;
    hover(&mut runtime, "image-strip");
    click(&mut runtime, &format!("widget-{id}-edit"));
    click(&mut runtime, "image-strip-title-input");
    runtime.frame(1.0 / 60.0);
    runtime.key(rune::KeyEvent { key: rune::Key::Command('a'), mods: rune::Mods::default() });
    runtime.frame(1.0 / 60.0);
    runtime.paste("Delve");
    runtime.frame(1.0 / 60.0);
    assert_eq!(overlay(&runtime).widget(id).map(|widget| widget.wm_title.as_str()), Some("Delve"));
}

/// `WidgetDelveGrid.vue`: 9 × 11 cells of the overlay height's size, centred in an 858 × 673
/// box that crops the overflow, and the hotkey toggles the widget (`menu::skip`, so the menu has
/// no button for it).
#[test]
fn the_delve_grid_draws_nine_by_eleven_cells() {
    let mut runtime = runtime(Kind::DelveGrid);
    let metrics = widgets::delve_grid::Metrics::for_overlay(crate::HEIGHT);
    assert_eq!((metrics.width, metrics.height), (858.0, 673.0), "1030 × 808 of a 1080-tall panel");
    assert_eq!(metrics.cell, 81.0);

    let elements = runtime.elements();
    let mut cells = 0;
    for row in 0..9 {
        for column in 0..11 {
            let cell = element(&elements, &format!("delve-grid-cell-{row}-{column}"))
                .unwrap_or_else(|| panic!("no cell {row}/{column}"));
            assert_eq!([cell.rect[2], cell.rect[3]], [81.0, 81.0]);
            cells += 1;
        }
    }
    assert_eq!(cells, 99);
    assert!(element(&elements, "delve-grid-cell-9-0").is_none(), "nine rows");

    // The cells are centred with the overflow they leave: (858 - 891) / 2 and (673 - 729) / 2.
    let card = element(&elements, "delve-grid").expect("the box");
    assert_eq!([card.rect[2], card.rect[3]], [858.0, 673.0]);
    let first = element(&elements, "delve-grid-cell-0-0").expect("the first cell");
    assert_eq!(first.rect[0] - card.rect[0], -16.5);
    assert_eq!(first.rect[1] - card.rect[1], -28.0);

    // Its hotkey (`widget-action` target `delve-grid`) shows and hides it.
    let id = overlay(&runtime).shown;
    assert!(overlay(&runtime).widget(id).is_some_and(|widget| widget.has_flag("hide-on-focus")), "`hide-on-focus`");
    overlay_mut(&mut runtime).widget_action("delve-grid", 0.0);
    assert_eq!(overlay(&runtime).widget(id).map(|widget| widget.wm_wants), Some(Wants::Hide));
    overlay_mut(&mut runtime).widget_action("delve-grid", 0.0);
    assert_eq!(overlay(&runtime).widget(id).map(|widget| widget.wm_wants), Some(Wants::Show));
}

/// `Widget.vue`'s hover panel: `hide`, `edit`, `move` and `delete` as the widget allows them,
/// and `delete` removes the widget (`wm.remove`).
#[test]
fn the_widgets_hover_panel_offers_its_actions() {
    let mut runtime = runtime_panel(Kind::Stopwatch);
    let id = overlay(&runtime).shown;
    assert!(runtime.find(&format!("widget-{id}-panel")).is_none(), "hidden while the pointer is away");

    hover(&mut runtime, "stopwatch");
    assert_eq!(overlay(&runtime).hover, Some(id));
    // `WidgetTimer.vue` is `hideable: false` and edits in the settings window.
    assert_eq!(text_of(&runtime, &format!("widget-{id}-edit-label")).as_deref(), Some("edit"));
    assert_eq!(text_of(&runtime, &format!("widget-{id}-move-label")).as_deref(), Some("move"));
    assert_eq!(text_of(&runtime, &format!("widget-{id}-delete-label")).as_deref(), Some("delete"));
    assert!(runtime.find(&format!("widget-{id}-hide")).is_none());

    // `edit` on a widget without inline editing opens the settings window on it.
    click(&mut runtime, &format!("widget-{id}-edit"));
    // `wm.setFlag(settings, …)` then `wm.show(settings)`: the show is the recorded manager call,
    // and `EditInSettings` is the port's note of which widget the settings window was pointed at.
    assert_eq!(actions(&mut runtime), vec![Action::Show(2), Action::EditInSettings(id)]);

    // `move` draws the handles; the timer's are the centre one.
    click(&mut runtime, &format!("widget-{id}-move"));
    assert_eq!(overlay(&runtime).moving, Some(id));
    assert!(runtime.find(&format!("widget-{id}-handle-cc")).is_some());
    let elements_before = runtime.elements();
    assert!(
        element(&elements_before, &format!("widget-{id}")).is_some_and(|frame| frame.opacity < 1.0),
        "`Widget.vue`'s `opacity-75` starts as the move mode comes on"
    );
    settle(&mut runtime);
    let elements = runtime.elements();
    let frame = element(&elements, &format!("widget-{id}")).expect("the card's frame");
    assert!((frame.opacity - 0.75).abs() < 0.01, "`opacity-75` while moving: {}", frame.opacity);

    click(&mut runtime, &format!("widget-{id}-delete"));
    assert!(overlay(&runtime).widget(id).is_none(), "removed");
    assert_eq!(actions(&mut runtime), vec![Action::Remove(id)]);
}

/// The image strip's panel has `hide` and edits inline; the delve grid's is `hide` only
/// (`readonly`, `removable: false`, no move handles).
#[test]
fn the_panel_follows_each_widgets_props() {
    let mut strip = runtime_panel(Kind::ImageStrip);
    hover(&mut strip, "image-strip");
    let id = overlay(&strip).shown;
    assert!(strip.find(&format!("widget-{id}-hide")).is_some());
    assert!(strip.find(&format!("widget-{id}-edit")).is_some());
    assert!(strip.find(&format!("widget-{id}-move")).is_some());
    assert!(strip.find(&format!("widget-{id}-delete")).is_some());
    click(&mut strip, &format!("widget-{id}-hide"));
    assert_eq!(actions(&mut strip), vec![Action::Hide(id)]);

    let mut grid = runtime_panel(Kind::DelveGrid);
    hover(&mut grid, "delve-grid");
    let id = overlay(&grid).shown;
    assert!(grid.find(&format!("widget-{id}-hide")).is_some());
    assert!(grid.find(&format!("widget-{id}-edit")).is_none(), "readonly");
    assert!(grid.find(&format!("widget-{id}-move")).is_none(), "`move-handles: none`");
    assert!(grid.find(&format!("widget-{id}-delete")).is_none(), "`removable: false`");
}

/// `Widget.vue`'s `updatePosition`: `Math.min(Math.max(e.clientX / window.innerWidth, 0), 1) *
/// 100`, the percentages an `anchor` stores.
#[test]
fn a_pointer_becomes_the_anchors_percentages() {
    assert_eq!(widgets::pointer_percent([400.0, 300.0], (1600.0, 1200.0)), (25.0, 25.0));
    assert_eq!(widgets::pointer_percent([0.0, 0.0], (1600.0, 1200.0)), (0.0, 0.0));
    assert_eq!(widgets::pointer_percent([1600.0, 1200.0], (1600.0, 1200.0)), (100.0, 100.0));
    // Outside the overlay the percentage stops at the edge, as upstream's clamp does.
    assert_eq!(widgets::pointer_percent([-50.0, 2000.0], (1600.0, 1200.0)), (0.0, 100.0));
    // A scene (or a window) whose size is not known yet has no percentages to write.
    assert_eq!(widgets::pointer_percent([20.0, 20.0], (0.0, 0.0)), (0.0, 0.0));
}

/// `Widget.vue`'s `startMove`/`updatePosition`: a handle press makes its `pos` the anchor, puts
/// that corner under the pointer and follows it; the release is what the shell saves, and a
/// cancelled gesture (Rune takes the pointer away) puts the anchor back.
#[test]
fn a_move_handle_writes_the_anchor_and_asks_to_save() {
    let mut app = App::widgets_scene(GameId::Poe1, Kind::Stopwatch);
    let overlay = app.widgets.as_mut().expect("a widget scene");
    let id = overlay.shown;
    let before = (overlay.widget(id).expect("the timer").anchor_pos(), overlay.widget(id).unwrap().anchor());
    overlay.size = (1600.0, 1200.0);

    overlay.begin_move(id, "tl", [400.0, 300.0]);
    assert_eq!(overlay.moving, Some(id), "`Widget.vue`'s `isMoving`");
    assert_eq!(overlay.widget(id).unwrap().anchor_pos(), "tl");
    assert_eq!(overlay.widget(id).unwrap().anchor(), (25.0, 25.0));
    assert!(overlay.take_config_dirty(), "the press raised the widget (`bringToTop`)");

    // A cancelled drag is not a move: the anchor is the one the press found.
    overlay.end_move(id, "tl", [400.0, 300.0], true);
    assert_eq!(
        (overlay.widget(id).unwrap().anchor_pos(), overlay.widget(id).unwrap().anchor()),
        before,
        "a cancelled gesture restores the anchor"
    );
    assert!(!overlay.config_dirty());

    // A released one keeps the pointer's place, every frame of the drag.
    overlay.begin_move(id, "tl", [400.0, 300.0]);
    overlay.drag_to(id, "tl", [800.0, 600.0]);
    assert_eq!(overlay.widget(id).unwrap().anchor(), (50.0, 50.0));
    overlay.end_move(id, "tl", [800.0, 600.0], false);
    assert!(overlay.take_config_dirty(), "the shell has a new anchor to save");
    assert!(!overlay.config_dirty(), "one save per gesture");
}

/// `OverlayWindow.vue`'s `bringToTop`: the pressed widget gets one `wmZorder` above the topmost
/// numbered one, an already-topmost one is left alone (upstream returns early), and an exclusive
/// widget keeps its own place. Every change is what the shell saves.
#[test]
fn pressing_a_widget_raises_it_and_marks_the_config() {
    let mut app = App::widgets_scene(GameId::Poe1, Kind::Menu);
    let overlay = app.widgets.as_mut().expect("a widget scene");
    let menu = overlay.shown;
    let strip = overlay
        .widgets
        .iter()
        .find(|widget| widget.wm_type == Kind::ImageStrip.wm_type())
        .expect("the config's image strip")
        .wm_id;
    let settings = overlay
        .widgets
        .iter()
        .find(|widget| widget.wm_type == widgets::SETTINGS)
        .expect("the config's settings widget")
        .wm_id;
    assert_eq!(overlay.widget(menu).unwrap().zorder(), Some(1.0), "the menu's own zorder");
    assert_eq!(overlay.widget(strip).unwrap().zorder(), None, "CSS `auto`");

    // `wm.show` does not raise; `Widget.vue`'s mousedown does.
    overlay.show(strip);
    overlay.take_actions();
    assert!(!overlay.config_dirty());
    overlay.bring_to_top(strip);
    assert_eq!(overlay.widget(strip).unwrap().zorder(), Some(2.0));
    assert!(overlay.take_config_dirty(), "the shell saves the new paint order");

    overlay.bring_to_top(strip);
    assert!(!overlay.config_dirty(), "already the topmost: nothing to save");
    overlay.bring_to_top(menu);
    assert_eq!(overlay.widget(menu).unwrap().zorder(), Some(3.0), "the lower card came to the front");
    assert!(overlay.take_config_dirty(), "the menu's new order is saved");

    // `wmZorder: 'exclusive'` already wins the whole overlay.
    overlay.bring_to_top(settings);
    assert_eq!(overlay.widget(settings).unwrap().zorder(), None, "not a number");
    assert!(!overlay.config_dirty());
}

/// Let animated values (a layout change's spring) land: the first frame only starts them.
fn settle(runtime: &mut Runtime<App>) {
    for _ in 0..60 {
        runtime.frame(1.0 / 60.0);
    }
}

/// Focus a field and paste `text` over its contents.
fn type_query(runtime: &mut Runtime<App>, key: &str, text: &str) {
    click(runtime, key);
    runtime.key(rune::KeyEvent { key: rune::Key::Command('a'), mods: rune::Mods::default() });
    runtime.frame(1.0 / 60.0);
    for ch in text.chars() {
        runtime.key(rune::KeyEvent { key: rune::Key::Text(ch.to_string()), mods: rune::Mods::default() });
    }
    // The keys land in the field over the next frames, and the card grows around the results;
    // let the layout land before a press reads its rect.
    settle(runtime);
}

/// `WidgetStashSearch.vue`: the config's entries as buttons, one `CLIENT->MAIN::user-action` per
/// press, and the "Keys" switch only when an entry has a hotkey.
#[test]
fn the_stash_search_buttons_send_their_text() {
    let mut runtime = runtime(Kind::StashSearch);
    assert_eq!(text_of(&runtime, "stash-search-title").as_deref(), Some("Map rolling"));
    assert_eq!(text_of(&runtime, "stash-search-entry-0-text").as_deref(), Some("\"Pack Size: +3\""));
    assert_eq!(text_of(&runtime, "stash-search-entry-3-text").as_deref(), Some("\"Cannot Leech Mana\""));
    // `hasHotkeys`: the default entries carry none, so the switch is not drawn.
    assert!(runtime.find("stash-search-keys").is_none());

    // One press is one action, with the entry's exact text.
    click(&mut runtime, "stash-search-entry-0");
    assert_eq!(actions(&mut runtime), [Action::StashSearch("\"Pack Size: +3\"".to_owned())]);
    assert!(actions(&mut runtime).is_empty(), "nothing runs by itself");

    // A hotkey label shows the switch; it hides the labels when it is off. The entries are the
    // widget's own config object, which is what the settings window's editor writes.
    let id = overlay(&runtime).shown;
    if let Some(widget) = overlay_mut(&mut runtime).widget_mut(id)
        && let Some(entries) = widget.config.get_mut("entries").and_then(|entries| entries.as_array_mut())
    {
        entries[0]["hotkey"] = serde_json::json!("Ctrl+Shift+1");
    }
    // A press re-runs the view, which is what draws from the edited config.
    click(&mut runtime, "stash-search-entry-3");
    assert_eq!(actions(&mut runtime), [Action::StashSearch("\"Cannot Leech Mana\"".to_owned())]);
    assert!(runtime.find("stash-search-keys").is_some(), "the Keys switch");
    assert_eq!(text_of(&runtime, "stash-search-entry-0-hotkey-text").as_deref(), Some("Ctrl+Shift+1"));
    // The card grows to fit the switch; Rune springs the layout change, so let it land.
    settle(&mut runtime);
    assert_eq!(text_of(&runtime, "stash-search-title").as_deref(), Some("Map rolling"));
    click(&mut runtime, "stash-search-keys");
    assert!(!overlay(&runtime).shown_widget().unwrap().config["enableHotkeys"].as_bool().unwrap());
    assert!(runtime.find("stash-search-entry-0-hotkey").is_none(), "the labels follow the switch");
    click(&mut runtime, "stash-search-keys");
    assert!(runtime.find("stash-search-entry-0-hotkey").is_some());
}

/// `WidgetItemSearch.vue` on APT: the gem tab's list is empty at this pin, the Replicas tab
/// finds a replica, and pressing its strip entry reopens the price check on the virtual item.
#[test]
fn the_item_search_prices_a_picked_replica() {
    let mut runtime = runtime(Kind::ItemSearch);
    // APT opens on the gem tab, which has no names at this pin: the query finds nothing.
    type_query(&mut runtime, "item-search-input", "headhunter");
    assert_eq!(
        text_of(&runtime, "item-search-not-found-label").as_deref(),
        Some("No items found."),
        "the gem tab is empty"
    );
    assert_eq!(text_of(&runtime, "item-search-tabs-label").as_deref(), Some("Heist target:"), "APT's tab label");
    assert_eq!(text_of(&runtime, "item-search-tab-gem-label").as_deref(), Some("Skill Gem"));
    // APT's replica tab names what it no longer searches, struck through (`Base items`).
    assert_eq!(text_of(&runtime, "item-search-tab-replica-label").as_deref(), Some("Replicas, Base items"));

    // The tab press itself is the same handler the button runs; `update` re-runs the view as a
    // press does, without a click landing on a result that is still sliding into place.
    runtime.update(|app, _tx| app.item_search(|state| state.set_tab(Tab::Replica)));
    settle(&mut runtime);
    assert_eq!(text_of(&runtime, "item-search-result-0-name-text").as_deref(), Some("Replica Headhunter"));

    click(&mut runtime, "item-search-result-0-select");
    settle(&mut runtime);
    // `searchValue = ''`, and the pick sits in the strip with its price.
    assert_eq!(text_of(&runtime, "item-search-input").as_deref(), Some(""));
    assert_eq!(text_of(&runtime, "item-search-selected-0-name").as_deref(), Some("Replica Headhunter"));
    // The recorded poe.ninja blob prices it at six divines.
    assert_eq!(text_of(&runtime, "item-search-price-min").as_deref(), Some("6"));
    assert_eq!(text_of(&runtime, "item-search-price-currency").as_deref(), Some("\u{a0}div"));

    // `selectedItemClick`: the app reopens the price check on the virtual item.
    click(&mut runtime, "item-search-selected-0");
    settle(&mut runtime);
    assert!(
        runtime
            .state
            .widgets
            .as_ref()
            .is_none_or(|overlay| { overlay.shown_widget().is_some_and(|widget| widget.wm_wants == Wants::Hide) }),
        "the price check replaced the widget"
    );
    let item = runtime.state.price_check.item().expect("the virtual item");
    assert_eq!(item.info().name, "Replica Headhunter");
    assert_eq!(item.raw_text, "VIRTUAL_ITEM");
    assert_eq!(item.rarity, Some(wraex_core::Rarity::Unique));
    assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Replica Headhunter"));
}

/// EE2's copy: a query lists the base types, a unique's row names its base beside it, and the
/// strip entry opens the PoE2 price check on the virtual item.
#[test]
fn the_poe2_item_search_prices_a_picked_unique() {
    let mut runtime = runtime_game(GameId::Poe2, Kind::ItemSearch);
    assert_eq!(text_of(&runtime, "item-search-tabs-label").as_deref(), Some("Type of Item:"));
    assert_eq!(text_of(&runtime, "item-search-tab-gem-label").as_deref(), Some("Skill Gems"));
    assert_eq!(text_of(&runtime, "item-search-tab-unique-label").as_deref(), Some("Uniques"));
    assert_eq!(text_of(&runtime, "item-search-tab-base_item-label").as_deref(), Some("Base Items"));

    // The base-item tab is the one it opens on.
    type_query(&mut runtime, "item-search-input", "divine orb");
    assert_eq!(text_of(&runtime, "item-search-result-0-name-text").as_deref(), Some("Divine Orb"));

    // The unique tab: a unique's row names its base beside it (`item.name + ' ' + item.unique.base`).
    runtime.update(|app, _tx| {
        app.item_search(|state| {
            state.set_tab(Tab::Unique);
            state.set_query("Mageblood");
        })
    });
    settle(&mut runtime);
    assert_eq!(text_of(&runtime, "item-search-result-0-name-text").as_deref(), Some("Mageblood Utility Belt"));

    click(&mut runtime, "item-search-result-0-select");
    settle(&mut runtime);
    assert_eq!(text_of(&runtime, "item-search-selected-0-name").as_deref(), Some("Mageblood"));
    // The screenshots' recorded price database has no uniques, so it shows the reference's `?`.
    assert_eq!(text_of(&runtime, "item-search-price-min").as_deref(), Some("?"));
    assert_eq!(text_of(&runtime, "item-search-price-currency").as_deref(), Some(""));

    // Pressing the strip entry reopens the PoE2 price check on the virtual item.
    click(&mut runtime, "item-search-selected-0");
    settle(&mut runtime);
    let check = runtime.state.poe2.as_ref().expect("the PoE2 price check");
    let item = check.item().expect("the virtual item");
    assert_eq!(item.info().name.as_deref(), Some("Mageblood"));
    assert_eq!(item.raw_text, "VIRTUAL_ITEM");
    assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Mageblood"));
}

/// `WidgetXpTracker.vue`: the scene's fixed level 72 / area 80 state prints a `98.3%`
/// penalty, `Over: -1`, the untested-range asterisk, and the level in its input.
#[test]
fn the_xp_tracker_shows_the_logs_levels_and_penalty() {
    let runtime = runtime_game(GameId::Poe2, Kind::XpTracker);
    assert_eq!(text_of(&runtime, "xp-tracker-level").as_deref(), Some("72"));
    assert_eq!(text_of(&runtime, "xp-tracker-exp-label").as_deref(), Some("Exp *:"));
    assert_eq!(text_of(&runtime, "xp-tracker-exp-value").as_deref(), Some("98.3%"));
    assert_eq!(text_of(&runtime, "xp-tracker-over-label").as_deref(), Some("Over:"));
    assert_eq!(text_of(&runtime, "xp-tracker-over-value").as_deref(), Some("-1"));

    // The reference's card is 192 × 40 (`target/screens/poe2/xp-tracker.png`).
    let elements = runtime.elements();
    let card = element(&elements, "xp-tracker").expect("the card");
    assert!((card.rect[2] - 192.0).abs() <= 2.0, "width {}", card.rect[2]);
    assert!((card.rect[3] - 40.0).abs() <= 2.0, "height {}", card.rect[3]);

    // APT has no experience tracker, so its scenes cannot open one.
    assert!(widgets::scene("widget-xp-tracker").is_none(), "PoE1 has no client-log widget");
    assert!(widgets::scene("poe2-widget-xp-tracker").is_some());
}
