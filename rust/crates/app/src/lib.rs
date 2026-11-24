//! The Wraex desktop app.
//!
//! A Rune shell over the core crates. It has one window per game: PoE1's price check
//! ([`poe1::price_check`]) and PoE2's ([`poe2::price_check`]); the title bar and the widgets
//! they are built from live in [`ui`] and [`theme`], and both windows trade through [`trade`]
//! and the recorded answers of [`replay`]. Each game's item check ([`poe1::item_check`],
//! [`poe2::item_check`]) draws its map check ([`poe1::map_check`], [`poe2::map_check`]) for a
//! map-like item. The settings window ([`settings`]) opens over either game's check, one scene
//! per page. The overlay's own widgets ([`widgets`]) — the menu, the stopwatch, the image strip,
//! the delve grid, the item search and the stash search — are one scene each.
//!
//! `cargo run -p wraex-app` runs the app: a tray, the config's global hotkeys and a window
//! opened on demand ([`shell`]). `-- --game poe2` picks the other game. Anything else —
//! `-- --snap out.png --scene <slug>`, `--tree`, a script — is the port's screenshot mode,
//! which renders one corpus case headlessly and touches no OS state (`--list-scenes` lists
//! the cases).

pub mod poe1;
pub mod poe2;
pub mod replay;
pub mod settings;
pub mod shell;
pub mod theme;
pub mod trade;
pub mod ui;
pub mod widgets;

use rune::{AppSpec, Cx, El, Waker};
use wraex_core::GameId;

use crate::poe1::price_check::{PriceCheck, WIDTH};
use crate::settings::{Page, Settings};
use crate::trade::Mode;

/// The window's height, in points. Upstream's overlay spans the game's panel; the screenshots
/// in `tools/golden/screens` are 900 tall, so the port's window is too.
pub const HEIGHT: f32 = 900.0;

/// The app's state: which game's window is open, and both games' price checks.
///
/// A window built for PoE1 has no PoE2 state until a PoE2 scene asks for one (its data and
/// prices are not free), and vice versa: the other game's field is idle but keeps its state,
/// as Rune keeps a window's state across closes.
pub struct App {
    /// Which game's price check the window shows.
    pub game: GameId,
    /// PoE1's window, as before.
    pub price_check: PriceCheck,
    /// PoE2's window, once a PoE2 scene opened it.
    pub poe2: Option<poe2::price_check::PriceCheck>,
    /// The settings window, while it is open over the game's price check. It edits a working
    /// copy of the config (`settings::Settings`) and writes it only on Save.
    pub settings: Option<Settings>,
    /// The item check window (`WidgetItemCheck.vue`) on PoE1: the map check for a map-like item,
    /// the item's info otherwise. A scene or the item-check hotkey opens it; its top-level
    /// buttons record their [`poe1::item_check::Action`] for the shell instead of running it.
    pub item_check: Option<poe1::item_check::ItemCheck>,
    /// The item check window on PoE2.
    pub item_check_poe2: Option<poe2::item_check::ItemCheck>,
    /// An overlay widget (`widgets`): the menu, the stopwatch, the image strip, the delve grid,
    /// the item search or the stash search, one per scene. A live window draws one widget at a
    /// time — the one a hotkey or the widget menu asks for — because it is an ordinary window,
    /// where upstream's overlay paints every `show` widget over the game (phase 2).
    pub widgets: Option<widgets::Overlay>,
    /// Where trade requests go: HTTPS in a window, recorded answers headless.
    mode: Mode,
    /// The waker `AppSpec::poll` gave the app, handed to whichever window is built later.
    waker: Option<Waker>,
    /// The running app's OS half: the tray, the hotkeys, the clipboard and the startup work
    /// ([`shell`]). `None` in a scene, a `--snap` render and a test, which is what keeps every
    /// side effect out of them.
    pub live: Option<shell::Live>,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("game", &self.game)
            .field("price_check", &self.price_check)
            .field("poe2", &self.poe2)
            .field("settings", &self.settings)
            .field("item_check", &self.item_check)
            .field("item_check_poe2", &self.item_check_poe2)
            .field("widgets", &self.widgets)
            .field("mode", &self.mode)
            .field("live", &self.live)
            .finish()
    }
}

impl App {
    /// The app with its price check opened on `text` (the game's copy key produced it).
    pub fn from_text(text: &str) -> Self {
        Self::with_mode(text, Mode::for_run())
    }

    /// [`App::from_text`] with the trade transport chosen.
    pub fn with_mode(text: &str, mode: Mode) -> Self {
        Self {
            game: GameId::Poe1,
            price_check: PriceCheck::with_mode(text, mode),
            poe2: None,
            settings: None,
            item_check: None,
            item_check_poe2: None,
            widgets: None,
            mode,
            waker: None,
            live: None,
        }
    }

    /// The app with PoE2's window opened on `text`.
    pub fn with_mode_poe2(text: &str, mode: Mode) -> Self {
        let app = Self {
            game: GameId::Poe2,
            // PoE1's window is not shown in this scene; it still parses the text so that a
            // later switch back has something to show.
            price_check: PriceCheck::with_mode(text, mode),
            poe2: Some(poe2::price_check::PriceCheck::with_mode(text, mode)),
            settings: None,
            item_check: None,
            item_check_poe2: None,
            widgets: None,
            mode,
            waker: None,
            live: None,
        };
        app.attach_waker();
        app
    }

    /// The app with the settings window open on one of its pages, over the first scene's
    /// price check. A scene never reads or writes the user's config file: [`Settings::scene`]
    /// opens the default config on a temp-dir store.
    pub fn settings_scene(game: GameId, page: Page) -> Self {
        let mut app = match game {
            GameId::Poe2 => Self::with_mode_poe2(&corpus_poe2("currency-djinn-barya"), Mode::Replay),
            _ => Self::with_mode(&corpus(SCENES[0].0), Mode::Replay),
        };
        app.game = game;
        app.settings = Some(Settings::scene(game, page));
        app
    }

    /// A corpus case answered from the recorded screenshots, for tests.
    pub fn replay(slug: &str) -> Self {
        Self::with_mode(&corpus(slug), Mode::Replay)
    }

    /// Reopen the PoE1 price check on a corpus case: `<slug>.txt`.
    pub fn load(&mut self, slug: &str) {
        self.game = GameId::Poe1;
        self.price_check = PriceCheck::with_mode(&corpus(slug), self.mode);
    }

    /// Reopen the price check and press Search, as the screenshot harness does for the
    /// cases whose reference window does not search by itself (a rare, a map).
    pub fn load_and_search(&mut self, slug: &str) {
        self.load(slug);
        self.price_check.start_search();
    }

    /// `--scene settings-<game>-<page>` and the tests: replace the state with the settings
    /// window on `page`.
    pub fn settings_page(&mut self, game: GameId, page: Page) {
        let waker = self.waker.clone();
        *self = Self::settings_scene(game, page);
        // The scenes' setup runs after this; a window that switches at run time keeps the
        // waker its update check answers on.
        if let Some(waker) = waker {
            self.set_waker(waker);
        }
    }

    /// Reopen PoE2's price check on a PoE2 corpus case: `tools/golden/corpus/poe2/<slug>.txt`.
    pub fn load_poe2(&mut self, slug: &str) {
        self.game = GameId::Poe2;
        self.poe2 = Some(poe2::price_check::PriceCheck::with_mode(&corpus_poe2(slug), self.mode));
        self.attach_waker();
    }

    /// [`App::load_poe2`] and press Search, for the cases the reference does not search by
    /// itself (a rare bow, a waystone).
    pub fn load_and_search_poe2(&mut self, slug: &str) {
        self.load_poe2(slug);
        if let Some(check) = &mut self.poe2 {
            check.start_search();
        }
    }

    /// `--scene map-check-<case>` / `--scene item-check-<case>`: the item check on a PoE1 corpus
    /// case. The widget picks its own half: the map check for a map-like item, the item info
    /// otherwise, as `WidgetItemCheck.vue` does.
    pub fn load_item_check(&mut self, slug: &str) {
        *self = Self::item_check_scene(GameId::Poe1, slug);
    }

    /// [`App::load_item_check`] on PoE2's corpus (`poe2-map-check-<case>`, `poe2-item-check-<case>`).
    pub fn load_item_check_poe2(&mut self, slug: &str) {
        *self = Self::item_check_scene(GameId::Poe2, slug);
    }

    /// The app with only the item check window open on `slug`: a scene never reads or writes the
    /// user's config or the network (the map check reads the game data only).
    pub fn item_check_scene(game: GameId, slug: &str) -> Self {
        let mut app = match game {
            GameId::Poe2 => Self::with_mode_poe2(&corpus_poe2(slug), Mode::Replay),
            _ => Self::with_mode(&corpus(slug), Mode::Replay),
        };
        app.game = game;
        match game {
            GameId::Poe2 => {
                app.item_check_poe2 = Some(poe2::item_check::ItemCheck::from_text(&corpus_poe2(slug), Mode::Replay))
            }
            _ => app.item_check = Some(poe1::item_check::ItemCheck::from_text(&corpus(slug))),
        }
        app
    }

    /// The app with only an overlay widget open on `kind`'s scene: the menu, the stopwatch, the
    /// image strip, the delve grid, the item search or the stash search, at the card's own size.
    /// A scene never reads the user's
    /// config or the network: [`widgets::Overlay::scene`] reads `default_config()` only.
    pub fn widgets_scene(game: GameId, kind: widgets::Kind) -> Self {
        let mut app = match game {
            GameId::Poe2 => Self::with_mode_poe2(&corpus_poe2("currency-djinn-barya"), Mode::Replay),
            _ => Self::with_mode(&corpus(SCENES[0].0), Mode::Replay),
        };
        app.game = game;
        app.widgets = Some(widgets::Overlay::scene(game, kind));
        app
    }

    /// `--scene widget-<name>` / `--scene poe2-widget-<name>` and the tests: replace the state
    /// with that widget's scene.
    pub fn show_widget(&mut self, game: GameId, kind: widgets::Kind) {
        let waker = self.waker.clone();
        *self = Self::widgets_scene(game, kind);
        if let Some(waker) = waker {
            self.set_waker(waker);
        }
    }

    /// The waker the window's trade and price answers use to run `poll`.
    pub fn set_waker(&mut self, waker: Waker) {
        self.price_check.set_waker(waker.clone());
        // The live shell's startup price fetch lands on the same waker.
        if let Some(prices) = self.live.as_ref().and_then(|live| live.prices.as_ref()) {
            prices.set_waker(waker.clone());
        }
        // The about page's update check lands on the same waker.
        if let Some(settings) = &self.settings {
            settings.set_waker(waker.clone());
        }
        // The image cache wakes the same window when a picture finishes decoding.
        ui::art::install_waker(waker.clone());
        self.waker = Some(waker);
        self.attach_waker();
    }

    /// Hand the stored waker to the PoE2 window, when one exists.
    fn attach_waker(&self) {
        if let (Some(waker), Some(check)) = (&self.waker, &self.poe2) {
            check.set_waker(waker.clone());
        }
    }

    /// `AppSpec::poll`: a landed answer in whichever window is open, the overlay's stopwatches
    /// advanced to `clock`, and the item search's picked entry opened; true when something
    /// changed.
    pub fn poll(&mut self, clock: f64) -> bool {
        let poe1 = self.price_check.poll();
        let poe2 = self.poe2.as_mut().is_some_and(|check| check.poll());
        let settings = self.settings.as_mut().is_some_and(|settings| settings.poll());
        let widgets = self.widgets.as_mut().is_some_and(|overlay| overlay.poll(clock));
        // The item search's field shows the query its results came from: Rune's write-back of a
        // blurred field's model can land empty, and the view rebuilds the field from it.
        let repaired = self
            .widgets
            .as_mut()
            .and_then(|overlay| overlay.item_search.as_mut())
            .is_some_and(|state| state.sync_field());
        // `MAIN->CLIENT::item-text` from the item search's strip: the widget records the
        // dispatch, and the shell reopens the price check on the item.
        let opened = match self.widgets.as_mut().and_then(widgets::item_search::take_picked) {
            Some(picked) => {
                self.open_picked_item(picked);
                true
            }
            None => false,
        };
        poe1 || poe2 || settings || widgets || opened || repaired
    }

    /// `MAIN->CLIENT::item-text` with an item the parser never saw: the item search's
    /// `selectedItemClick` builds a `createVirtualItem` and self-dispatches it, which reopens the
    /// price check on it. In an overlay both widgets would stay up; this one-window shell shows
    /// the price check instead of the widget that asked for it.
    pub fn open_picked_item(&mut self, picked: widgets::item_search::Picked) {
        // Hide the widget rather than drop it: a queued handler of the frame that pressed the
        // strip entry still projects onto the overlay, and its state is what a later show returns
        // to. The view stops drawing it once it is hidden.
        if let Some(overlay) = self.widgets.as_mut()
            && let Some(widget) = overlay.widget_mut(overlay.shown)
        {
            widget.set_wants(widgets::Wants::Hide);
        }
        match picked {
            widgets::item_search::Picked::Poe1(item) => {
                self.game = GameId::Poe1;
                self.price_check = PriceCheck::with_item(*item, self.mode);
                if let Some(waker) = &self.waker {
                    self.price_check.set_waker(waker.clone());
                }
            }
            widgets::item_search::Picked::Poe2(item) => {
                self.game = GameId::Poe2;
                self.poe2 = Some(poe2::price_check::PriceCheck::with_item(*item, self.mode));
                self.attach_waker();
            }
        }
    }

    /// Run `f` on the item search's state, when the item-search widget built one. The scenes use
    /// it to open on the query their screenshot types.
    pub fn item_search(&mut self, f: impl FnOnce(&mut widgets::item_search::State)) {
        if let Some(state) = self.widgets.as_mut().and_then(|overlay| overlay.item_search.as_mut()) {
            f(state);
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::with_mode(&corpus(SCENES[0].0), Mode::for_run())
    }
}

/// `App` → the PoE1 price check, for the window's handlers.
fn project(app: &mut App) -> &mut PriceCheck {
    &mut app.price_check
}

/// `App` → the PoE2 price check, for the window's handlers.
fn project_poe2(app: &mut App) -> &mut poe2::price_check::PriceCheck {
    app.poe2.as_mut().expect("a PoE2 scene built the window")
}

/// `App` → the settings window, for its handlers.
fn project_settings(app: &mut App) -> &mut Settings {
    app.settings.as_mut().expect("a settings scene built the window")
}

/// `App` → the overlay widget's state, for its handlers.
fn project_widgets(app: &mut App) -> &mut widgets::Overlay {
    app.widgets.as_mut().expect("a widget scene built the window")
}

/// `App` → the PoE1 item check, for its handlers.
fn project_item_check(app: &mut App) -> &mut poe1::item_check::ItemCheck {
    app.item_check.as_mut().expect("an item-check scene built the window")
}

/// `App` → the PoE2 item check, for its handlers.
fn project_item_check_poe2(app: &mut App) -> &mut poe2::item_check::ItemCheck {
    app.item_check_poe2.as_mut().expect("an item-check scene built the window")
}

/// The app's window: the settings window while it is open, else the item check, else whichever
/// game's price check is open.
pub fn view(app: &App, cx: &mut Cx<App>) -> El {
    // A live window with nothing to draw is hidden by the shell (`shell::Live::sync_window`);
    // this only keeps the frame it was hidden in from drawing a widget.
    if app.live.is_some() && app.shown() == shell::Shown::None {
        return rune::el();
    }
    let card = card(app, cx);
    match app.overlay_offset() {
        // The window is the overlay over the game (Windows, PLAN.md phase 2): the widget is
        // placed inside it where the cursor asked for it, or centred for the tray's windows.
        Some((x, y)) => {
            let mut placed = card.absolute(x, y);
            // The price check fills the overlay (`#price-window` is `h-full`); the other
            // cards keep their own size.
            if let Some((width, height)) = app.overlay_size() {
                placed = placed.w(width).h(height);
            }
            rune::col().w_full().h_full().child(placed)
        }
        None => card,
    }
}

/// The widget the window shows, at its own size.
fn card(app: &App, cx: &mut Cx<App>) -> El {
    // An overlay widget is a card of its own, at its own size; a hidden one shows the window
    // under it (the item search hands the window to the price check its pick opened).
    if let Some(overlay) = &app.widgets
        && overlay.shown_widget().is_some_and(|widget| widget.wm_wants == widgets::Wants::Show)
    {
        return widgets::view(overlay, cx, project_widgets);
    }
    if let Some(settings) = &app.settings
        && settings.open
    {
        return settings::window::view(settings, cx, project_settings);
    }
    // `WidgetItemCheck.vue` is an overlay card of its own, on top of the price check.
    if let Some(check) = &app.item_check
        && check.open
    {
        return poe1::item_check::view(check, cx, project_item_check);
    }
    if let Some(check) = &app.item_check_poe2
        && check.open
    {
        return poe2::item_check::view(check, cx, project_item_check_poe2);
    }
    match app.game {
        GameId::Poe2 => {
            poe2::price_check::view::view(app.poe2.as_ref().expect("a PoE2 scene built the window"), cx, project_poe2)
        }
        _ => poe1::price_check::view::view(&app.price_check, cx, project),
    }
}

/// A named state for `--scene` (`AppSpec::scenes`).
pub type Scene = (&'static str, fn(&mut App));

/// The PoE1 corpus cases the reference screenshots render
/// (`tools/golden/screens/cases.mjs`), plus a text that isn't an item and three cases that
/// exercise the other kinds of filter row: a magic flask (explicit stats), a cluster jewel
/// (enchant and hidden stats, two presets) and an anointed amulet (pseudo stats and the oil
/// row).
pub const SCENES: &[Scene] = &[
    ("currency-divine-orb", |app| app.load("currency-divine-orb")),
    // `tools/golden/screens/cases.mjs` marks these two `search: true`.
    ("adv-rare-ring-mirrored-catalyst", |app| app.load_and_search("adv-rare-ring-mirrored-catalyst")),
    ("adv-unique-timeless-jewel", |app| app.load("adv-unique-timeless-jewel")),
    ("real-map-rare-t16-max-res", |app| app.load_and_search("real-map-rare-t16-max-res")),
    ("error-not-an-item", |app| app.load("error-not-an-item")),
    ("adv-magic-flask-charge-recovery", |app| app.load("adv-magic-flask-charge-recovery")),
    ("jewel-cluster-large", |app| app.load("jewel-cluster-large")),
    ("adv-rare-amulet-anointed", |app| app.load("adv-rare-amulet-anointed")),
    // The item check (`WidgetItemCheck.vue`): the map check for every PoE1 map case, and the
    // item info for a rare, a unique, a gem and a weapon.
    ("map-check-real-map-rare-t16-max-res", |app| app.load_item_check("real-map-rare-t16-max-res")),
    ("map-check-real-map-blighted-corrupted", |app| app.load_item_check("real-map-blighted-corrupted")),
    ("map-check-real-map-valdo-foil", |app| app.load_item_check("real-map-valdo-foil")),
    ("map-check-map-normal-t16", |app| app.load_item_check("map-normal-t16")),
    ("map-check-map-rare-t16-corrupted", |app| app.load_item_check("map-rare-t16-corrupted")),
    ("item-check-adv-rare-ring-mirrored-catalyst", |app| app.load_item_check("adv-rare-ring-mirrored-catalyst")),
    ("item-check-adv-unique-timeless-jewel", |app| app.load_item_check("adv-unique-timeless-jewel")),
    ("item-check-gem-support-empower", |app| app.load_item_check("gem-support-empower")),
    ("item-check-adv-rare-sword-cannot-use", |app| app.load_item_check("adv-rare-sword-cannot-use")),
    // PoE2 (`tools/golden/screens/cases.mjs`'s `poe2` cases), plus its not-an-item text.
    ("poe2-currency-djinn-barya", |app| app.load_poe2("currency-djinn-barya")),
    // The harness clicks Search for these two, as it does for PoE1's rare and map.
    ("poe2-ee2-rare-item", |app| app.load_and_search_poe2("ee2-rare-item")),
    ("poe2-ee2-unique-item", |app| app.load_poe2("ee2-unique-item")),
    ("poe2-ee2-rare-map", |app| app.load_and_search_poe2("ee2-rare-map")),
    ("poe2-error-not-an-item", |app| app.load_poe2("error-not-an-item")),
    // EE2's item check: the waystones (map check) and a slightly priced rare, a unique and an
    // uncut gem (item info).
    ("poe2-map-check-ee2-rare-map", |app| app.load_item_check_poe2("ee2-rare-map")),
    ("poe2-map-check-ee2-rare-map-fake-all-props", |app| app.load_item_check_poe2("ee2-rare-map-fake-all-props")),
    ("poe2-item-check-note-rare-bow", |app| app.load_item_check_poe2("note-rare-bow")),
    ("poe2-item-check-ee2-unique-item", |app| app.load_item_check_poe2("ee2-unique-item")),
    ("poe2-item-check-ee2-uncut-skill-gem", |app| app.load_item_check_poe2("ee2-uncut-skill-gem")),
    // The overlay widgets (`tools/golden/screens/widgets.mjs`): the menu, the stopwatch, the
    // image strip and the delve grid, for both games. A scene is the widget's own card, which is
    // what their screenshots crop.
    ("widget-menu", |app| app.show_widget(GameId::Poe1, widgets::Kind::Menu)),
    ("widget-stopwatch", |app| app.show_widget(GameId::Poe1, widgets::Kind::Stopwatch)),
    ("widget-image-strip", |app| app.show_widget(GameId::Poe1, widgets::Kind::ImageStrip)),
    ("widget-delve-grid", |app| app.show_widget(GameId::Poe1, widgets::Kind::DelveGrid)),
    ("poe2-widget-menu", |app| app.show_widget(GameId::Poe2, widgets::Kind::Menu)),
    ("poe2-widget-stopwatch", |app| app.show_widget(GameId::Poe2, widgets::Kind::Stopwatch)),
    ("poe2-widget-image-strip", |app| app.show_widget(GameId::Poe2, widgets::Kind::ImageStrip)),
    ("poe2-widget-delve-grid", |app| app.show_widget(GameId::Poe2, widgets::Kind::DelveGrid)),
    // The two widgets that are config lists rather than chrome: the stash searches ("Map
    // rolling" and "Dump sorting" come from `defaultConfig()`) and the item search, opened on the
    // query its screenshot types (`tools/golden/screens/widgets.mjs`). APT's data has no
    // alt-quality gems at this pin, so its item search shows a replica instead.
    ("widget-item-search", |app| {
        app.show_widget(GameId::Poe1, widgets::Kind::ItemSearch);
        app.item_search(|state| {
            state.set_tab(widgets::item_search::Tab::Replica);
            state.set_query("headhunter");
        });
    }),
    ("widget-stash-search", |app| app.show_widget(GameId::Poe1, widgets::Kind::StashSearch)),
    ("poe2-widget-item-search", |app| {
        app.show_widget(GameId::Poe2, widgets::Kind::ItemSearch);
        app.item_search(|state| {
            state.set_tab(widgets::item_search::Tab::Unique);
            state.set_query("Mageblood");
        });
    }),
    ("poe2-widget-stash-search", |app| app.show_widget(GameId::Poe2, widgets::Kind::StashSearch)),
    // The settings window, one scene per menu page, for both games.
    ("settings-poe1-hotkeys", |app| app.settings_page(GameId::Poe1, Page::Hotkeys)),
    ("settings-poe1-chat", |app| app.settings_page(GameId::Poe1, Page::Chat)),
    ("settings-poe1-general", |app| app.settings_page(GameId::Poe1, Page::General)),
    ("settings-poe1-price-check", |app| app.settings_page(GameId::Poe1, Page::PriceCheck)),
    ("settings-poe1-maps", |app| app.settings_page(GameId::Poe1, Page::Maps)),
    ("settings-poe1-item-check", |app| app.settings_page(GameId::Poe1, Page::ItemCheck)),
    ("settings-poe1-debug", |app| app.settings_page(GameId::Poe1, Page::Debug)),
    ("settings-poe1-about", |app| app.settings_page(GameId::Poe1, Page::About)),
    ("settings-poe2-hotkeys", |app| app.settings_page(GameId::Poe2, Page::Hotkeys)),
    ("settings-poe2-chat", |app| app.settings_page(GameId::Poe2, Page::Chat)),
    ("settings-poe2-general", |app| app.settings_page(GameId::Poe2, Page::General)),
    ("settings-poe2-price-check", |app| app.settings_page(GameId::Poe2, Page::PriceCheck)),
    ("settings-poe2-maps", |app| app.settings_page(GameId::Poe2, Page::Maps)),
    ("settings-poe2-item-check", |app| app.settings_page(GameId::Poe2, Page::ItemCheck)),
    ("settings-poe2-debug", |app| app.settings_page(GameId::Poe2, Page::Debug)),
    ("settings-poe2-about", |app| app.settings_page(GameId::Poe2, Page::About)),
];

/// The fixture corpus a scene loads (`tools/golden/corpus/poe1/<slug>.txt`).
pub fn corpus(slug: &str) -> String {
    let path = format!("{CORPUS}/{slug}.txt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tools/golden/corpus/poe1");

/// The PoE2 fixture corpus (`tools/golden/corpus/poe2/<slug>.txt`).
pub fn corpus_poe2(slug: &str) -> String {
    let path = format!("{CORPUS_POE2}/{slug}.txt");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

const CORPUS_POE2: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tools/golden/corpus/poe2");

/// A `settings-poe1-maps` style scene slug: the game and the page.
pub fn settings_scene(slug: &str) -> Option<(GameId, Page)> {
    let rest = slug.strip_prefix("settings-")?;
    let (game, page) = rest.split_once('-')?;
    let game = match game {
        "poe1" => GameId::Poe1,
        "poe2" => GameId::Poe2,
        _ => return None,
    };
    Some((game, Page::from_slug(page)?))
}

/// The window size a `--scene` renders at. A settings scene is the settings window: upstream's
/// `.window` is at most 50 rem wide and 38 rem tall (`settings::WIDTH`/`HEIGHT`), so it renders
/// at that size and needs no `--size`. A price check keeps the price-check window's size, and
/// an explicit `--size` still wins.
pub fn scene_size(scene: Option<&str>) -> (f32, f32) {
    if let Some((game, kind)) = scene.and_then(widgets::scene) {
        return widgets::scene_size(game, kind);
    }
    match scene.and_then(settings_scene) {
        Some(_) => (settings::WIDTH, settings::HEIGHT),
        None => (WIDTH, HEIGHT),
    }
}

/// Runs the app: a window, or a headless render with Rune's `--snap`/`--scene`/`--tree`.
pub fn main() {
    // Read `--game` and `--scene` before the window exists: the game picks the config and the
    // data, and a settings scene is a window of its own size. Rune parses the same flags again
    // when it builds the window.
    let args = rune::tooling::Args::parse();
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let game = shell::game_from_args(&argv);
    // Where the data, images and tray icon come from: the packaged `assets/` directory, or
    // the reference checkouts in development (`wraex_core::assets`). Resolved here, once.
    eprintln!("wraex: assets {}", wraex_core::assets::Assets::get().summary());
    // A live run is the app proper: tray, hotkeys, a window on demand. Everything else — a
    // scene, a snapshot, a scripted run — is the port's screenshot mode, unchanged.
    if !rune::is_headless() && args.scene.is_none() {
        shell::run(game);
        return;
    }
    let _ = game;
    let size = scene_size(args.scene.as_deref());
    rune::run(AppSpec::<App> {
        scenes: SCENES,
        fonts: theme::FONTS,
        families: theme::FAMILIES,
        // A live search's answer lands from a worker thread; the waker runs `poll`, which
        // applies it and redraws. Replay answers land in the handler, so `poll` is idle.
        setup: Some(|app, waker| app.set_waker(waker)),
        poll: Some(|app, clock, tx| app.poll_with(clock, tx)),
        // The window is exactly the widget: the price check is a fixed 28.75 rem wide, so the
        // window must not shrink past it. (As an overlay it won't be resizable at all.)
        min_size: Some(size),
        ..AppSpec::new("Wraex", size, App::default, view)
    });
}

#[cfg(test)]
#[path = "widgets/tests.rs"]
mod widget_tests;

#[cfg(test)]
mod tests {
    use rune::runtime::ElementInfo;
    use rune::text::TextSystem;
    use rune::{Env, Runtime};

    use super::*;

    fn runtime(slug: &str) -> Runtime<App> {
        // A `settings-<game>-<page>` scene opens the settings window; a `poe2-` scene opens
        // PoE2's window on its own corpus (`poe2-ee2-rare-item` →
        // `corpus/poe2/ee2-rare-item.txt`); everything else is PoE1's, as before.
        let size = scene_size(Some(slug));
        if let Some((game, kind)) = widgets::scene(slug) {
            return runtime_app(App::widgets_scene(game, kind), size);
        }
        if let Some((game, page)) = settings_scene(slug) {
            return runtime_app(App::settings_scene(game, page), size);
        }
        if let Some(case) = slug.strip_prefix("poe2-map-check-").or_else(|| slug.strip_prefix("poe2-item-check-")) {
            return runtime_app(App::item_check_scene(GameId::Poe2, case), size);
        }
        if let Some(case) = slug.strip_prefix("map-check-").or_else(|| slug.strip_prefix("item-check-")) {
            return runtime_app(App::item_check_scene(GameId::Poe1, case), size);
        }
        match slug.strip_prefix("poe2-") {
            Some(case) => runtime_app(App::with_mode_poe2(&corpus_poe2(case), Mode::Replay), size),
            None => runtime_app(App::replay(slug), size),
        }
    }

    /// A `map-check-<case>` / `item-check-<case>` scene (with either game's prefix).
    fn is_item_check_scene(slug: &str) -> bool {
        ["map-check-", "item-check-"].iter().any(|prefix| slug.starts_with(prefix))
            || ["poe2-map-check-", "poe2-item-check-"].iter().any(|prefix| slug.starts_with(prefix))
    }

    fn runtime_app(app: App, size: (f32, f32)) -> Runtime<App> {
        let text = TextSystem::new(theme::FONTS, theme::FAMILIES);
        let mut runtime = Runtime::with_text(app, view, size, Env::new(true, 1.0), text);
        // `AppSpec::poll` in a window; the tests drive frames themselves.
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

    /// `fixtures/<game>/<case>/item-check.json`: what the reference's item check opens and sends.
    fn item_check_fixture(game: &str, case: &str) -> serde_json::Value {
        let path = format!("{}/../../../fixtures/{game}/{case}/item-check.json", env!("CARGO_MANIFEST_DIR"));
        let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
        serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {path}: {e}"))
    }

    /// Let animated values (a decision row's colour) land: the first frame only starts them.
    fn settle(runtime: &mut Runtime<App>) {
        for _ in 0..40 {
            runtime.frame(1.0 / 60.0);
        }
    }

    fn click(runtime: &mut Runtime<App>, key: &str) {
        let (x, y) = runtime.find(key).unwrap_or_else(|| panic!("no element keyed `{key}`"));
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);
    }

    /// Focus a text field, select its contents and paste `text` over them.
    fn replace_input(runtime: &mut Runtime<App>, key: &str, text: &str) {
        click(runtime, key);
        runtime.key(rune::KeyEvent { key: rune::Key::Command('a'), mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        runtime.paste(text);
        runtime.frame(1.0 / 60.0);
    }

    /// The window frame, in every scene: a title bar over `bg-gray-800`.
    #[test]
    fn every_scene_has_the_title_bar() {
        // With no `--scene`, the window opens on the first scene.
        assert!(App::replay(SCENES[0].0).price_check.item.is_ok());
        for (slug, _) in SCENES {
            let runtime = runtime(slug);
            if slug.starts_with("settings-") {
                let title = if slug.starts_with("settings-poe2-") {
                    "Settings - Exiled Exchange 2"
                } else {
                    "Settings - Awakened PoE Trade"
                };
                assert_eq!(text_of(&runtime, "titlebar-title").as_deref(), Some(title), "{slug}");
                assert!(runtime.find("titlebar-close").is_some(), "{slug}: close button");
            } else if is_item_check_scene(slug) {
                // `WidgetItemCheck.vue` is an overlay card: upstream's overlay draws no title bar
                // over it either (its hover actions panel is not ported).
                assert!(runtime.find("titlebar").is_none(), "{slug}: no title bar");
            } else if widgets::scene(slug).is_some() {
                // An overlay widget is a card on the game's panel, with no title bar of its own.
                assert!(runtime.find("titlebar").is_none(), "{slug}: no title bar");
            } else {
                assert_eq!(text_of(&runtime, "titlebar-title").as_deref(), Some("Standard"), "{slug}");
                assert!(runtime.find("titlebar-close").is_some(), "{slug}: close button");
            }
            assert!(!runtime.elements().is_empty());
        }
    }

    /// A settings scene renders in the settings window's own size, so `--snap --scene
    /// settings-…` needs no `--size`; a price-check scene keeps the price-check window's.
    #[test]
    fn a_settings_scene_is_as_large_as_the_settings_window() {
        assert_eq!(settings::WIDTH, 800.0, "`max-width: 50rem`");
        assert_eq!(settings::HEIGHT, 608.0, "`max-height: 38rem`");
        assert_eq!(scene_size(Some("settings-poe1-hotkeys")), (settings::WIDTH, settings::HEIGHT));
        assert_eq!(scene_size(Some("settings-poe2-about")), (settings::WIDTH, settings::HEIGHT));
        assert_eq!(scene_size(Some("currency-divine-orb")), (WIDTH, HEIGHT));
        assert_eq!(scene_size(None), (WIDTH, HEIGHT));

        // The window fills the scene, which is what `--size 900x700` used to centre in a
        // larger canvas.
        let runtime = runtime("settings-poe1-hotkeys");
        let elements = runtime.elements();
        let window = element(&elements, "settings-window").expect("the settings window");
        assert_eq!([window.rect[0], window.rect[1]], [0.0, 0.0]);
        assert_eq!([window.rect[2], window.rect[3]], [settings::WIDTH, settings::HEIGHT]);
    }

    /// A stackable currency: it searches straight away (`filters.preset_bulk`), so the
    /// exchange table stands in for the Search row.
    #[test]
    fn the_divine_orb_shows_its_exchange_table() {
        let runtime = runtime("currency-divine-orb");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Divine Orb"));
        // `PriceTrend.vue`: poe.ninja's 890.9 chaos, rounded for the row, with its graph.
        assert_eq!(text_of(&runtime, "trend-price").as_deref(), Some("891"));
        assert_eq!(text_of(&runtime, "trend-times").as_deref(), Some(" \u{d7}"));
        assert_eq!(text_of(&runtime, "trend-change").as_deref(), Some("6\u{2009}%"));
        assert_eq!(text_of(&runtime, "trend-change-label").as_deref(), Some("Last 7 days"));
        assert!(runtime.find("trend-sparkline-chart").is_some(), "the 7-day sparkline");
        assert!(runtime.find("search-label").is_none(), "no Search button once `doSearch`");
        // `Matched:` counts the offers per currency: chaos was asked for, divine was not.
        assert_eq!(text_of(&runtime, "trade-matched-label").as_deref(), Some("Matched:"));
        assert_eq!(text_of(&runtime, "bulk-currency-chaos-total").as_deref(), Some("9"));
        assert_eq!(text_of(&runtime, "bulk-currency-divine-total").as_deref(), Some("?"));
        // The recorded exchange answer's first and last offers, cheapest first.
        assert_eq!(text_of(&runtime, "bulk-0-price").as_deref(), Some("800"));
        assert_eq!(text_of(&runtime, "bulk-0-exchange-left").as_deref(), Some("800"));
        assert_eq!(text_of(&runtime, "bulk-0-date").as_deref(), Some("2 hr. ago"));
        // `useMarketRatioFinder`'s row sits at its price: 890.9 chaos per divine, between
        // the 880 and 900 chaos offers.
        assert_eq!(text_of(&runtime, "bulk-3-market-ratio").as_deref(), Some("Market Ratio"));
        assert_eq!(text_of(&runtime, "bulk-3-price").as_deref(), Some("890.9"));
        assert_eq!(text_of(&runtime, "bulk-3-exchange-left").as_deref(), Some("890.9"));
        assert_eq!(text_of(&runtime, "bulk-4-price").as_deref(), Some("900"));
        assert_eq!(text_of(&runtime, "bulk-9-price").as_deref(), Some("1800"));
        assert!(runtime.find("bulk-10-price").is_none(), "the rest of the table is empty");
        // `StackValue.vue`: the stack on screen, and one full stack of twenty.
        assert_eq!(text_of(&runtime, "stack-value-have-price").as_deref(), Some("2673 chaos"));
        assert_eq!(text_of(&runtime, "stack-value-stack-price").as_deref(), Some("17818 chaos"));
        // Each reading takes half the box (`w-1/2`), so the wide gap is the left half's
        // leftover; and the arrow keeps a space's width on both sides (`px-1` + the template's
        // spaces), so the price is not glued to the amount.
        let elements = runtime.elements();
        let box_ = element(&elements, "stack-value").expect("the box");
        let have = element(&elements, "stack-value-have").expect("the left half");
        let stack = element(&elements, "stack-value-stack").expect("the right half");
        assert_eq!(have.rect[2], stack.rect[2], "two equal halves");
        assert!(stack.rect[0] >= box_.rect[0] + box_.rect[2] / 2.0, "{:?} vs {:?}", stack.rect, box_.rect);
        let amount = element(&elements, "stack-value-have-amount").expect("the amount");
        let price = element(&elements, "stack-value-have-price").expect("the price");
        let gap = price.rect[0] - (amount.rect[0] + amount.rect[2]);
        assert!(gap >= 20.0, "the arrow needs its 8 pt on each side, not {gap} pt");
    }

    /// `PriceCheckWindow.vue`'s exchange-rate button, and the popover it opens.
    #[test]
    fn the_title_bar_shows_the_exchange_rate() {
        let mut runtime = runtime("currency-divine-orb");
        assert_eq!(text_of(&runtime, "titlebar-exchange-rate").as_deref(), Some("891"));
        assert!(runtime.find("exchange-popover").is_none(), "closed until pressed");
        click(&mut runtime, "titlebar-exchange");
        assert!(runtime.state.price_check.exchange_open);
        assert_eq!(text_of(&runtime, "exchange-price").as_deref(), Some("891"));
        assert_eq!(text_of(&runtime, "exchange-1").as_deref(), Some("0.1 div \u{21d2} 89 c"));
        assert_eq!(text_of(&runtime, "exchange-9").as_deref(), Some("0.9 div \u{21d2} 802 c"));
        click(&mut runtime, "titlebar-exchange");
        assert!(runtime.find("exchange-popover").is_none());
    }

    /// A rare searches by category and waits for the button; pressing it lands the
    /// recorded answer's twenty rows.
    #[test]
    fn a_rare_ring_labels_its_category_and_offers_search() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Category: Ring"));
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Not Corrupted"));
        assert_eq!(text_of(&runtime, "search-label").as_deref(), Some("Search"));
        assert!(runtime.find("trade-table").is_none());

        click(&mut runtime, "search");
        assert!(runtime.state.price_check.do_search);
        assert!(runtime.state.price_check.trade.error.is_none(), "{:?}", runtime.state.price_check.trade.error);
        assert!(runtime.find("trade-table").is_some());
        assert_eq!(text_of(&runtime, "trade-matched-total").as_deref(), Some("10000"));
        assert_eq!(text_of(&runtime, "listing-0-price").as_deref(), Some("1 chaos"));
        assert_eq!(text_of(&runtime, "listing-0-ilvl").as_deref(), Some("84"));
        assert_eq!(text_of(&runtime, "listing-0-date").as_deref(), Some("2 days ago"));
        assert_eq!(text_of(&runtime, "listing-19-price").as_deref(), Some("3 chaos"));
        assert!(runtime.find("listing-20-price").is_none(), "twenty rows, no more");
    }

    /// A unique is searched by name, and has the price row and the disenchant value.
    #[test]
    fn the_timeless_jewel_shows_its_unique_name() {
        let runtime = runtime("adv-unique-timeless-jewel");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Lethal Pride"));
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Not Corrupted"));
        assert_eq!(text_of(&runtime, "trend-price").as_deref(), Some("99"));
        assert_eq!(text_of(&runtime, "trend-change").as_deref(), Some("24\u{2009}%"));
        assert_eq!(text_of(&runtime, "dust-value-amount").as_deref(), Some("36,225"));
        // `DustValue.vue`'s `<img src="/images/dust.png" class="w-5 -my-5 inline-block">`.
        let elements = runtime.elements();
        let dust = element(&elements, "dust-value-icon").expect("the dust icon");
        assert_eq!(dust.role, "img");
        assert_eq!(dust.label.as_deref(), Some("/images/dust.png"));
        assert_eq!([dust.rect[2], dust.rect[3]], [20.0, 20.0], "`w-5`");
        assert!(runtime.find("search-label").is_none());
    }

    /// `PriceTrend.vue`'s "Base item" slot: a rare ring prices by its base type.
    #[test]
    fn the_rare_ring_shows_its_base_types_trend() {
        let runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert_eq!(text_of(&runtime, "trend-base-item").as_deref(), Some("Base item"));
        assert_eq!(text_of(&runtime, "trend-price").as_deref(), Some("3"));
        // A flat graph: `±` and no chevrons.
        assert_eq!(text_of(&runtime, "trend-change-sign").as_deref(), Some("\u{b1}"));
        assert_eq!(text_of(&runtime, "trend-change").as_deref(), Some("0\u{2009}%"));
    }

    /// A map poe.ninja has no entry for, and which is craftable: it gets no trend row at
    /// all (`showFallback` is false).
    #[test]
    fn a_map_has_no_trend_row() {
        let runtime = runtime("real-map-rare-t16-max-res");
        assert!(runtime.find("trend-price").is_none());
        assert!(runtime.find("trend").is_none());
    }

    /// A map has no name to search by: `FilterName` shows its category.
    #[test]
    fn a_map_searches_by_category() {
        let runtime = runtime("real-map-rare-t16-max-res");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Map"));
        assert_eq!(text_of(&runtime, "search-label").as_deref(), Some("Search"));
    }

    /// Text that isn't an item: the error box, then the raw block.
    #[test]
    fn a_text_that_is_not_an_item_shows_the_error() {
        let runtime = runtime("error-not-an-item");
        assert_eq!(text_of(&runtime, "error-name").as_deref(), Some("An error occurred while parsing the item"));
        assert_eq!(
            text_of(&runtime, "error-message").as_deref(),
            Some("This is probably a bug and you can report it on GitHub.")
        );
        let raw = text_of(&runtime, "error-raw-text").expect("the raw text block");
        assert!(raw.contains("This is not an item"), "{raw}");
        assert!(runtime.find("filter-name-text").is_none());
    }

    /// `FilterName.vue`'s two toggles run on the state.
    #[test]
    fn the_item_name_toggles_the_corruption_filter() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert!(!runtime.state.price_check.item_filters().unwrap().corrupted.as_ref().unwrap().value);
        click(&mut runtime, "corrupted");
        assert!(runtime.state.price_check.item_filters().unwrap().corrupted.as_ref().unwrap().value);
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Corrupted"));
    }

    /// The close button hides the widget, as `wm.hide` does upstream.
    #[test]
    fn closing_hides_the_window() {
        let mut runtime = runtime("currency-divine-orb");
        click(&mut runtime, "titlebar-close");
        assert!(!runtime.state.price_check.open);
        assert!(runtime.find("titlebar").is_none());
    }

    /// `FiltersBlock.vue` for the rare ring: its three visible rows, with upstream's labels
    /// and the rolls `ItemModifierText.vue` substitutes.
    #[test]
    fn the_rare_ring_lists_its_stat_rows() {
        let runtime = runtime("adv-rare-ring-mirrored-catalyst");
        // Stats 0, 2 and 3 are the visible rows; 1 (Fire Resistance) and 4 (Quality) are hidden.
        let expected = [
            (0, "94% total Elemental Resistance"),
            (2, "98 total maximum Life"),
            (3, "25% Global Critical Strike Chance"),
        ];
        for (index, label) in expected {
            assert_eq!(text_of(&runtime, &format!("stat-{index}-text")).as_deref(), Some(label), "row {index}");
        }
        assert!(runtime.find("stat-1").is_none());
        let stats = runtime.state.price_check.item_stats();
        let rolls: Vec<Option<f64>> = stats
            .iter()
            .filter_map(|stat| match stat {
                wraex_poe1::filters::FilterOrGroup::Stat(filter) => Some(filter.roll.as_ref().map(|roll| roll.value)),
                wraex_poe1::filters::FilterOrGroup::Group(_) => None,
            })
            .collect();
        assert_eq!(rolls, [Some(94.0), Some(66.0), Some(98.0), Some(25.0), Some(20.0)]);
        // The hidden Fire Resistance and Quality rows stay out of the list until `Hidden`.
        assert!(runtime.find("stat-1").is_none());
        assert!(runtime.find("stat-4").is_none());
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("2 of 5, stats"));
    }

    /// `FilterModifier.vue`'s checkbox writes `filter.disabled`.
    #[test]
    fn a_stat_row_toggles_its_filter() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        let disabled = |runtime: &Runtime<App>, index: usize| match &runtime.state.price_check.item_stats()[index] {
            wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.disabled,
            wraex_poe1::filters::FilterOrGroup::Group(group) => group.meta.disabled,
        };
        assert!(disabled(&runtime, 3));
        click(&mut runtime, "stat-3-toggle");
        assert!(!disabled(&runtime, 3));
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("3 of 5, stats"));
        click(&mut runtime, "stat-3-toggle");
        assert!(disabled(&runtime, 3));
    }

    /// Typing in a row's min field parses the number back into `filter.roll.min`.
    #[test]
    fn a_stat_row_parses_its_min_field() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert_eq!(
            runtime.state.price_check.item_stats().first().and_then(|stat| match stat {
                wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.roll.as_ref().and_then(|roll| roll.min),
                wraex_poe1::filters::FilterOrGroup::Group(_) => None,
            }),
            Some(84.0)
        );
        replace_input(&mut runtime, "stat-0-min", "100");
        let roll = match &runtime.state.price_check.item_stats()[0] {
            wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.roll.clone().expect("a roll"),
            wraex_poe1::filters::FilterOrGroup::Group(_) => panic!("a stat row"),
        };
        assert_eq!(roll.min, Some(100.0));
        assert!(!runtime.state.price_check.item_stats().first().is_some_and(|stat| match stat {
            wraex_poe1::filters::FilterOrGroup::Stat(filter) => filter.disabled,
            wraex_poe1::filters::FilterOrGroup::Group(_) => true,
        }));
        // Enter submits the form (`@submit` → `doSearch`).
        click(&mut runtime, "stat-0-max");
        runtime.key(rune::KeyEvent { key: rune::Key::Enter, mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.price_check.do_search);
    }

    /// `FiltersBlock.vue`'s preset tabs replace the rows.
    #[test]
    fn switching_preset_replaces_the_rows() {
        let mut runtime = runtime("real-map-rare-t16-max-res");
        assert_eq!(text_of(&runtime, "stat-0-text").as_deref(), Some("Atlas Drops: 67%"));
        assert!(runtime.find("stat-0").is_some());
        click(&mut runtime, "preset-1");
        assert_eq!(
            runtime.state.price_check.active_preset().map(|preset| preset.id.as_str()),
            Some("filters.preset_bulk")
        );
        assert!(runtime.find("stat-0").is_none());
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("Stats ignored"));
        click(&mut runtime, "preset-0");
        assert_eq!(text_of(&runtime, "stat-0-text").as_deref(), Some("Atlas Drops: 67%"));
    }

    /// Collapse hides the rows (the chip and the button both toggle it) and Hidden shows the
    /// rows upstream hides for a reason.
    #[test]
    fn collapse_and_hidden_switch_what_the_block_lists() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        click(&mut runtime, "filters-collapse");
        assert!(runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("stat-0").is_none());
        click(&mut runtime, "btn-stats");
        assert!(!runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("stat-0").is_some());

        assert!(runtime.find("stat-1").is_none());
        click(&mut runtime, "show-hidden");
        assert!(runtime.state.price_check.show_hidden);
        assert_eq!(text_of(&runtime, "stat-1-text").as_deref(), Some("66% total to Fire Resistance"));
        assert_eq!(text_of(&runtime, "stat-4-text").as_deref(), Some("Quality (Life and Mana Modifiers): 20%"));
    }

    /// The Mods toggle shows `SourceInfo.vue` as an overlay while a row whose sources
    /// deserve it is hovered (`FilterModifier.vue`'s `.mods`).
    #[test]
    fn the_mods_toggle_shows_a_rows_sources_on_hover() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        assert!(runtime.find("source-info-mod").is_none());
        click(&mut runtime, "show-sources");
        assert!(runtime.state.price_check.show_sources);
        assert!(runtime.find("source-info-mod").is_none(), "nothing while the pointer is away");

        let (x, y) = runtime.find("stat-0").expect("the first row");
        runtime.pointer_move(x, y);
        runtime.frame(1.0 / 60.0);
        assert_eq!(runtime.state.price_check.source_hover, Some(crate::poe1::price_check::StatPath::Top(0)));
        assert!(runtime.find("stat-0-sources").is_some(), "the overlay under the hovered row");
        assert!(runtime.find("source-info-mod").is_some());

        runtime.pointer_move(2.0, 2.0);
        runtime.frame(1.0 / 60.0);
        assert!(runtime.state.price_check.source_hover.is_none());
        assert!(runtime.find("source-info-mod").is_none());
    }

    /// `FilterGroup.vue`: the chevron collapses the group and its label's `smartToggle`
    /// closes an open, enabled group.
    #[test]
    fn a_group_expands_and_collapses() {
        let mut runtime = runtime("adv-unique-timeless-jewel");
        // The group sits at index 1 (a hidden `explicit` row comes first).
        assert!(runtime.find("stat-1-0").is_some());
        click(&mut runtime, "stat-1m-expand");
        assert!(!group_expanded(&runtime, 1));
        assert!(runtime.find("stat-1-0").is_none());
        click(&mut runtime, "stat-1m-expand");
        assert!(group_expanded(&runtime, 1));

        // Its label is enabled and open, so the first press only closes it.
        click(&mut runtime, "stat-1m-label");
        assert!(!group_expanded(&runtime, 1));
        assert!(!group_disabled(&runtime, 1));
        // Pressing it again turns the group off; a third press turns it back on and opens it.
        click(&mut runtime, "stat-1m-label");
        assert!(group_disabled(&runtime, 1));
        assert!(!group_expanded(&runtime, 1));
        click(&mut runtime, "stat-1m-label");
        assert!(!group_disabled(&runtime, 1));
        assert!(group_expanded(&runtime, 1));
    }

    fn group_expanded(runtime: &Runtime<App>, index: usize) -> bool {
        match &runtime.state.price_check.item_stats()[index] {
            wraex_poe1::filters::FilterOrGroup::Group(group) => group.expanded,
            wraex_poe1::filters::FilterOrGroup::Stat(_) => panic!("a group"),
        }
    }

    fn group_disabled(runtime: &Runtime<App>, index: usize) -> bool {
        match &runtime.state.price_check.item_stats()[index] {
            wraex_poe1::filters::FilterOrGroup::Group(group) => group.meta.disabled,
            wraex_poe1::filters::FilterOrGroup::Stat(_) => panic!("a group"),
        }
    }

    /// `FiltersBlock.vue` passes `statsVisibility.disabled` as the stats chip's `collapse`,
    /// and `FilterBtnLogical.vue` only draws an icon when it is truthy.
    #[test]
    fn the_stats_chip_draws_its_chevron_only_when_collapsed() {
        let mut runtime = runtime("real-map-rare-t16-max-res");
        assert!(runtime.find("btn-stats-chevron").is_none());
        click(&mut runtime, "btn-stats");
        assert!(runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("btn-stats-chevron").is_some());
        // The Collapse button is inside the block the chip just hid; the chip brings it back.
        click(&mut runtime, "btn-stats");
        assert!(!runtime.state.price_check.stats_collapsed);
        assert!(runtime.find("btn-stats-chevron").is_none());
    }

    /// `{{ t('filters.collapse') }} <i>`: a space, then the chevron.
    #[test]
    fn collapse_keeps_a_space_before_its_chevron() {
        let runtime = runtime("real-map-rare-t16-max-res");
        assert_eq!(text_of(&runtime, "filters-collapse-label").as_deref(), Some("Collapse "));
    }

    /// `FilterBtnNumeric.vue`'s `.input` is transparent: the chip draws the background and
    /// border, so a value like `Map Tier: 16` is plain text, not a box of its own.
    #[test]
    fn a_numeric_chip_draws_its_value_without_a_box() {
        let runtime = runtime("real-map-rare-t16-max-res");
        let elements = runtime.elements();
        let field = element(&elements, "btn-map_tier-min").expect("the map tier field");
        assert_eq!(field.text.as_deref(), Some("16"));
        assert_eq!(field.fill, rune::Color::TRANSPARENT, "the chip's background shows through");
        assert!((field.rect[3] - 20.0).abs() < 0.5, "inside the chip's 22 pt box: {:?}", field.rect);
        // A stat row's roll keeps `FilterModifier.vue`'s gray-900 box.
        let roll = element(&elements, "stat-0-max").expect("the first row's max field");
        assert_eq!(roll.fill, theme::gray(900));
    }

    /// `FilterBtnNumeric.vue`'s `inputMinBlur`: an emptied min field shows 0 and turns its
    /// filter off when the caret goes elsewhere.
    #[test]
    fn an_empty_min_field_shows_zero_and_turns_off_on_blur() {
        use crate::poe1::price_check::NumericFilter;
        let mut runtime = runtime("real-map-rare-t16-max-res");
        click(&mut runtime, "btn-map_tier-min");
        // `poll` records the focused field one frame after the click.
        runtime.frame(1.0 / 60.0);
        runtime.key(rune::KeyEvent { key: rune::Key::Command('a'), mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        runtime.key(rune::KeyEvent { key: rune::Key::Backspace, mods: rune::Mods::default() });
        runtime.frame(1.0 / 60.0);
        assert_eq!(text_of(&runtime, "btn-map_tier-min").as_deref(), Some(""));

        click(&mut runtime, "stat-0-max");
        runtime.frame(1.0 / 60.0);
        assert_eq!(text_of(&runtime, "btn-map_tier-min").as_deref(), Some("0"));
        assert!(runtime.state.price_check.numeric(NumericFilter::MapTier).expect("the chip").disabled);
    }

    /// `OnlineFilter.vue`'s label opens `OnlineFilterCore.vue` under it.
    #[test]
    fn the_online_filter_opens_its_panel() {
        let mut runtime = runtime("adv-rare-ring-mirrored-catalyst");
        click(&mut runtime, "search");
        assert!(runtime.find("online-filter-panel").is_none());
        assert_eq!(text_of(&runtime, "online-filter-label").as_deref(), Some("Merchant Only"));

        click(&mut runtime, "online-filter");
        assert!(runtime.state.price_check.online_open);
        let label = runtime.find("online-filter").expect("the label");
        let panel = runtime.find("online-filter-panel").expect("the panel");
        assert!(panel.1 > label.1, "under the label: {panel:?} vs {label:?}");
        assert_eq!(text_of(&runtime, "online-filter-merchant-label").as_deref(), Some("Merchant Only"));
        assert_eq!(text_of(&runtime, "online-filter-listed-1day-label").as_deref(), Some("1 Day Ago"));

        click(&mut runtime, "online-filter");
        assert!(runtime.find("online-filter-panel").is_none());
    }

    /// The extra scenes render their filter kinds: a magic flask's explicit rows, a cluster
    /// jewel's enchant rows and two presets, and an amulet's pseudo rows.
    #[test]
    fn the_extra_scenes_render_their_filter_rows() {
        let flask = runtime("adv-magic-flask-charge-recovery");
        assert_eq!(text_of(&flask, "stat-0-text").as_deref(), Some("51% increased Charge Recovery"));
        assert_eq!(text_of(&flask, "stat-1-text").as_deref(), Some("#% reduced effect"));

        let jewel = runtime("jewel-cluster-large");
        assert_eq!(text_of(&jewel, "stat-0-text").as_deref(), Some("Adds 8 Passive Skills"));
        assert!(jewel.find("preset-1").is_some());

        let amulet = runtime("adv-rare-amulet-anointed");
        assert_eq!(text_of(&amulet, "stat-0-text").as_deref(), Some("77% total Elemental Resistance"));
        assert!(amulet.find("show-hidden").is_some());
    }

    /// The images `PriceTrend.vue` and `ItemQuickPrice.vue` draw: the item's art from the
    /// CDN (replayed from the screenshot harness's recordings) and the chaos/divine icons
    /// from the reference's `public/images`, at the sizes the templates give them.
    #[test]
    fn a_scene_shows_the_item_art_and_the_currency_icons() {
        let orb = runtime("currency-divine-orb");
        let elements = orb.elements();
        let art = element(&elements, "trend-item-icon").expect("the divine orb's art");
        assert_eq!(art.role, "img", "a decoded image, not the placeholder");
        assert!(
            art.label.as_deref().is_some_and(|label| label.starts_with("https://web.poecdn.com/")),
            "{:?}",
            art.label
        );
        assert_eq!([art.rect[2], art.rect[3]], [32.0, 32.0], "`w-8 h-8`");

        // `ItemQuickPrice.vue`'s `isValuable`: a chaos price gets `chaos.png`.
        let currency = element(&elements, "trend-currency-icon").expect("the price's currency");
        assert_eq!(currency.role, "img");
        assert_eq!(currency.label.as_deref(), Some("/images/chaos.png"));
        assert_eq!([currency.rect[2], currency.rect[3]], [32.0, 32.0]);

        // `TradeBulk.vue`'s two totals: `chaos.png` and `divine.png`.
        assert_eq!(
            element(&elements, "bulk-currency-chaos-icon").and_then(|icon| icon.label.as_deref()),
            Some("/images/chaos.png")
        );
        assert_eq!(
            element(&elements, "bulk-currency-divine-icon").and_then(|icon| icon.label.as_deref()),
            Some("/images/divine.png")
        );

        // The title bar's exchange-rate popover shows a divine beside the chaos rate.
        let mut popped = runtime("currency-divine-orb");
        click(&mut popped, "titlebar-exchange");
        let elements = popped.elements();
        assert_eq!(
            element(&elements, "exchange-item-icon").and_then(|icon| icon.label.as_deref()),
            Some("/images/divine.png")
        );
    }

    /// `FilterModifier.vue`'s `rollTags` and the reference's influence badges: local art, so
    /// the mirror's `ReflectingMist.png` and life's `increased.png` are drawn beside the rows.
    #[test]
    fn the_filter_rows_draw_their_roll_markers() {
        let runtime = runtime("adv-rare-ring-mirrored-catalyst");
        let elements = runtime.elements();
        let labels: Vec<&str> =
            elements.iter().filter(|e| e.role == "img").filter_map(|e| e.label.as_deref()).collect();
        assert!(labels.contains(&"/images/ReflectingMist.png"), "{labels:?}");
        assert!(labels.contains(&"/images/increased.png"), "{labels:?}");
    }

    /// `FilterModifierAnointment.vue`: the amulet's anointment is a hidden row, and once the
    /// Hidden toggle shows it, its three oils carry their own art (`w-8 h-8`).
    #[test]
    fn the_anointment_row_draws_its_oils() {
        let mut runtime = runtime("adv-rare-amulet-anointed");
        assert!(runtime.find("anointment-oils").is_none(), "the row is hidden by default");
        click(&mut runtime, "show-hidden");
        let elements = runtime.elements();
        for index in 0..3 {
            let oil = element(&elements, &format!("anointment-oil-{index}")).expect("an oil's art");
            assert_eq!(oil.role, "img", "oil {index}");
            assert!(oil.label.as_deref().is_some_and(|label| label.starts_with("https://web.poecdn.com/")), "{index}");
            assert_eq!([oil.rect[2], oil.rect[3]], [32.0, 32.0], "`w-8 h-8`");
        }
        assert!(element(&elements, "anointment-oil-3").is_none(), "three oils");
    }

    /// A CDN URL that was never recorded is the dark placeholder square: a replay never goes
    /// to the network, and the window keeps its layout while the picture is missing.
    #[test]
    fn an_unrecorded_cdn_image_is_a_placeholder() {
        use rune::{Cx, El};

        // A test process never fetches: the scene's recordings are what it reads.
        ui::art::set_mode(Mode::Replay);

        fn view(_state: &(), _cx: &mut Cx<()>) -> El {
            let url = "https://web.poecdn.com/gen/image/WzI1LDE0/nothing/NoSuchPicture.png";
            rune::el().size(32.0, 32.0).child(ui::art::cdn(GameId::Poe1, url, (32.0, 32.0)).key("missing-image"))
        }
        let text = TextSystem::new(theme::FONTS, theme::FAMILIES);
        let mut runtime = Runtime::with_text((), view, (32.0, 32.0), Env::new(true, 1.0), text);
        runtime.frame(0.0);
        let elements = runtime.elements();
        let missing = element(&elements, "missing-image").expect("the element keeps its place");
        assert_ne!(missing.role, "img", "no picture, so the placeholder");
        assert_eq!(missing.label, None);
        assert_eq!([missing.rect[2], missing.rect[3]], [32.0, 32.0]);
    }

    // ---- Item check / map check ----------------------------------------------------------

    /// `MapCheck.vue` on a rare T16: the map's name, the profile buttons, every explicit stat
    /// with the default profile's decision colours, and the outdated-wording notice the
    /// reference shows for its own default stat list.
    #[test]
    fn the_map_check_lists_a_maps_stats_and_decisions() {
        let runtime = runtime("map-check-real-map-rare-t16-max-res");
        assert_eq!(text_of(&runtime, "map-check-name").as_deref(), Some("Map"));
        assert_eq!(text_of(&runtime, "map-check-profile-label").as_deref(), Some("Profile"));
        assert_eq!(text_of(&runtime, "map-check-profile-1-text").as_deref(), Some("I"));
        for numeral in ["I", "II", "III"] {
            assert!(runtime.find(&format!("map-check-profile-{numeral}")).is_none(), "keyed by number");
        }
        assert!(runtime.find("map-check-profile-3-text").is_some());
        let expected = [
            "Unique Boss deals 25% increased Damage",
            "Unique Boss has 30% increased Attack and Cast Speed",
            "28% increased Monster Movement Speed",
            "35% increased Monster Attack Speed",
            "37% increased Monster Cast Speed",
            "Players have -12% to all maximum Resistances",
            "Players are Cursed with Vulnerability",
        ];
        for (index, label) in expected.iter().enumerate() {
            assert_eq!(text_of(&runtime, &format!("stat-{index}-text")).as_deref(), Some(*label), "row {index}");
        }
        assert!(runtime.find("stat-7").is_none(), "seven rows");
        // `w` is a warning: `bg-orange-600`; an unmarked stat has no background of its own.
        let elements = runtime.elements();
        assert_eq!(element(&elements, "stat-5-cycle").expect("the warned row").fill, theme::orange(600));
        assert_eq!(element(&elements, "stat-0-cycle").expect("a plain row").fill, rune::Color::TRANSPARENT);
        assert_eq!(element(&elements, "stat-5-icon").expect("the warning icon").rect[2], 14.0);
        assert!(runtime.find("stat-0-icon").is_none(), "no icon without a decision");
        assert!(runtime.find("stat-5-seen-icon").is_none(), "`showNewStats` is off by default");
        assert!(runtime.find("map-check-no-mods").is_none());
        let outdated = text_of(&runtime, "map-check-outdated-text").expect("the wording notice");
        assert!(outdated.starts_with("Wording of some stats has been changed."), "{outdated}");
    }

    /// `MapStatButton.vue`: the row cycles its profile's decision and the profile buttons switch
    /// which character counts.
    #[test]
    fn a_map_stat_cycles_its_decision_per_profile() {
        let mut runtime = runtime("map-check-real-map-rare-t16-max-res");
        click(&mut runtime, "stat-5-cycle");
        let maps = &runtime.state.item_check.as_ref().unwrap().maps;
        assert_eq!(
            maps.selected_stats.iter().find(|s| s.matcher.ends_with("maximum Resistances")).unwrap().decision,
            "g--"
        );
        settle(&mut runtime);
        assert_eq!(element(&runtime.elements(), "stat-5-cycle").unwrap().fill, theme::green(700));
        click(&mut runtime, "stat-5-cycle");
        assert_eq!(runtime.state.item_check.as_ref().unwrap().maps.selected_stats[0].decision, "s--");
        settle(&mut runtime);
        assert_eq!(element(&runtime.elements(), "stat-5-cycle").unwrap().fill, rune::Color::TRANSPARENT);

        // Profile II has no decision for that stat, so the row loses its colour.
        click(&mut runtime, "map-check-profile-2");
        assert_eq!(runtime.state.item_check.as_ref().unwrap().maps.profile, 2);
        settle(&mut runtime);
        assert_eq!(element(&runtime.elements(), "stat-5-cycle").unwrap().fill, rune::Color::TRANSPARENT);
        click(&mut runtime, "stat-5-cycle");
        assert_eq!(runtime.state.item_check.as_ref().unwrap().maps.selected_stats[0].decision, "sd-");
        settle(&mut runtime);
        assert_eq!(element(&runtime.elements(), "stat-5-cycle").unwrap().fill, theme::red(700));
    }

    /// `toggleSeenStatus` only acts while the new-mods icon is on; the eye marks a stat seen.
    #[test]
    fn the_eye_marks_a_stat_seen_when_new_stats_are_shown() {
        let mut runtime = runtime("map-check-real-map-rare-t16-max-res");
        click(&mut runtime, "stat-0-seen");
        assert!(
            runtime.state.item_check.as_ref().unwrap().maps.selected_stats.iter().all(|stat| stat.decision != "s--")
        );

        // `showNewStats` is a settings toggle, not a widget control: set it, then press a profile
        // button (the next frame draws the eye) as the settings window would have.
        runtime.state.item_check.as_mut().unwrap().maps.show_new_stats = true;
        click(&mut runtime, "map-check-profile-1");
        assert!(runtime.find("stat-0-seen-icon").is_some(), "the eye for an undecided stat");
        assert!(runtime.find("stat-5-seen-icon").is_none(), "not for a warned one");
        click(&mut runtime, "stat-0-seen");
        let found = runtime
            .state
            .item_check
            .as_ref()
            .unwrap()
            .maps
            .selected_stats
            .iter()
            .find(|stat| stat.matcher == "Unique Boss deals #% increased Damage");
        assert_eq!(found.map(|stat| stat.decision.as_str()), Some("s--"), "a new entry, marked seen");
        runtime.frame(1.0 / 60.0);
        assert!(runtime.find("stat-0-seen-icon").is_none());
    }

    /// A map with a picture shows it (21:9, `object-fit: cover`); a blighted map hides it and
    /// keeps the item's own name; a map with no mods says so.
    #[test]
    fn the_map_check_shows_a_maps_picture_and_its_empty_state() {
        let valdo = runtime("map-check-real-map-valdo-foil");
        assert_eq!(text_of(&valdo, "map-check-name").as_deref(), Some("Beach"));
        let elements = valdo.elements();
        let picture = element(&elements, "map-check-image-picture").expect("the map's screenshot");
        assert_eq!(picture.role, "img");
        assert!(
            picture.label.as_deref().is_some_and(|url| url.starts_with("https://i.imgur.com/")),
            "{:?}",
            picture.label
        );
        let frame = element(&elements, "map-check-image").expect("the 21:9 box");
        assert!((frame.rect[2] / frame.rect[3] - 21.0 / 9.0).abs() < 0.05, "{:?}", frame.rect);
        assert_eq!(text_of(&valdo, "map-check-no-mods-text").as_deref(), Some("Item has no modifiers."));

        // A blighted area has its own (unrecorded) picture and is named after the item.
        let blighted = runtime("map-check-real-map-blighted-corrupted");
        assert_eq!(text_of(&blighted, "map-check-name").as_deref(), Some("Blighted Map"));
        assert!(blighted.find("map-check-image").is_none());
        assert_eq!(text_of(&blighted, "stat-0-text").as_deref(), Some("Area is inhabited by ranged monsters"));
        assert_eq!(text_of(&blighted, "stat-2-text").as_deref(), Some("20% increased Magic Monsters"));
    }

    /// `ItemInfo.vue`: the item's name, the three buttons a mirrored rare gets, and the empty
    /// grey panel under them.
    #[test]
    fn the_item_check_shows_a_rare_rings_actions() {
        let mut runtime = runtime("item-check-adv-rare-ring-mirrored-catalyst");
        assert_eq!(text_of(&runtime, "item-check-name-text").as_deref(), Some("Diamond Ring"));
        assert_eq!(text_of(&runtime, "item-check-wiki-label").as_deref(), Some("wiki"));
        assert_eq!(text_of(&runtime, "item-check-poedb-label").as_deref(), Some("poedb"));
        assert_eq!(text_of(&runtime, "item-check-stash-label").as_deref(), Some("Find in Stash"));
        assert!(runtime.find("item-check-coe").is_none(), "a mirrored item is not craftable");
        assert!(runtime.find("item-check-info").is_some(), "the empty gray-800 panel");
        assert!(runtime.state.item_check.as_ref().unwrap().actions.is_empty(), "nothing runs by itself");

        click(&mut runtime, "item-check-wiki");
        click(&mut runtime, "item-check-poedb");
        click(&mut runtime, "item-check-stash");
        // `fixtures/poe1/adv-rare-ring-mirrored-catalyst/item-check.json`, the reference's own
        // answer for this item.
        let fixture = item_check_fixture("poe1", "adv-rare-ring-mirrored-catalyst");
        assert_eq!(fixture["similar"]["name"].as_str(), Some("CLIENT->MAIN::user-action"));
        let expected = vec![
            poe1::item_check::Action::Open(fixture["wiki"].as_str().expect("the wiki URL").to_owned()),
            poe1::item_check::Action::Open(fixture["poedb"].as_str().expect("the poedb URL").to_owned()),
            poe1::item_check::Action::Send(wraex_poe1::item_check::UserAction {
                name: "CLIENT->MAIN::user-action",
                payload: wraex_poe1::item_check::StashSearch {
                    action: "stash-search",
                    text: fixture["similar"]["payload"]["text"].as_str().expect("the stash search").to_owned(),
                },
            }),
        ];
        assert_eq!(&runtime.state.item_check.as_ref().unwrap().actions, &expected);
    }

    /// A weapon gets its DPS grid and, being craftable, the CoE button, whose URL carries the
    /// whole item text (`hotkeyable-actions.ts` `openCoE`).
    #[test]
    fn the_item_check_shows_a_weapons_dps_and_opens_craft_of_exile() {
        let mut runtime = runtime("item-check-adv-rare-sword-cannot-use");
        assert_eq!(text_of(&runtime, "item-check-name-text").as_deref(), Some("Jewelled Foil"));
        assert_eq!(text_of(&runtime, "item-check-dps-0-label").as_deref(), Some("Physical DPS: "));
        assert_eq!(text_of(&runtime, "item-check-dps-0-value").as_deref(), Some("137"));
        assert_eq!(text_of(&runtime, "item-check-dps-1-value").as_deref(), Some("62"));
        assert_eq!(text_of(&runtime, "item-check-dps-2-value").as_deref(), Some("199"));
        assert_eq!(text_of(&runtime, "item-check-coe-label").as_deref(), Some("CoE"));

        click(&mut runtime, "item-check-coe");
        let fixture = item_check_fixture("poe1", "adv-rare-sword-cannot-use");
        assert_eq!(
            runtime.state.item_check.as_ref().unwrap().actions,
            vec![poe1::item_check::Action::Open(
                fixture["craftOfExile"].as_str().expect("the Craft of Exile URL").to_owned()
            )]
        );
    }

    /// A unique's dust value with its icon, and a gem, which has neither dust nor a craft page.
    #[test]
    fn the_item_check_shows_dust_and_a_gems_actions() {
        let unique = runtime("item-check-adv-unique-timeless-jewel");
        assert_eq!(text_of(&unique, "item-check-name-text").as_deref(), Some("Lethal Pride"));
        assert_eq!(text_of(&unique, "item-check-dust-label").as_deref(), Some("Disenchanting"));
        assert_eq!(text_of(&unique, "item-check-dust-amount").as_deref(), Some("36,225"));
        let elements = unique.elements();
        let icon = element(&elements, "item-check-dust-icon").expect("the dust icon");
        assert_eq!(icon.label.as_deref(), Some("/images/dust.png"));

        let gem = runtime("item-check-gem-support-empower");
        assert_eq!(text_of(&gem, "item-check-name-text").as_deref(), Some("Empower Support"));
        assert!(gem.find("item-check-coe").is_none());
        assert!(gem.find("item-check-dps").is_none());
        assert!(gem.find("item-check-dust").is_none());
    }

    // ---- PoE2 (`ref/ee2`'s price check) ------------------------------------------------

    /// `Exiled Exchange 2`'s window on the Djinn Barya check: the two chips a trial coin
    /// gets, no stats chip, and the trade table the reference searched by itself.
    #[test]
    fn the_poe2_trial_coin_shows_its_area_level_and_listings() {
        let runtime = runtime("poe2-currency-djinn-barya");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Djinn Barya"));
        assert!(runtime.find("corrupted").is_none(), "a currency has no corruption state");
        // `awardedAscendancyPoints` and `areaLevel`, in `FiltersBlock.vue`'s chip order.
        assert_eq!(text_of(&runtime, "btn-ascendancy_points-label").as_deref(), Some("Points:"));
        assert_eq!(text_of(&runtime, "btn-ascendancy_points-min").as_deref(), Some("3"));
        assert_eq!(text_of(&runtime, "btn-area_level-label").as_deref(), Some("Area Level:"));
        assert_eq!(text_of(&runtime, "btn-area_level-min").as_deref(), Some("65"));
        assert!(runtime.find("btn-stats").is_none(), "it has no stat filters");
        // The header's poe.ninja rate, `Math.round(xchgRate)`.
        assert_eq!(text_of(&runtime, "titlebar-rate-text").as_deref(), Some("132"));
        // `listingType: 'securable'` is the reference's `Instant`.
        assert_eq!(text_of(&runtime, "online-filter-label").as_deref(), Some("Instant"));
        assert!(runtime.find("search-label").is_none(), "it searches as soon as it opens");
        assert_eq!(text_of(&runtime, "trade-matched-total").as_deref(), Some("26"));
        // `TradeItem.vue`: the price, with the normalised price in divines where it differs.
        assert_eq!(text_of(&runtime, "listing-0-price").as_deref(), Some("3 divine"));
        assert_eq!(text_of(&runtime, "listing-1-price").as_deref(), Some("11111 exalted (84 div)"));
        assert_eq!(text_of(&runtime, "listing-15-price").as_deref(), Some("1 mirror (9970 div)"));
        assert_eq!(text_of(&runtime, "listing-19-price").as_deref(), Some("30000 divine"));
        assert!(runtime.find("listing-20-price").is_none(), "twenty rows, no more");
    }

    /// EE2's rare item: its two presets, the flat stat rows with their tags and tiers, the
    /// elemental-dps buttons, and the twenty listings the Search button fetches.
    #[test]
    fn the_poe2_rare_bow_lists_its_stat_rows() {
        let mut runtime = runtime("poe2-ee2-rare-item");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Category: Bow"));
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Not Corrupted"));
        assert_eq!(text_of(&runtime, "btn-item_level-label").as_deref(), Some("Item Level:"));
        assert_eq!(text_of(&runtime, "btn-item_level-min").as_deref(), Some("80"));
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("2 of 8, stats"));
        // `createPresets` returns the pseudo and the base-item preset for it.
        assert_eq!(text_of(&runtime, "preset-0-label").as_deref(), Some("Pseudo"));
        assert_eq!(text_of(&runtime, "preset-1-label").as_deref(), Some("Base item"));

        // `FilterModifier.vue`: the label carries the roll, in `ItemModifierText`'s face.
        let rows = [
            (0, "Total DPS: 173"),
            (1, "Elemental DPS: 103"),
            (3, "Attacks per Second: 1.2"),
            (4, "Critical Hit Chance: 5%"),
            (5, "57 to Accuracy Rating"),
            (6, "15% increased Light Radius"),
        ];
        for (index, label) in rows {
            assert_eq!(text_of(&runtime, &format!("stat-{index}-text")).as_deref(), Some(label), "row {index}");
        }
        assert!(runtime.find("stat-2").is_none(), "Physical DPS is hidden by default");
        assert!(runtime.find("stat-7").is_none(), "so is the empty-modifier count");
        // The q20 notice and the roll fields (`wc` 155, `max` empty).
        assert_eq!(text_of(&runtime, "stat-0-quality-text").as_deref(), Some("Q 20%"));
        assert_eq!(text_of(&runtime, "stat-0-min").as_deref(), Some("155"));
        // The max field is empty and shows `FilterModifier.vue`'s placeholder.
        assert_eq!(text_of(&runtime, "stat-0-max").as_deref(), Some(""));
        assert_eq!(runtime.find("stat-0-max").map(|_| "max"), Some("max"));
        // `FilterModifierTiers.vue` and the tag beside it.
        assert_eq!(text_of(&runtime, "tier-tier-1-0-text").as_deref(), Some("Tier 1"));
        assert_eq!(text_of(&runtime, "tag-explicit-text").as_deref(), Some("explicit"));
        // `FilterModifierItemIsElemental.vue`: the "Any" option is the one the presets chose.
        assert!(runtime.find("stat-1-element-lightning").is_some());

        // A rare waits for the Search button; pressing it lands the recorded answer.
        assert_eq!(text_of(&runtime, "search-label").as_deref(), Some("Search"));
        assert!(runtime.find("trade-table").is_none());
        click(&mut runtime, "search");
        assert!(
            runtime.state.poe2.as_ref().unwrap().trade.error.is_none(),
            "{:?}",
            runtime.state.poe2.as_ref().unwrap().trade.error
        );
        assert_eq!(text_of(&runtime, "trade-matched-total").as_deref(), Some("10000"));
        assert_eq!(text_of(&runtime, "listing-0-price").as_deref(), Some("1 exalted"));
        assert_eq!(text_of(&runtime, "listing-0-ilvl").as_deref(), Some("66"));
        assert_eq!(text_of(&runtime, "listing-0-date").as_deref(), Some("5 mo. ago"));
        assert_eq!(text_of(&runtime, "listing-19-price").as_deref(), Some("1 exalted"));
        assert!(runtime.find("listing-20-price").is_none(), "twenty rows, no more");

        // Choosing an element swaps in that element's roll; the search body changes with
        // it, which is why the click comes after the recorded answer's assertions.
        click(&mut runtime, "stat-1-element-lightning");
        assert_eq!(
            runtime
                .state
                .poe2
                .as_ref()
                .and_then(|check| check.stat(1))
                .and_then(|stat| stat.option)
                .and_then(|o| o.value),
            Some(3.0)
        );
    }

    /// `FiltersBlock.vue`'s preset tabs: the base-item preset replaces the pseudo rows.
    #[test]
    fn the_poe2_preset_tabs_replace_the_rows() {
        let mut runtime = runtime("poe2-ee2-rare-item");
        click(&mut runtime, "preset-1");
        let check = runtime.state.poe2.as_ref().unwrap();
        assert_eq!(check.active_preset().map(|preset| preset.id.as_str()), Some("filters.preset_base_item"));
        assert_eq!(text_of(&runtime, "stat-0-text").as_deref(), Some("Adds 43 to 43 Lightning Damage"));
        // The results belonged to the old preset, so the Search button is back.
        assert!(runtime.find("search-label").is_some());
    }

    /// EE2's unique: no tag line (a unique's explicit mods carry no tag), the roll sliders
    /// under the min/max fields, and the `?` a poe.ninja-less unique keeps.
    #[test]
    fn the_poe2_unique_shows_its_roll_sliders() {
        let runtime = runtime("poe2-ee2-unique-item");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("The Eternal Spark"));
        assert_eq!(text_of(&runtime, "corrupted-label").as_deref(), Some("Not Corrupted"));
        assert_eq!(text_of(&runtime, "trend-price").as_deref(), Some("?"));
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("2 of 5, stats"));
        assert_eq!(text_of(&runtime, "stat-0-text").as_deref(), Some("56% increased Energy Shield"));
        // The third stat is the second visible row: the constant rolls between are hidden.
        assert_eq!(text_of(&runtime, "stat-2-text").as_deref(), Some("26% to Lightning Resistance"));
        // `StatRollSlider.vue`: the bounds at its ends, the roll's fields above it.
        assert_eq!(text_of(&runtime, "stat-0-min").as_deref(), Some("54"));
        assert_eq!(text_of(&runtime, "stat-0-slider-min").as_deref(), Some("50"));
        assert_eq!(text_of(&runtime, "stat-0-slider-max").as_deref(), Some("70"));
        assert!(runtime.find("stat-0-slider-tick").is_some());
        assert!(runtime.find("tag-explicit").is_none(), "a unique's explicit mods show no tag");
        // A unique searches when it opens.
        assert_eq!(text_of(&runtime, "trade-matched-total").as_deref(), Some("866"));
    }

    /// EE2's images: the item's art beside the `?` row (`ItemQuickPrice.vue`'s `itemImg`),
    /// and the item-editor button's `augments/empty-socket.png` (`FiltersBlock.vue`).
    #[test]
    fn the_poe2_window_shows_its_item_art_and_socket() {
        let unique = runtime("poe2-ee2-unique-item");
        let elements = unique.elements();
        let art = element(&elements, "trend-item-icon").expect("the unique's art");
        assert_eq!(art.role, "img");
        assert!(
            art.label.as_deref().is_some_and(|label| label.starts_with("https://web.poecdn.com/")),
            "{:?}",
            art.label
        );

        // A bow is a weapon the item editor can add runes to: the empty-socket art.
        let bow = runtime("poe2-ee2-rare-item");
        let elements = bow.elements();
        let socket = element(&elements, "item-editor-icon").expect("the editor's socket");
        assert_eq!(socket.role, "img");
        assert_eq!(socket.label.as_deref(), Some("/images/augments/empty-socket.png"));
        // No trend row for a craftable rare: the reference draws none, so no art is loaded.
        assert!(element(&elements, "trend-item-icon").is_none());
    }

    /// EE2's waystone: the map tier chip, "Stats ignored", the Explicit toggle and the
    /// unknown modifier with its reload button.
    #[test]
    fn the_poe2_waystone_shows_its_unknown_modifier() {
        let mut runtime = runtime("poe2-ee2-rare-map");
        assert_eq!(text_of(&runtime, "filter-name-text").as_deref(), Some("Category: Map"));
        assert_eq!(text_of(&runtime, "btn-map_tier-label").as_deref(), Some("Map Tier:"));
        assert_eq!(text_of(&runtime, "btn-map_tier-min").as_deref(), Some("14"));
        assert_eq!(text_of(&runtime, "btn-stats-label").as_deref(), Some("Stats ignored"));
        assert_eq!(text_of(&runtime, "show-hidden-label").as_deref(), Some("Explicit"));
        // The revives row is hidden (`filters.hide_revives`), so the visible rows start at 1.
        let rows = [(1, "Monster Pack Size: 34%"), (2, "Monster Rarity: 28%"), (3, "Waystone Drop Chance: 75%")];
        for (index, label) in rows {
            assert_eq!(text_of(&runtime, &format!("stat-{index}-text")).as_deref(), Some(label), "row {index}");
        }
        // The trade site's data has no stat for the last line: it stays unknown.
        let text = text_of(&runtime, "unknown-0-text").unwrap_or_default();
        assert!(text.contains("Monster Critical Damage Bonus"), "{text}");
        assert_eq!(text_of(&runtime, "unknown-0-type").as_deref(), Some("explicit — "));
        assert_eq!(text_of(&runtime, "unknown-0-unrecognized").as_deref(), Some("Not recognized modifier"));
        assert!(runtime.find("unknown-0-reload-button").is_some());
        // A waystone waits for Search too.
        click(&mut runtime, "search");
        assert_eq!(text_of(&runtime, "trade-matched-total").as_deref(), Some("9002"));
        assert!(runtime.find("listing-head-ilvl").is_none(), "no iLvl filter, no iLvl column");
    }

    /// Text that isn't a PoE2 item: the error box and the raw block, with EE2's reload
    /// button among the error's actions.
    #[test]
    fn a_poe2_text_that_is_not_an_item_shows_the_error() {
        let runtime = runtime("poe2-error-not-an-item");
        assert_eq!(text_of(&runtime, "error-name").as_deref(), Some("An error occurred while parsing the item"));
        assert_eq!(
            text_of(&runtime, "error-message").as_deref(),
            Some("This is probably a bug and you can report it on GitHub.")
        );
        assert!(runtime.find("error-reload").is_some());
        let raw = text_of(&runtime, "error-raw-text").expect("the raw text block");
        assert!(raw.contains("This is not an item"), "{raw}");
        assert!(runtime.find("filter-name-text").is_none());
    }

    /// EE2's bulk table (`TradeBulk.vue`): the three currency buttons and the selected
    /// table's eight rows. A screenshot case never reaches it (no exchange answer was
    /// recorded for PoE2), so the state is built here from an `execBulkSearch` result.
    #[test]
    fn the_poe2_bulk_table_shows_a_currencys_offers() {
        use crate::poe2::price_check::trade::bulk::{BulkPricingResult, BulkSearch};
        use crate::poe2::price_check::trade::{BulkCurrency, BulkResults};
        use wraex_poe2::results::AccountStatus;

        let mut app = App::with_mode_poe2(&corpus_poe2("ee2-uncut-skill-gem"), Mode::Replay);
        let check = app.poe2.as_mut().expect("the PoE2 window");
        assert_eq!(check.trade.api(check), wraex_poe2::trade::Api::Bulk, "an uncut gem trades in bulk");
        let row = |id: &str, exchange: f64, item: f64, stock: f64| BulkPricingResult {
            id: id.to_owned(),
            relative_date: "1 day ago".to_owned(),
            exchange_amount: exchange,
            item_amount: item,
            stock,
            account_status: AccountStatus::Online,
            is_mine: false,
            account_name: "someone".to_owned(),
            ign: "Someone".to_owned(),
        };
        check.trade.error = None;
        check.do_search = true;
        check.trade.selected = BulkCurrency::Divine;
        check.trade.bulk = Some(BulkResults {
            divine: Some(BulkSearch {
                query_id: "query".to_owned(),
                have_tag: "divine".to_owned(),
                total: 9.0,
                listed: vec![row("a", 2.0, 1.0, 12.0), row("b", 4.0, 2.0, 3.0)],
            }),
            ..Default::default()
        });
        let runtime = runtime_app(app, scene_size(None));
        assert_eq!(text_of(&runtime, "bulk-currency-divine-total").as_deref(), Some("9"));
        assert_eq!(text_of(&runtime, "bulk-currency-exalted-total").as_deref(), Some("?"));
        assert_eq!(text_of(&runtime, "bulk-head-exchange-left").as_deref(), Some("div"));
        // `Number((exchangeAmount / itemAmount).toFixed(4))` and the amounts as sent.
        assert_eq!(text_of(&runtime, "bulk-0-price").as_deref(), Some("2"));
        assert_eq!(text_of(&runtime, "bulk-0-exchange-left").as_deref(), Some("2"));
        assert_eq!(text_of(&runtime, "bulk-1-price").as_deref(), Some("2"));
        assert_eq!(text_of(&runtime, "bulk-1-exchange-right").as_deref(), Some("2"));
        assert_eq!(text_of(&runtime, "bulk-0-stock").as_deref(), Some("12"));
        assert_eq!(text_of(&runtime, "bulk-1-fulfill").as_deref(), Some("1"));
        assert!(runtime.find("bulk-2-price").is_none(), "two offers, the rest of the table is empty");
    }

    /// Closing one game's window leaves the other's state alone.
    #[test]
    fn the_poe2_window_closes_on_its_own() {
        let mut runtime = runtime("poe2-ee2-unique-item");
        click(&mut runtime, "titlebar-close");
        assert!(!runtime.state.poe2.as_ref().unwrap().open);
        assert!(runtime.state.price_check.open);
        assert!(runtime.find("titlebar").is_none());
    }

    // ---- PoE2's item check / map check ---------------------------------------------------

    /// EE2's waystone check: every stat (with its type) and the unknown modifier the live trade
    /// data leaves untranslated, plus the wording notice.
    #[test]
    fn the_poe2_map_check_lists_a_waystones_stats() {
        let runtime = runtime("poe2-map-check-ee2-rare-map");
        assert_eq!(text_of(&runtime, "map-check-name").as_deref(), Some("Waystone (Tier 14)"));
        assert_eq!(text_of(&runtime, "map-check-profile-label").as_deref(), Some("Profile"));
        let expected = [
            "Area has patches of Shocked Ground",
            "28% increased Monster Damage",
            "Monsters fire 2 additional Projectiles",
            "Monsters have 293% increased Critical Hit Chance",
        ];
        for (index, label) in expected.iter().enumerate() {
            assert_eq!(text_of(&runtime, &format!("stat-{index}-text")).as_deref(), Some(*label), "row {index}");
        }
        assert!(runtime.find("stat-4").is_none(), "four prepared stats");
        // The live trade data renames `#% to Monster Critical Damage Bonus`, so EE2's own page
        // (which reads the same recorded answer) shows it as an unknown modifier too.
        let unknown = text_of(&runtime, "map-check-unknown-0-text").expect("the unknown modifier line");
        assert!(unknown.starts_with("Not recognized modifier \u{2014} "), "{unknown}");
        assert!(unknown.ends_with("to Monster Critical Damage Bonus"), "{unknown}");
        assert!(runtime.find("map-check-outdated-text").is_some());
    }

    /// EE2's item info on a rare with a price note: the DPS grid, CoE and the `$` button, whose
    /// stash search sends the note and, with `samePricedType: "both"`, the item's name.
    #[test]
    fn the_poe2_item_check_searches_a_priced_rare() {
        let mut runtime = runtime("poe2-item-check-note-rare-bow");
        assert_eq!(text_of(&runtime, "item-check-name-text").as_deref(), Some("Rider Bow"));
        assert_eq!(text_of(&runtime, "item-check-dps-0-value").as_deref(), Some("58"));
        assert_eq!(text_of(&runtime, "item-check-dps-1-value").as_deref(), Some("103"));
        assert_eq!(text_of(&runtime, "item-check-dps-2-value").as_deref(), Some("161"));
        assert!(runtime.find("item-check-coe").is_some());
        assert!(runtime.find("item-check-price").is_some(), "the price note shows the `$` button");

        click(&mut runtime, "item-check-wiki");
        click(&mut runtime, "item-check-poedb");
        click(&mut runtime, "item-check-price");
        click(&mut runtime, "item-check-stash");
        let actions = &runtime.state.item_check_poe2.as_ref().unwrap().actions;
        assert_eq!(actions[0], poe2::item_check::Action::Open("https://www.poe2wiki.net/wiki/Rider Bow".to_owned()));
        assert_eq!(actions[1], poe2::item_check::Action::Open("https://poe2db.tw/us/Bows#ModifiersCalc".to_owned()));
        match &actions[2] {
            poe2::item_check::Action::Send(action) => {
                assert_eq!(action.payload.action, "stash-search");
                let text = action.payload.text.as_deref().expect("the note and the name");
                assert_eq!(text, "\"~price 5 exalted\"\"Rider Bow\"");
            }
            other => panic!("{other:?}"),
        }
        match &actions[3] {
            poe2::item_check::Action::Send(action) => assert_eq!(action.payload.text.as_deref(), Some("\"Rider Bow\"")),
            other => panic!("{other:?}"),
        }
    }

    /// A unique focus has no craft page and no DPS; an uncut gem has CoE but still no DPS.
    #[test]
    fn the_poe2_item_check_hides_what_the_item_lacks() {
        let unique = runtime("poe2-item-check-ee2-unique-item");
        assert_eq!(text_of(&unique, "item-check-name-text").as_deref(), Some("The Eternal Spark"));
        assert!(unique.find("item-check-coe").is_none());
        assert!(unique.find("item-check-price").is_none());
        assert!(unique.find("item-check-dps").is_none());

        let gem = runtime("poe2-item-check-ee2-uncut-skill-gem");
        assert_eq!(text_of(&gem, "item-check-name-text").as_deref(), Some("Uncut Skill Gem (Level 19)"));
        assert!(gem.find("item-check-coe").is_some());
        assert!(gem.find("item-check-dps").is_none());
    }
}
