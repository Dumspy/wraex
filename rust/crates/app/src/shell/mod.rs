//! The running app: what upstream's Electron main process does around the Rune windows.
//!
//! Upstream is two processes: the renderer draws widgets (`ref/*/renderer`), and the main
//! process owns the tray, the global hotkeys, the clipboard, the update check and the
//! leagues (`ref/*/main`). [`wraex_platform`] ports that main process's logic into state
//! machines; this module is the glue that gives them an OS and a Rune app:
//!
//! 1. **Settings** ([`config`]): Wraex's own `config.json`, or upstream's imported once as a
//!    copy, fed through the game's `host_config`.
//! 2. **Startup work** ([`spawn_startup_work`]): the trade site's leagues, the game data and
//!    poe.ninja's prices on a worker, like the renderer's `useLeagues`/`usePoeninja`.
//! 3. **Tray** ([`tray`], [`wraex_platform::tray`]): the icon and its three entries, created
//!    on the UI thread once Rune's event loop runs.
//! 4. **Hotkeys**: the config's shortcuts in Rune's global-shortcut API, each press run
//!    through [`wraex_platform::shortcuts::Shortcuts::press`] with the OS [`keys`] and
//!    [`clipboard`]. A `copy-item` press opens a window next to the cursor.
//! 5. **Update check** ([`updates`]): at launch and every 16 hours, shown on the settings
//!    window's About page.
//! 6. **Actions** ([`dispatch`]): the widgets record what their buttons ask for; the shell
//!    opens the browser, types a stash search or quits.
//!
//! # The game's window (PLAN.md phase 2)
//!
//! On Windows the shell starts [`wraex_platform::window::Watcher`], the Win32 hook that finds
//! the game's window by the config's `windowTitle` and reports its focus, moves and
//! destruction (`electron-overlay-window`'s native half). Everything upstream hangs off that
//! follows from it:
//!
//! - **The window is the overlay.** It is transparent, borderless, on top and click-through
//!   (`window_options()`), covers the game's client area, and the widget is drawn inside it
//!   at the cursor ([`App::overlay_offset`]). Hotkeys show it *inactive* (`showInactive()`:
//!   the game keeps focus and the keyboard); the tracker activates it — click-through off,
//!   overlay focused — when the mouse reaches the price check's card, and the overlay handle
//!   gives the game its focus back.
//! - **The widget follows the mouse.** A [`wraex_platform::window::MouseEvent`] stream from
//!   the Win32 low-level mouse hook feeds the ported `WidgetAreaTracker`: moving away from
//!   the press position closes the price check, clicking it activates the overlay, and
//!   Ctrl+wheel outside the stash walks the stash tabs.
//! - **The Alt peek and the key log.** A [`wraex_platform::window::KeyEvent`] stream from the
//!   Win32 low-level keyboard hook feeds `OverlayVisibility`: holding Alt alone hides the
//!   widgets, releasing it (or pressing another key) brings them back. The hook reports every
//!   key, which is also what `Shortcuts`' `logKeys` writes.
//! - **Hotkeys exist only while the game is focused**: [`Shortcuts::on_game_active`], as
//!   upstream's `active-change` listener, so a press can never land in another application.
//!   That is what lets `keys::OSSender` synthesize keys by default here (see its
//!   documentation for the platforms without tracking).
//! - **The window follows the game**: moves and resizes re-place it, and it hides while the
//!   game is unfocused, as the overlay library does. A `focus-change` with the overlay out of
//!   front closes every `hide-on-blur` widget, as upstream's renderer does.
//!
//! macOS, X11 and Wayland have no window hook yet, so there the app keeps its earlier shape:
//! an ordinary window placed next to the cursor, hotkeys registered all the time, and key
//! synthesis opt-in (`WRAEX_KEY_SYNTHESIS=1`). `wraex_platform::window` documents the
//! follow-ups. Where a hotkey cannot be synthesized, a price check prices the item already on
//! the clipboard instead of pressing the game's copy key.
//!
//! None of this runs in a scene, a `--snap` render or a test: [`run`] is only reached from
//! `main` when Rune is not headless and no `--scene` was given, and every entry point checks
//! `App::live`.

pub mod accelerator;
pub mod clipboard;
pub mod config;
pub mod dispatch;
pub mod keys;
pub mod tray;
pub mod updates;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use rune::{AppHandle, AppSpec, Tx};
use serde_json::json;
use wraex_core::GameId;
use wraex_core::game_files::{self, Os, UserDirs};
use wraex_core::leagues::{LeagueSource, Leagues};
use wraex_core::trade::api::Clock;
use wraex_core::updates::{NoDownloadReason, UpdateInfo};
use wraex_platform::clipboard::{Clipboard, is_poe_item};
use wraex_platform::overlay::{OverlayController, OverlayEvent, OverlayHandle, TrackArea};
use wraex_platform::shortcuts::{
    Action, HostConfig, ItemText, PressContext, Registration, ShortcutAction, ShortcutEvent, Shortcuts, WheelEvent,
};
use wraex_platform::tray::{Tray, TrayAction};
use wraex_platform::window::{self, Bounds, GameWindow, KeyEvent, MouseEvent, Point};

#[cfg(target_os = "windows")]
use wraex_platform::window::WindowTarget;

use crate::settings::{self, Settings};
use crate::trade::{LiveClock, Mode};
use crate::widgets;
use crate::{App, HEIGHT, view};

pub use dispatch::ItemAction;

/// The product version the tray tooltip and the update check use.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The trade site both games ask for leagues and trade data (`getTradeEndpoint()`).
const TRADE_HOST: &str = "www.pathofexile.com";

/// How far below and right of the cursor a widget opens, in physical pixels.
const CURSOR_OFFSET: i32 = 12;

/// `WidgetAreaTracker.ts`'s `closeThreshold`: `2.5 * AppConfig().fontSize`, in points. Moving
/// the mouse further than this from the press position closes the price check (unless the
/// configured hold key is down); [`TrackArea`] scales it for Windows.
const CLOSE_THRESHOLD: f32 = 2.5 * crate::theme::REM;

/// How many frames a focus the overlay asked for counts as "in front", at 60 fps about a
/// second and a half: `electron-overlay-window` waits for the `focus` event, and this is the
/// bound on waiting for one that never comes.
const FOCUS_PATIENCE: u8 = 90;

/// Which window the app is showing, for the native window's size and placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shown {
    /// Nothing: the app is idle in the tray.
    None,
    Settings,
    ItemCheck,
    PriceCheck,
    Poe2,
    Widget(String),
}

/// Where a shown window goes: next to the cursor that asked for it (a hotkey), or in the
/// middle of whatever it is over (the tray's settings and widgets).
#[derive(Debug, Clone, Copy, PartialEq)]
enum Placement {
    Cursor(Point),
    Centred,
}

/// Whether what is shown is a widget over the game (the overlay), as opposed to the settings
/// window, which upstream draws in an ordinary window of its own.
fn is_overlay(shown: &Shown) -> bool {
    matches!(shown, Shown::PriceCheck | Shown::Poe2 | Shown::ItemCheck | Shown::Widget(_))
}

/// `OverlayVisibility`: a lone Alt hides the widget the overlay draws, not the window itself
/// (the game keeps the mouse and keyboard it already had) and not the settings window, which
/// upstream draws in an ordinary window of its own.
fn peek_hides(shown: &Shown, overlay_visible: bool) -> bool {
    is_overlay(shown) && !overlay_visible
}

/// Where a widget goes inside the overlay window: at the cursor that asked for it (12 px below
/// and right, as the ordinary window has always opened) or centred for a window the tray
/// opened, then clamped so the whole card stays inside the game's client area.
///
/// `bounds` is the game's client area in physical pixels and `scale` the window's DPI scale,
/// which is what turns both into the points Rune lays the widget out in.
fn widget_offset(placement: Option<Placement>, size: (f32, f32), bounds: Bounds, scale: f64) -> (f32, f32) {
    let (width, height) = (f64::from(size.0), f64::from(size.1));
    let (client_width, client_height) = (bounds.width / scale, bounds.height / scale);
    let (x, y) = match placement {
        Some(Placement::Cursor(point)) => (
            (point.x - bounds.x) / scale + f64::from(CURSOR_OFFSET),
            (point.y - bounds.y) / scale + f64::from(CURSOR_OFFSET),
        ),
        _ => ((client_width - width) / 2.0, (client_height - height) / 2.0),
    };
    // A widget taller than the game (a card at a small window size) starts at the top edge.
    let clamp = |value: f64, limit: f64| value.clamp(0.0, limit.max(0.0)) as f32;
    (clamp(x, client_width - width), clamp(y, client_height - height))
}

/// The card's size inside the overlay window, in points. The price check fills the overlay's
/// height (`#price-window` is `h-full` with the listings scrolling inside it); every other
/// card keeps its own size, as upstream's `Widget.vue` cards do.
fn overlay_widget_size(shown: &Shown, bounds: Bounds, scale: f64) -> Option<(f32, f32)> {
    match shown {
        Shown::PriceCheck | Shown::Poe2 => Some((crate::poe1::price_check::WIDTH, (bounds.height / scale) as f32)),
        _ => window_size(shown),
    }
}

/// `OVERLAY->MAIN::track-area`, in the coordinates upstream's renderer reports: the card's
/// rectangle and the press position in screen DIPs, and `dpr` for `WidgetAreaTracker`'s Win32
/// conversion. `offset` is where the card sits inside the overlay and `size` its size, both
/// in points; `bounds` is the game's client area in physical pixels and `scale` the overlay's
/// DPI scale.
fn track_area(
    offset: (f32, f32),
    size: (f32, f32),
    bounds: Bounds,
    scale: f64,
    cursor: Point,
    hold_key: Option<String>,
) -> TrackArea {
    TrackArea {
        hold_key,
        close_threshold: f64::from(CLOSE_THRESHOLD),
        // `from` is where the item's hotkey fired (`e.position`).
        from: Point { x: cursor.x / scale, y: cursor.y / scale },
        // `window.screenX + wm.poePanelWidth`, and the overlay's whole `innerHeight`.
        area: Bounds {
            x: bounds.x / scale + f64::from(offset.0),
            y: bounds.y / scale + f64::from(offset.1),
            width: f64::from(size.0),
            height: f64::from(size.1),
        },
        dpr: scale,
    }
}

/// The price check's `hotkeyHold` (`Ctrl` by default), the modifier that keeps the card open
/// while the cursor is away from the press position. `None` is JavaScript's `undefined`: a
/// hotkey with no hold key at all. An empty string is not the same thing.
fn price_check_hold_key(config: &serde_json::Value) -> Option<String> {
    config
        .get("widgets")?
        .as_array()?
        .iter()
        .find(|widget| widget.get("wmType").and_then(|value| value.as_str()) == Some("price-check"))?
        .get("hotkeyHold")?
        .as_str()
        .map(str::to_owned)
}

/// What an [`OverlayEvent`] does to the app's windows, matching upstream's renderer:
/// `PriceCheckWindow.vue` hides itself on `hide-exclusive-widget`, and `OverlayWindow.vue`
/// hides every `hide-on-blur` widget whenever a `focus-change` says the overlay is not in
/// front (Escape, Ctrl+W, the overlay key's off state, the game taking the focus back).
fn apply_overlay_event(event: &OverlayEvent, app: &mut App) {
    match event {
        OverlayEvent::HideExclusiveWidget => app.close_exclusive_widget(),
        OverlayEvent::FocusChange { overlay: false, .. } => app.hide_widgets_on_blur(),
        _ => {}
    }
}

/// One keyboard event from the global hook, as upstream's `uiohook` listeners read it:
/// `OverlayVisibility` (an Alt-only keydown hides the widgets after a delay, any other key or
/// the Alt release brings them back) and `Shortcuts`' `logKeys`. The hook only observes, so
/// nothing here swallows, repeats or synthesizes a key.
fn apply_key_event(overlay: &mut OverlayController, shortcuts: &Shortcuts, event: KeyEvent, clock: &dyn Clock) {
    match event {
        KeyEvent::Down(event) => {
            overlay.on_global_key_down(&event, clock);
            if let Some(line) = shortcuts.log_key_down(&event) {
                eprintln!("{line}");
            }
        }
        KeyEvent::Up(event) => {
            overlay.on_global_key_up(event.alt);
            if let Some(line) = shortcuts.log_key_up(event.key) {
                eprintln!("{line}");
            }
        }
    }
}

/// Whether Escape/Ctrl+W and the overlay key act on what is shown. They are the overlay's
/// `before-input-event` commands; the settings window is an ordinary window upstream with no
/// such handler, and it must keep its keys (and its clicks) while it is open.
fn overlay_commands_apply(shown: &Shown, interactable: bool) -> bool {
    interactable && *shown != Shown::Settings
}

/// The live app's OS-facing half. Held by [`App`] as `App::live`; `None` in a scene or a
/// test, which is what keeps every side effect out of them.
pub struct Live {
    pub game: GameId,
    pub startup: config::Startup,
    pub host: HostConfig,
    pub shortcuts: Shortcuts,
    pub overlay: OverlayController,
    pub window: GameWindow,
    pub clipboard: clipboard::SystemClipboard,
    pub keys: keys::OSSender,
    pub handle: AppHandle<App>,
    pub tray: Option<Tray>,
    /// The OS layer that finds and follows the game's window (`electron-overlay-window`'s
    /// native half). `Some` once [`Live::boot`] started it; Windows only for now.
    pub watcher: Option<window::Watcher>,
    /// The window title the watcher was started with, to notice a changed config.
    watched_title: Option<String>,
    /// The overlay's window and the game's, shared with every `OverlayPanel`.
    panel: Arc<Panel>,
    /// Where the next window goes, and where the current one went.
    placement: Option<Placement>,
    /// The widget's offset inside the overlay window, in points ([`App::overlay_offset`]).
    overlay_offset: Option<(f32, f32)>,
    /// The overlay window's height in points, which the price-check card fills
    /// ([`App::overlay_size`]).
    overlay_height: Option<f32>,
    /// The game rectangle the window was last placed over.
    placed: Option<Bounds>,
    /// A price-check press asked for the card's area to be followed
    /// (`OVERLAY->MAIN::track-area`); honoured once the window is placed over the game.
    pending_track: Option<Point>,
    /// A widget opened under the cursor takes the mouse as soon as it is on screen
    /// (`Live::sync_window`). Only for a window that is the widget itself (no game to be over)
    /// and for the settings window; a hotkey-opened widget over the game stays inactive.
    activate_on_show: bool,
    /// Whether the overlay window itself has the focus (`AppSpec::on_focus`): the library's
    /// blur rule hides the overlay when neither it nor the game is in front.
    pub overlay_focused: bool,
    /// Tray clicks, queued by the tray's native menu callback for [`Live::poll`].
    tray_actions: Arc<Mutex<Vec<TrayAction>>>,
    registered: Vec<rune::ShortcutId>,
    /// Hotkeys that could not be registered and settings that could not be read, for the
    /// startup log (and, later, the debug page).
    pub warnings: Vec<String>,
    /// `leagues.selectedId`: the league prices and searches use.
    pub league: String,
    pub updates: updates::Updates,
    pub update: UpdateInfo,
    /// poe.ninja's prices for the next PoE1 price check, fetched since launch.
    pub prices: Option<crate::poe1::price_check::prices::PriceList>,
    /// EE2's poe.ninja overviews, fetched since launch (`usePoeninja` is app-wide there).
    pub poe2_prices: Option<Vec<(&'static str, String)>>,
    /// The settings window was open at the last poll: a close means Save or Cancel, and
    /// either way the app re-reads its file.
    settings_was_open: bool,
    shown: Shown,
}

impl std::fmt::Debug for Live {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Live")
            .field("game", &self.game)
            .field("league", &self.league)
            .field("config", &self.startup.store.path())
            .field("warnings", &self.warnings)
            .finish_non_exhaustive()
    }
}

impl Live {
    /// The shell's state for a live run: the config, its host half, the idle OS traits and
    /// the prices the first price check will use. `panel` is the overlay's window, created
    /// before the app because Rune hands the window over on creation.
    pub fn new(handle: AppHandle<App>, startup: config::Startup, panel: Arc<Panel>) -> Self {
        let game = startup.game;
        let os = current_os();
        let host = host_config(game, &startup.config);
        let mut shortcuts = Shortcuts::new(game, os);
        let show_mods = show_mods_key(game, &startup.config);
        let mut warnings: Vec<String> =
            shortcuts.apply_host_config(&host, &show_mods).into_iter().map(|warning| warning.to_string()).collect();

        let mut overlay = OverlayController::new(game, os);
        overlay.update_from(&host);

        let league = startup
            .config
            .get("leagueId")
            .and_then(|value| value.as_str())
            .filter(|league| !league.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| default_league(game).to_owned());

        // `usePoeninja` runs for the whole renderer at launch; the port's prices live in a
        // price check, so the shell keeps one warm and hands it to the first window opened.
        let mut prices = crate::poe1::price_check::prices::PriceList::new(Mode::Live);
        prices.queue(&league, false);

        if startup.source == config::Source::ImportedUpstream {
            warnings.push(format!(
                "imported {} into {} (upstream's file was left as it was)",
                startup.upstream.as_deref().map_or("<upstream config>".to_owned(), |path| path.display().to_string()),
                startup.store.path().display()
            ));
        }

        Self {
            game,
            startup,
            host,
            shortcuts,
            overlay,
            window: GameWindow::new(),
            clipboard: clipboard::SystemClipboard::new(),
            keys: keys::OSSender::new(),
            handle,
            tray: None,
            watcher: None,
            watched_title: None,
            panel,
            placement: None,
            overlay_offset: None,
            overlay_height: None,
            placed: None,
            pending_track: None,
            activate_on_show: false,
            overlay_focused: false,
            tray_actions: Arc::new(Mutex::new(Vec::new())),
            registered: Vec::new(),
            warnings,
            league,
            updates: updates::Updates::default(),
            update: UpdateInfo::Initial,
            prices: Some(prices),
            poe2_prices: None,
            settings_was_open: false,
            shown: Shown::None,
        }
    }

    /// The app's log line for a problem: printed once, and kept for the debug page.
    pub fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        eprintln!("wraex: {message}");
        self.warnings.push(message);
    }

    /// The first turn of the event loop: the tray, the game window's hook, and the configured
    /// hotkeys, all of which need Rune's host to exist (the tray's native menu on macOS,
    /// `register_shortcut`).
    pub fn boot(&mut self) {
        self.start_tray();
        self.start_game_window();
        // With the game's window followed, the hotkeys belong to its focus
        // (`poll_game_window` registers them); before the first report there is nothing to
        // register them for.
        if self.hotkeys_follow_game() {
            self.register_hotkeys();
        }
    }

    /// Start following the game's window, by the config's `windowTitle` (`OverlayWindow`'s
    /// `updateOpts(overlayKey, windowTitle)`). The watcher finds it now or when it appears.
    #[cfg(target_os = "windows")]
    fn start_game_window(&mut self) {
        if self.watcher.is_some() {
            return;
        }
        let title = self.host.window_title.clone();
        if title.is_empty() {
            return self.warn("the config has no window title: the game's window cannot be followed");
        }
        let handle = self.handle.clone();
        match window::Watcher::start(WindowTarget::new(title.clone()), Box::new(move || handle.wake())) {
            Ok(watcher) => {
                eprintln!("wraex: following the \"{title}\" window");
                self.watcher = Some(watcher);
                self.watched_title = Some(title);
                // Hotkeys now exist only while the game is focused, so pressing keys is safe
                // without the environment variable (`keys` documentation).
                self.keys.set_game_focus_tracking();
            }
            Err(error) => self.warn(error.to_string()),
        }
    }

    /// macOS, X11 and Wayland keep the earlier behaviour: an ordinary window, system-wide
    /// hotkeys and opt-in key synthesis. `wraex_platform::window` documents the follow-ups.
    #[cfg(not(target_os = "windows"))]
    fn start_game_window(&mut self) {}

    /// The OS layer's reports: the game gained or lost the focus, moved, was minimised or
    /// closed. The window state machine turns them into the changes the rest of the host
    /// acts on, and the hotkeys follow the focus as upstream's `active-change` does.
    fn poll_game_window(&mut self) -> bool {
        let Some(watcher) = &mut self.watcher else { return false };
        let events = watcher.drain();
        if events.is_empty() {
            return false;
        }
        let game = watcher.hwnd();
        if let Ok(mut current) = self.panel.game.lock() {
            *current = game;
        }
        let mut panel = OverlayPanel::new(self.panel.clone());
        let mut changed = false;
        for event in events {
            if let Some(change) = self.window.feed(event) {
                self.overlay.on_window_event(&change, &self.window, &mut panel);
                changed = true;
            }
        }
        // The game is in front again: a focus the overlay asked for is settled either way.
        if self.window.is_active() {
            self.panel.focus_settled();
        }
        // One registration decision per batch: the overlay may itself have taken the focus
        // while handling these (`assert_overlay_active` → `set_active(false)`), which
        // unregisters the hotkeys exactly like a real blur does upstream.
        if self.sync_registration() {
            changed = true;
        }
        changed
    }

    /// The overlay's own events (`MAIN->OVERLAY::*`). Upstream's renderer listens for them;
    /// this port has no such channel, so the ones a person has to see are logged — the
    /// administrator-rights error above all (`OverlayWindow.handleOverlayAttached`) — and the
    /// two that close widgets are run against the app: the cursor left the card
    /// (`hide-exclusive-widget`), and the overlay is no longer in front, which hides every
    /// widget flagged `hide-on-blur` (`OverlayWindow.vue`'s `focus-change` handler).
    fn drain_overlay_events(&mut self, app: &mut App) {
        for event in self.overlay.take_events() {
            match event {
                OverlayEvent::NoAccess => self.warn(
                    "the game is running with administrator rights: start Wraex as administrator too, \
                     or the overlay cannot reach its window",
                ),
                OverlayEvent::Attached => eprintln!("wraex: overlay attached to the game window"),
                OverlayEvent::FocusChange { game, overlay, using_hotkey } => {
                    eprintln!("wraex: focus game={game} overlay={overlay} hotkey={using_hotkey}");
                    apply_overlay_event(&event, app);
                }
                OverlayEvent::HideExclusiveWidget => apply_overlay_event(&event, app),
                // The Alt peek: `sync_window` reads `OverlayController::is_visible`, which is
                // the state this event reports; there is no renderer channel to forward it to.
                OverlayEvent::Visibility { .. } => {}
            }
        }
    }

    /// The global mouse hook's events (`uiohook`'s `mousemove`/`mousedown`/`wheel`). Moves feed
    /// the tracked card and the Alt peek; a click on the card takes the overlay; Ctrl+wheel
    /// outside the stash walks the stash tabs (`Shortcuts.ts`). The hook's move stream is
    /// armed only while a card is tracked, so an idle desktop wakes nothing.
    fn poll_mouse(&mut self) -> bool {
        let Some(watcher) = self.watcher.as_mut() else { return false };
        let events = watcher.drain_mouse();
        if events.is_empty() {
            watcher.set_mouse_moves(self.overlay.is_tracking_area());
            return false;
        }
        let mut panel = OverlayPanel::new(self.panel.clone());
        for event in events {
            match event {
                MouseEvent::Move { point, ctrl, alt } => {
                    self.overlay.on_global_mouse_move(alt);
                    self.overlay.on_mouse_move(point, ctrl, alt, &mut self.window, &mut panel);
                }
                MouseEvent::Down { point } => {
                    self.overlay.on_mouse_down(point, &mut self.window, &mut panel);
                }
                MouseEvent::Wheel { point, rotation, ctrl } => {
                    let wheel = WheelEvent { point, rotation, ctrl };
                    self.shortcuts.on_wheel(&wheel, &self.window, &mut self.keys);
                }
            }
        }
        watcher.set_mouse_moves(self.overlay.is_tracking_area());
        self.sync_registration();
        true
    }

    /// The global keyboard hook's events (`uiohook`'s `keydown`/`keyup`): the Alt peek and the
    /// key log. The peek's hide is a timer (`OverlayVisibility`'s `setTimeout`), so the deadline
    /// is polled here too, on frames without a key. Only a change of the peek redraws: a key that
    /// starts or cancels a pending hide leaves the widget on screen, and the log is stderr.
    fn poll_keyboard(&mut self) -> bool {
        let events = match self.watcher.as_mut() {
            Some(watcher) => watcher.drain_keys(),
            None => return false,
        };
        let visible = self.overlay.is_visible();
        for event in events {
            apply_key_event(&mut self.overlay, &self.shortcuts, event, &LiveClock);
        }
        self.overlay.poll(&LiveClock);
        visible != self.overlay.is_visible()
    }

    /// `Shortcuts::on_game_active`: upstream's `active-change` listener. Registers the
    /// configured hotkeys while the game is focused and unregisters them the moment it is
    /// not — including when the overlay takes the keyboard itself.
    fn sync_registration(&mut self) -> bool {
        // Without a window hook the hotkeys stay system-wide, as the port has always done.
        if self.watcher.is_none() {
            return false;
        }
        match self.shortcuts.on_game_active(self.window.is_active()) {
            Registration::Register => {
                self.register_hotkeys();
                true
            }
            Registration::UnregisterAll => {
                self.unregister_hotkeys();
                true
            }
            Registration::None => false,
        }
    }

    /// `AppTray.ts`: the icon and its menu. Created on the UI thread inside the event loop,
    /// which is what macOS' status item needs.
    fn start_tray(&mut self) {
        let (rgba, width, height) = match tray::icon(self.game) {
            Ok(icon) => icon,
            Err(error) => return self.warn(format!("no tray icon: {error}")),
        };
        let overlay_key = self.host.overlay_key.clone();
        match Tray::new(VERSION, &overlay_key, rgba, width, height) {
            Ok(tray) => {
                self.tray = Some(tray);
                eprintln!("wraex: tray icon shown ({}x{width})", height);
                // `AppTray.ts`'s click handler: the entries report themselves; the app drains
                // them in `poll`, which this callback wakes.
                let pending = self.tray_actions.clone();
                let handle = self.handle.clone();
                wraex_platform::tray::set_menu_handler(move |action| {
                    if let Ok(mut queue) = pending.lock() {
                        queue.push(action);
                    }
                    handle.wake();
                });
            }
            Err(error) => self.warn(format!("no tray: {error}")),
        }
    }

    /// `globalShortcut.register` for every configured hotkey: one Rune registration each, so
    /// one press runs one action.
    fn register_hotkeys(&mut self) {
        self.unregister_hotkeys();
        let entries: Vec<ShortcutAction> = self
            .shortcuts
            .registered_shortcuts()
            .into_iter()
            // `test-only`: there only to see whether the OS takes the chord.
            .filter(|entry| !entry.unregister_immediately)
            .map(|entry| ShortcutAction {
                shortcut: entry.shortcut,
                keep_mod_keys: entry.keep_mod_keys,
                action: entry.action,
            })
            .collect();
        for entry in entries {
            let Some(chord) = accelerator::accelerator(&entry.shortcut) else {
                self.warn(format!("hotkey \"{}\" has a key Rune cannot register", entry.shortcut));
                continue;
            };
            let shortcut: rune::Shortcut = match chord.parse() {
                Ok(shortcut) => shortcut,
                Err(error) => {
                    self.warn(format!("hotkey \"{}\": {error}", entry.shortcut));
                    continue;
                }
            };
            let handler = move |app: &mut App, _tx: &mut Tx| app.on_hotkey(&entry);
            match self.handle.register_shortcut(shortcut.clone(), handler) {
                Ok(id) => self.registered.push(id),
                Err(error) => {
                    let mut message = format!("hotkey could not be registered: {error}");
                    if error == rune::ShortcutError::Conflict
                        && let Some(owner) = self.handle.shortcut_owner(&shortcut)
                    {
                        message.push_str(&format!(" ({owner})"));
                    }
                    self.warn(message);
                }
            }
        }
        if self.registered.is_empty() {
            if self.shortcuts.actions().is_empty() {
                self.warn("no hotkey is configured: open the settings window and record one");
            } else {
                self.warn("no hotkey could be registered: see the messages above");
            }
        } else {
            let hotkeys: Vec<&str> = self.shortcuts.actions().iter().map(|entry| entry.shortcut.as_str()).collect();
            eprintln!("wraex: {} hotkeys registered: {}", self.registered.len(), hotkeys.join(", "));
        }
    }

    fn unregister_hotkeys(&mut self) {
        for id in self.registered.drain(..) {
            let _ = self.handle.unregister_shortcut(id);
        }
    }

    /// Re-read the config after the settings window closed (Save or Cancel) and apply what
    /// changed to the running app, as upstream's `updateConfig` does.
    fn reload_config(&mut self) {
        let Some(contents) = self.startup.store.load() else { return };
        let loaded = settings::load(self.game, Some(&contents));
        if loaded.save_temporary {
            return self.warn("the config could not be read back after the settings window");
        }
        self.startup.config = loaded.config;
        // `updateConfig` also carries the league: the next price check prices in it, and the
        // prices the shell kept warm are asked for again.
        if let Some(league) = self
            .startup
            .config
            .get("leagueId")
            .and_then(|value| value.as_str())
            .filter(|league| !league.is_empty() && *league != self.league)
            .map(str::to_owned)
        {
            self.league = league.clone();
            if let Some(prices) = &mut self.prices {
                prices.queue(&league, true);
            }
        }
        self.host = host_config(self.game, &self.startup.config);
        let show_mods = show_mods_key(self.game, &self.startup.config);
        let warnings = self.shortcuts.apply_host_config(&self.host, &show_mods);
        for warning in warnings {
            self.warn(warning.to_string());
        }
        self.overlay.update_from(&self.host);
        // `electron-overlay-window` takes the title once, at init; a config that renamed the
        // window gets a fresh hook instead (upstream restarts the app for it).
        if self.watched_title.as_deref().is_some_and(|title| title != self.host.window_title) {
            self.watcher = None;
            self.watched_title = None;
            if let Some(change) = self.window.feed(window::GameWindowEvents::Detach) {
                let mut panel = OverlayPanel::new(self.panel.clone());
                self.overlay.on_window_event(&change, &self.window, &mut panel);
            }
            self.start_game_window();
        }
        if let Some(tray) = &mut self.tray
            && let Err(error) = tray.set_overlay_key(self.host.overlay_key.clone())
        {
            self.warn(format!("rebuilding the tray menu failed: {error}"));
        }
        if self.hotkeys_follow_game() {
            self.register_hotkeys();
        }
    }

    /// Whether the hotkeys may be registered now: without a window hook they are system-wide
    /// (the earlier behaviour), with one they exist only while the game has the focus.
    fn hotkeys_follow_game(&self) -> bool {
        self.watcher.is_none() || self.window.is_active()
    }

    /// A press of one configured hotkey: upstream's `Shortcuts.press`, then whatever the
    /// press produced.
    fn press(&mut self, app: &mut App, entry: &ShortcutAction) {
        let cursor = cursor_point();

        // Without key synthesis the game's copy key cannot be pressed, so a copy action reads
        // the clipboard as it stands: the item the user copied from the game themselves.
        if !self.keys.can_synthesize()
            && let Action::CopyItem { target, focus_overlay } = &entry.action
        {
            let text = self.clipboard.read_text();
            if is_poe_item(&text, self.game) {
                let events = vec![ShortcutEvent::ItemText(ItemText {
                    target: target.clone(),
                    clipboard: text,
                    position: cursor,
                    focus_overlay: *focus_overlay,
                })];
                self.deliver(app, dispatch::shortcut_requests(events));
                self.after_press(app, entry, cursor);
                self.sync_registration();
                return;
            }
            return self.warn(format!(
                "the {target} hotkey needs synthetic keys on this build: copy the item in the game first"
            ));
        }

        let show_mods = show_mods_key(self.game, &self.startup.config);
        let mut overlay_handle = OverlayPanel::new(self.panel.clone());
        let events = {
            let Live { shortcuts, clipboard, keys, window, overlay, .. } = self;
            shortcuts.press(
                entry,
                PressContext {
                    keys,
                    system_clipboard: clipboard,
                    clock: &LiveClock,
                    window,
                    overlay,
                    overlay_handle: &mut overlay_handle,
                    cursor,
                    show_mods_key: &show_mods,
                },
            )
        };
        self.deliver(app, dispatch::shortcut_requests(events));
        self.after_press(app, entry, cursor);
        // `focus-overlay` may have taken the focus from the game.
        self.sync_registration();
    }

    /// What a press leaves behind for the window: where an opened widget goes and how it is
    /// shown. Upstream shows the overlay with `showInactive()`: the game keeps focus and the
    /// keyboard, and `WidgetAreaTracker` activates the overlay only when the mouse reaches the
    /// price check's card. Without a game to be over (macOS, X11, a closed game) the window
    /// *is* the widget and takes the mouse and keyboard like the port always did.
    fn after_press(&mut self, app: &App, entry: &ShortcutAction, cursor: Point) {
        self.placement = Some(Placement::Cursor(cursor));
        self.activate_on_show = self.game_bounds().is_none();
        // `PriceCheckWindow.vue`: the renderer reports the card's area for the normal hotkey
        // (`focusOverlay: false`); the locked one focuses the overlay instead.
        if let Action::CopyItem { target, focus_overlay } = &entry.action
            && target == "price-check"
            && !focus_overlay
            && app.shown() != Shown::None
        {
            self.pending_track = Some(cursor);
        }
    }

    /// Run the shell's side of a press's or a widget's requests, oldest first.
    fn deliver(&mut self, app: &mut App, requests: Vec<dispatch::Request>) {
        for request in requests {
            self.apply(app, request);
        }
    }

    /// One [`dispatch::Request`]: the OS effect, or the window to open.
    fn apply(&mut self, app: &mut App, request: dispatch::Request) {
        match request {
            dispatch::Request::OpenUrl(url) => {
                if !url.starts_with("https://") && !url.starts_with("http://") {
                    return self.warn(format!("refusing to open {url}: not an http(s) URL"));
                }
                if let Err(error) = open_external(&url) {
                    self.warn(format!("opening {url} failed: {error}"));
                }
            }
            dispatch::Request::ConfigFolder => {
                let Some(folder) = self.startup.folder().map(PathBuf::from) else {
                    return self.warn("the config folder has no parent");
                };
                let opened =
                    std::fs::create_dir_all(&folder).and_then(|()| open_external(&folder.display().to_string()));
                if let Err(error) = opened {
                    self.warn(format!("opening {} failed: {error}", folder.display()));
                }
            }
            dispatch::Request::Quit => self.handle.quit(),
            dispatch::Request::Settings => app.open_settings(self),
            dispatch::Request::StashSearch(text) => self.stash_search(&text),
            dispatch::Request::PriceCheck(text) => app.open_price_check(self, &text),
            dispatch::Request::ItemCheck { text, action } => app.open_item_check(self, &text, action),
            dispatch::Request::WidgetAction(target) => app.widget_action(self, &target),
            dispatch::Request::ShowWidget(wm_type) => app.open_widget(self, &wm_type),
            dispatch::Request::Unsupported(what) => self.warn(format!("not wired yet: {what}")),
        }
    }

    /// `CLIENT->MAIN::user-action` with `action: 'stash-search'`: the shortcut module's
    /// clipboard-and-keys sequence, one action per press.
    fn stash_search(&mut self, text: &str) {
        let show_mods = show_mods_key(self.game, &self.startup.config);
        let cursor = cursor_point();
        let mut overlay_handle = OverlayPanel::new(self.panel.clone());
        let Live { shortcuts, clipboard, keys, window, overlay, .. } = self;
        shortcuts.user_stash_search(
            text,
            PressContext {
                keys,
                system_clipboard: clipboard,
                clock: &LiveClock,
                window,
                overlay,
                overlay_handle: &mut overlay_handle,
                cursor,
                show_mods_key: &show_mods,
            },
        );
        self.sync_registration();
    }

    /// A key pressed while the overlay window has the keyboard (`OverlayWindow`'s
    /// `handleExtraCommands`): Escape and Ctrl+W give the game back its focus, the overlay key
    /// toggles, and a key nothing handles stays the widget's. `true` means it was handled.
    ///
    /// The settings window is an ordinary window upstream with no `before-input-event`
    /// handler, so the overlay commands do not apply to it; it keeps its own keys.
    fn overlay_key(&mut self, event: &rune::KeyEvent) -> bool {
        if !overlay_commands_apply(&self.shown, self.overlay.is_interactable()) {
            return false;
        }
        let Some(input) = keys::overlay_input(event) else { return false };
        let mut panel = OverlayPanel::new(self.panel.clone());
        let handled = self.overlay.on_overlay_input(&input, &mut self.window, &mut panel);
        if handled {
            self.sync_registration();
        }
        handled
    }

    /// The native window's visibility, size and placement follow what the app draws.
    ///
    /// With the game's window followed (Windows), the window *is* the overlay: it covers the
    /// game's client area in physical pixels and the widget is drawn inside it at the cursor
    /// ([`App::overlay_offset`]); the overlay library hides it while the game is unfocused.
    /// Without a hook it is the ordinary window the port has had: sized to the widget, next
    /// to the cursor when a hotkey asked for it.
    fn sync_window(&mut self, app: &App, tx: &mut Tx) {
        let shown = app.shown();
        let opened = shown != self.shown;
        let over_game = self.game_bounds();
        if opened {
            self.shown = shown.clone();
            // The overlay's size comes from the game, not from the widget.
            if over_game.is_none()
                && let Some((width, height)) = window_size(&shown)
            {
                tx.resize_window_instant((width, height));
            }
        }

        if shown == Shown::None {
            // Nothing to draw: the next hotkey places its widget afresh.
            self.placement = None;
            self.overlay_offset = None;
            self.overlay_height = None;
            self.activate_on_show = false;
            self.stop_tracking();
            return self.hide_panel();
        }
        // `electron-overlay-window`'s `blur` handler: hide the overlay when neither the game
        // nor the overlay window itself is in front. (`focusNext`: a focus we asked for is not
        // a blur yet.) The settings window is not the overlay — upstream gives it an ordinary
        // window of its own — so it stays while the game is in the background.
        let in_front = self.window.is_active() || self.overlay_focused || self.panel.expects_focus();
        if is_overlay(&shown) && self.watcher.is_some() && !in_front {
            self.overlay_offset = None;
            self.overlay_height = None;
            self.stop_tracking();
            return self.hide_panel();
        }
        // Only the price check's card is followed (`PriceCheckWindow.vue`'s `track-area`); a
        // widget switched to or a card closed drops the listeners, as its `removeListeners` do.
        if !matches!(shown, Shown::PriceCheck | Shown::Poe2) {
            self.stop_tracking();
        }

        let Some(bounds) = over_game else {
            // No game window to be over: the widget's own window, at the cursor.
            self.overlay_offset = None;
            self.overlay_height = None;
            self.activate_on_show = false;
            self.placed = None;
            self.stop_tracking();
            if !self.handle.is_visible() {
                if let Some(Placement::Cursor(point)) = self.placement {
                    self.handle
                        .set_position((point.x.round() as i32 + CURSOR_OFFSET, point.y.round() as i32 + CURSOR_OFFSET));
                }
                self.placement = None;
                self.handle.open();
                self.panel.visible.store(true, Ordering::Relaxed);
            }
            // Without a game to be over, the settings window is still the only way in: it has
            // to take the mouse and the keyboard (the overlay window is click-through by
            // default, `window_options`).
            if shown == Shown::Settings && opened {
                let mut panel = OverlayPanel::new(self.panel.clone());
                self.overlay.assert_overlay_active(&mut self.window, &mut panel);
            }
            return;
        };

        // The overlay covers the game, so its height is what the price check fills.
        self.overlay_height = Some((bounds.height / self.panel.scale()) as f32);
        self.overlay_offset = self.offset_over(&shown, bounds);
        if self.placed != Some(bounds) || !self.handle.is_visible() {
            if !self.handle.is_visible() {
                // `showInactive()`: create/show the overlay without taking the focus or the
                // keyboard from the game. `open()` would activate it — a hotkey's whole point
                // is that the game keeps playing.
                self.handle.show();
                self.panel.visible.store(true, Ordering::Relaxed);
            }
            // The window may not exist yet (Rune creates it on `show`); the next poll places
            // it, because `placed` only records a window that was really moved.
            self.placed =
                self.panel.overlay_hwnd().and_then(|hwnd| window::place_overlay(hwnd, bounds).then_some(bounds));
        }

        // The settings window always takes the mouse and the keyboard; a widget does when the
        // cursor is already on it, which is the tray and the no-game paths. A hotkey-opened
        // widget over the game stays click-through until `WidgetAreaTracker` sees the mouse.
        if (shown == Shown::Settings && opened) || std::mem::take(&mut self.activate_on_show) {
            let mut panel = OverlayPanel::new(self.panel.clone());
            self.overlay.assert_overlay_active(&mut self.window, &mut panel);
            self.sync_registration();
        }

        // `OVERLAY->MAIN::track-area`: the press asked for the card to be followed. The locked
        // price check (`focusOverlay`) took the overlay straight away instead, as upstream's
        // renderer only reports the area when it did not.
        if let Some(cursor) = self.pending_track.take()
            && matches!(shown, Shown::PriceCheck | Shown::Poe2)
            && !self.overlay.is_interactable()
            && let Some(offset) = self.overlay_offset
            && let Some(size) = overlay_widget_size(&shown, bounds, self.panel.scale())
        {
            let area = track_area(
                offset,
                size,
                bounds,
                self.panel.scale(),
                cursor,
                price_check_hold_key(&self.startup.config),
            );
            self.overlay.on_track_area(area);
            if let Some(watcher) = &self.watcher {
                watcher.set_mouse_moves(true);
            }
        }

        // The overlay library's `focus` handler makes the window click-through again; the
        // controller can clear `isInteractable` without telling the window (the game came back
        // through the OS hook), so the two are reconciled here.
        if is_overlay(&shown) {
            let wanted = !self.overlay.is_interactable();
            if self.panel.click_through() != Some(wanted) {
                OverlayPanel::new(self.panel.clone()).set_click_through(wanted);
            }
        }
    }

    /// `WidgetAreaTracker.ts`'s `removeListeners`: stop following the price check's card and
    /// disarm the hook's move stream.
    fn stop_tracking(&mut self) {
        if self.overlay.is_tracking_area() {
            self.overlay.stop_tracking_area();
        }
        self.pending_track = None;
        if let Some(watcher) = &self.watcher {
            watcher.set_mouse_moves(false);
        }
    }

    /// Where the widget goes inside the overlay window, in logical points: at the cursor that
    /// asked for it (clamped into the window), or centred for a window the tray opened.
    fn offset_over(&self, shown: &Shown, bounds: Bounds) -> Option<(f32, f32)> {
        let size = overlay_widget_size(shown, bounds, self.panel.scale())?;
        Some(widget_offset(self.placement, size, bounds, self.panel.scale()))
    }

    /// The game's client area while the OS layer follows the window, in physical pixels.
    fn game_bounds(&self) -> Option<Bounds> {
        self.watcher.as_ref()?;
        self.window.bounds().filter(|bounds| bounds.width > 0.0 && bounds.height > 0.0)
    }

    fn hide_panel(&mut self) {
        self.placed = None;
        if self.handle.is_visible() {
            self.handle.hide();
        }
        // A hidden overlay is click-through again, so the frame after the next show cannot
        // swallow a click before `sync_window` reconciles it.
        if self.panel.click_through() != Some(true) {
            OverlayPanel::new(self.panel.clone()).set_click_through(true);
        }
        self.panel.visible.store(false, Ordering::Relaxed);
    }

    /// `AppSpec::poll`'s live half: tray clicks, update answers, landed prices, the windows'
    /// recorded actions, and the window itself.
    fn poll(&mut self, app: &mut App, tx: &mut Tx) -> bool {
        let mut changed = self.poll_game_window();
        changed |= self.poll_mouse();
        changed |= self.poll_keyboard();

        let actions: Vec<TrayAction> =
            self.tray_actions.lock().map(|mut queue| std::mem::take(&mut *queue)).unwrap_or_default();
        for action in actions {
            changed = true;
            self.apply(app, dispatch::tray_request(action));
        }

        if let Some(info) = self.updates.take() {
            app.apply_update(info);
            changed = true;
        }

        if let Some(prices) = &mut self.prices
            && prices.poll()
        {
            changed = true;
        }

        // The widgets' buttons, and the item check's, record what they want; the shell runs it.
        let mut requests: Vec<dispatch::Request> = Vec::new();
        if let Some(check) = &mut app.item_check {
            requests.extend(check.take_actions().iter().map(dispatch::poe1_item_request));
        }
        if let Some(check) = &mut app.item_check_poe2 {
            requests.extend(check.take_actions().iter().map(dispatch::poe2_item_request));
        }
        if let Some(overlay) = &mut app.widgets {
            requests.extend(overlay.take_actions().iter().map(dispatch::widget_request));
        }
        if !requests.is_empty() {
            changed = true;
            self.deliver(app, requests);
        }

        // Save or Cancel closes the settings window; either way the app re-reads its config so
        // the next hotkey press uses what the file now says.
        let settings_open = app.settings.as_ref().is_some_and(|settings| settings.open);
        if self.settings_was_open && !settings_open {
            self.reload_config();
            changed = true;
        }
        self.settings_was_open = settings_open;

        self.drain_overlay_events(app);
        self.sync_window(app, tx);
        changed
    }
}

impl App {
    /// The live app: idle in the tray, with the shell's state attached.
    pub fn live(live: Live) -> Self {
        let game = live.game;
        let league = live.league.clone();
        // The window that is not shown yet still needs a price check's state. Nothing is
        // parsed (`PriceCheck::empty`), so this does not read the game data.
        let mut price_check = crate::poe1::price_check::PriceCheck::empty(Mode::Live, &league);
        price_check.open = false;
        Self {
            game,
            price_check,
            poe2: None,
            settings: None,
            item_check: None,
            item_check_poe2: None,
            widgets: None,
            mode: Mode::Live,
            waker: None,
            live: Some(live),
        }
    }

    /// `AppSpec::poll` in a window: the live half first (it may open or close windows), then
    /// the windows' own polls.
    pub fn poll_with(&mut self, clock: f64, tx: &mut Tx) -> bool {
        if self.live.is_none() {
            return self.poll(clock);
        }
        let mut live = self.live.take().expect("checked above");
        let live_changed = live.poll(self, tx);
        self.live = Some(live);
        self.poll(clock) || live_changed
    }

    /// [`App::shown`]'s widget inside the overlay window: where it goes, in points, when the
    /// window covers the game (Windows with the game's window followed). `None` when the
    /// window is the widget itself.
    pub fn overlay_offset(&self) -> Option<(f32, f32)> {
        self.live.as_ref().and_then(|live| live.overlay_offset)
    }

    /// The widget's size inside the overlay window, in points: what [`crate::view`] lays the
    /// card out at. The price check takes the overlay's whole height because upstream's
    /// `#price-window` is `h-full` and its listings scroll inside it; the other cards keep
    /// their own size. `None` when the window is the widget itself.
    pub fn overlay_size(&self) -> Option<(f32, f32)> {
        let live = self.live.as_ref()?;
        live.overlay_offset?;
        let height = live.overlay_height?;
        let (width, own_height) = window_size(&self.shown())?;
        Some(match self.shown() {
            Shown::PriceCheck | Shown::Poe2 => (width, height),
            _ => (width, own_height),
        })
    }

    /// `OverlayVisibility`: a lone Alt has the widgets peeked away. What the app draws is then
    /// empty ([`crate::view`]), while the window and the widget's state stay: the game keeps the
    /// mouse and keyboard it had, and nothing closes on a focus change.
    pub fn is_peeked(&self) -> bool {
        self.live.as_ref().is_some_and(|live| peek_hides(&self.shown(), live.overlay.is_visible()))
    }

    /// What the window draws, in [`crate::view`]'s order.
    pub fn shown(&self) -> Shown {
        if let Some(overlay) = &self.widgets
            && let Some(widget) = overlay.shown_widget()
            && widget.wm_wants == widgets::Wants::Show
        {
            return Shown::Widget(widget.wm_type.clone());
        }
        if let Some(settings) = &self.settings
            && settings.open
        {
            return Shown::Settings;
        }
        if self.item_check.as_ref().is_some_and(|check| check.open) {
            return Shown::ItemCheck;
        }
        if self.item_check_poe2.as_ref().is_some_and(|check| check.open) {
            return Shown::ItemCheck;
        }
        match self.game {
            GameId::Poe2 => {
                if self.poe2.as_ref().is_some_and(|check| check.open) {
                    Shown::Poe2
                } else if self.price_check.open {
                    Shown::PriceCheck
                } else {
                    Shown::None
                }
            }
            _ => {
                if self.price_check.open {
                    Shown::PriceCheck
                } else if self.poe2.as_ref().is_some_and(|check| check.open) {
                    Shown::Poe2
                } else {
                    Shown::None
                }
            }
        }
    }

    /// The window's close button: every widget is closed, and the shell hides the window.
    pub fn close_windows(&mut self) {
        self.price_check.open = false;
        if let Some(check) = &mut self.poe2 {
            check.open = false;
        }
        if let Some(settings) = &mut self.settings {
            settings.cancel();
        }
        if let Some(check) = &mut self.item_check {
            check.open = false;
        }
        if let Some(check) = &mut self.item_check_poe2 {
            check.open = false;
        }
        if let Some(overlay) = &mut self.widgets
            && let Some(widget) = overlay.widget_mut(overlay.shown)
        {
            widget.set_wants(widgets::Wants::Hide);
        }
    }

    /// `MAIN->OVERLAY::hide-exclusive-widget`: the cursor moved away from the price check's
    /// card, so the window hides itself (`PriceCheckWindow.vue` is the only listener).
    pub fn close_exclusive_widget(&mut self) {
        self.price_check.open = false;
        if let Some(check) = &mut self.poe2 {
            check.open = false;
        }
    }

    /// `OverlayWindow.vue`'s `focus-change` handler with the overlay no longer in front: every
    /// widget flagged `hide-on-blur` goes. Both price checks of the port carry it, as the item
    /// check and most overlay widgets do upstream; a widget without it stays on screen.
    pub fn hide_widgets_on_blur(&mut self) {
        self.close_exclusive_widget();
        if let Some(check) = &mut self.item_check {
            check.open = false;
        }
        if let Some(check) = &mut self.item_check_poe2 {
            check.open = false;
        }
        if let Some(overlay) = &mut self.widgets {
            let id = overlay.shown;
            if overlay.widget(id).is_some_and(|widget| widget.has_flag("hide-on-blur"))
                && let Some(widget) = overlay.widget_mut(id)
            {
                widget.set_wants(widgets::Wants::Hide);
            }
        }
    }

    /// A global hotkey fired: one press, one action, as upstream's `Shortcuts.press`.
    fn on_hotkey(&mut self, entry: &ShortcutAction) {
        let Some(mut live) = self.live.take() else { return };
        live.press(self, entry);
        self.live = Some(live);
    }

    /// `AppSpec::on_focus`: whether the overlay window itself has the focus. The library's
    /// `blur` rule needs it — a widget whose window is in front stays while the game is not —
    /// and it settles a focus the overlay asked for (`focusNext`).
    pub fn on_focus(&mut self, focused: bool, _tx: &mut Tx) {
        if let Some(live) = &mut self.live {
            live.overlay_focused = focused;
            live.panel.focus_settled();
        }
    }

    /// `AppSpec::on_key`: keys the window's widget did not take. While the overlay holds the
    /// keyboard they are upstream's `handleExtraCommands` (Escape and Ctrl+W give the game its
    /// focus, the overlay key toggles); otherwise the widget keeps them.
    pub fn on_key(&mut self, event: &rune::KeyEvent, _tx: &mut Tx) -> bool {
        let Some(mut live) = self.live.take() else { return false };
        let handled = live.overlay_key(event);
        self.live = Some(live);
        handled
    }

    /// The tray's "Settings/League", a widget's edit button, or `wm.show` on the settings
    /// widget: upstream's settings window, over the game's widget.
    fn open_settings(&mut self, live: &mut Live) {
        let mut settings = Settings::open_live(live.game, Mode::Live);
        settings.update = live.update.clone();
        if let Some(waker) = &self.waker {
            settings.set_waker(waker.clone());
        }
        live.settings_was_open = true;
        self.settings = Some(settings);
        live.placement = Some(Placement::Centred);
        live.activate_on_show = true;
    }

    /// `MAIN->CLIENT::item-text` for `price-check`: the window for the copied item, in the
    /// league the trade site lists.
    fn open_price_check(&mut self, live: &mut Live, text: &str) {
        let warm = live.prices.take();
        let overviews = live.poe2_prices.take();
        let league = live.league.clone();
        self.open_price_check_for(live.game, &league, Mode::Live, text, warm, overviews);
        self.settings = None;
        // The press recorded where the cursor was; that is where the widget goes.
    }

    /// [`App::open_price_check`] without the shell: the window itself, which a test drives
    /// with the recorded answers (`MAIN->CLIENT::item-text`'s whole effect).
    ///
    /// `warm` is the prices the app fetched at startup (`usePoeninja` is shared by every
    /// window upstream), and `overviews` EE2's; either is used in place of a fresh request.
    pub fn open_price_check_for(
        &mut self,
        game: GameId,
        league: &str,
        mode: Mode,
        text: &str,
        warm: Option<crate::poe1::price_check::prices::PriceList>,
        overviews: Option<Vec<(&'static str, String)>>,
    ) {
        let waker = self.waker.clone();
        match game {
            GameId::Poe2 => {
                let mut check = crate::poe2::price_check::PriceCheck::with_mode_league(text, mode, league);
                if let Some(overviews) = overviews {
                    check.apply_price_overviews(&overviews);
                }
                if let Some(waker) = &waker {
                    check.set_waker(waker.clone());
                }
                self.game = GameId::Poe2;
                self.poe2 = Some(check);
            }
            _ => {
                let mut check = crate::poe1::price_check::PriceCheck::with_mode_league(text, mode, league);
                if let Some(mut warm) = warm {
                    // The startup fetch is the app's prices; the window's own request goes
                    // with the empty list it replaces.
                    std::mem::swap(&mut check.prices, &mut warm);
                    if let Some(waker) = &waker {
                        check.prices.set_waker(waker.clone());
                    }
                }
                if let Some(waker) = &waker {
                    check.set_waker(waker.clone());
                }
                self.game = GameId::Poe1;
                self.price_check = check;
            }
        }
    }

    /// `MAIN->CLIENT::item-text` for `item-check`, and its own hotkeys, which press the
    /// button whose page they open.
    fn open_item_check(&mut self, live: &mut Live, text: &str, action: Option<ItemAction>) {
        self.open_item_check_for(live.game, &live.startup.config, Mode::Live, text, action);
        if self.item_check.as_ref().is_some_and(|check| check.open)
            || self.item_check_poe2.as_ref().is_some_and(|check| check.open)
        {
            self.settings = None;
            // The press recorded where the cursor was (see `Live::press`).
        } else {
            live.warn(format!("the copied text is not a {:?} item", live.game));
            return;
        }
        let requests: Vec<dispatch::Request> = match live.game {
            GameId::Poe2 => self
                .item_check_poe2
                .as_mut()
                .map(|check| check.take_actions().iter().map(dispatch::poe2_item_request).collect())
                .unwrap_or_default(),
            _ => self
                .item_check
                .as_mut()
                .map(|check| check.take_actions().iter().map(dispatch::poe1_item_request).collect())
                .unwrap_or_default(),
        };
        live.deliver(self, requests);
    }

    /// [`App::open_item_check`] without the shell, for a test: the widget on `text`, with the
    /// requested hotkey action recorded in its `actions` (`WidgetItemCheck.vue`'s
    /// `hotkeyable-actions`).
    pub fn open_item_check_for(
        &mut self,
        game: GameId,
        config: &serde_json::Value,
        mode: Mode,
        text: &str,
        action: Option<ItemAction>,
    ) {
        match game {
            GameId::Poe2 => {
                let mut check = crate::poe2::item_check::ItemCheck::from_config(text, config, mode);
                match action {
                    Some(ItemAction::Wiki) => check.open_wiki(),
                    Some(ItemAction::Poedb) => check.open_poedb(),
                    Some(ItemAction::CraftOfExile) => check.open_coe(),
                    Some(ItemAction::Similar) => check.find_similar_items(),
                    None => {}
                }
                if check.open {
                    self.game = GameId::Poe2;
                    self.item_check_poe2 = Some(check);
                }
            }
            _ => {
                let mut check = crate::poe1::item_check::ItemCheck::from_config(text, config);
                match action {
                    Some(ItemAction::Wiki) => check.open_wiki(),
                    Some(ItemAction::Poedb) => check.open_poedb(),
                    Some(ItemAction::CraftOfExile) => check.open_coe(),
                    Some(ItemAction::Similar) => check.find_similar_items(),
                    None => {}
                }
                if check.open {
                    self.game = GameId::Poe1;
                    self.item_check = Some(check);
                }
            }
        }
    }

    /// `MAIN->CLIENT::widget-action`: a widget's hotkey (the delve grid's toggle, a timer's
    /// play/pause).
    fn widget_action(&mut self, live: &mut Live, target: &str) {
        let now = crate::trade::now(Mode::Live) as f64;
        let config = live.startup.config.clone();
        let overlay = self.widgets.get_or_insert_with(|| widgets::Overlay::live(live.game, &config));
        // A timer's own state has to exist before its toggle (the target names its `wmId`).
        if let Some(wm_id) = target.rsplit(':').next().and_then(|id| id.parse::<u32>().ok())
            && overlay.widget(wm_id).is_some()
        {
            overlay.focus(wm_id, Mode::Live);
        }
        overlay.widget_action(target, now);
        // The widget whose type the target names becomes the drawn one, unless its toggle just
        // hid it.
        let wm_type = target.split(':').next().unwrap_or(target);
        let shown = overlay
            .widgets
            .iter()
            .find(|widget| widget.wm_type == wm_type && widget.wm_wants == widgets::Wants::Show)
            .map(|widget| widget.wm_id);
        if let Some(wm_id) = shown {
            overlay.focus(wm_id, Mode::Live);
        }
        live.placement = Some(Placement::Centred);
        live.activate_on_show = true;
    }

    /// Draw a widget of `wm_type` in the overlay window (`wm.show(wmId)` from the menu).
    fn open_widget(&mut self, live: &mut Live, wm_type: &str) {
        let config = live.startup.config.clone();
        let overlay = self.widgets.get_or_insert_with(|| widgets::Overlay::live(live.game, &config));
        if !overlay.show_type(wm_type, Mode::Live) {
            // No instance in the config: `wm.create`, which the scene's `initialised` has.
            let id = overlay.create(wm_type);
            overlay.focus(id, Mode::Live);
        }
        live.placement = Some(Placement::Centred);
        live.activate_on_show = true;
    }

    /// The update worker's answer, for the settings window's About page (`Host.updateInfo`).
    fn apply_update(&mut self, info: UpdateInfo) {
        if let Some(live) = &mut self.live {
            live.update = info.clone();
        }
        if let Some(settings) = &mut self.settings {
            settings.update = info;
        }
    }

    /// The startup worker's answers: the trade site's leagues, the selected league and EE2's
    /// poe.ninja overviews.
    fn apply_startup(
        &mut self,
        leagues: Leagues,
        league: Option<String>,
        overviews: Option<Vec<(&'static str, String)>>,
    ) {
        let Some(live) = &mut self.live else { return };
        if let Some(error) = &leagues.error {
            live.warn(format!("loading the leagues failed: {error}"));
        }
        if let Some(league) = league.filter(|league| *league != live.league) {
            live.warn(format!("price checks now use the league \"{league}\""));
            live.league = league.clone();
            if let Some(prices) = &mut live.prices {
                prices.queue(&league, true);
            }
        }
        if let Some(overviews) = overviews {
            // A window that is already open takes them now; otherwise the next one does.
            if let Some(check) = &mut self.poe2 {
                check.apply_price_overviews(&overviews);
            }
            live.poe2_prices = Some(overviews);
        }
    }
}

/// `HostConfig` from the game's config, or a fresh install's defaults when the config cannot
/// be translated (upstream quits in that case; a widget app should still start).
fn host_config(game: GameId, config: &serde_json::Value) -> HostConfig {
    let translated = match game {
        GameId::Poe2 => wraex_poe2::config::host_config(config),
        _ => wraex_poe1::config::host_config(config),
    };
    translated.ok().and_then(|value| serde_json::from_value(value).ok()).unwrap_or_else(|| {
        serde_json::from_value(json!({
            "shortcuts": [],
            "restoreClipboard": true,
            "stashScroll": true,
            "logKeys": false,
            "overlayKey": "Shift + Space",
            "windowTitle": "",
            "language": "en"
        }))
        .expect("the fallback host config")
    })
}

/// `GameConfig.showModsKey`: the game's "show advanced item descriptions" key from its own
/// INI, or `Alt` when the file is not where the port looks (upstream's default).
pub fn show_mods_key(game: GameId, config: &serde_json::Value) -> String {
    let configured = config.get("gameConfig").and_then(|value| value.as_str()).unwrap_or_default();
    let path = if configured.is_empty() {
        user_dirs().and_then(|dirs| {
            let paths = game_files::config_paths(game, current_os(), &dirs);
            game_files::guess_file_location(&paths)
        })
    } else {
        Some(PathBuf::from(configured))
    };
    path.as_deref().and_then(game_files::show_mods_key).unwrap_or_else(|| "Alt".to_owned())
}

/// The league a fresh install prices in, per game.
pub fn default_league(game: GameId) -> &'static str {
    match game {
        GameId::Poe2 => crate::poe2::price_check::DEFAULT_LEAGUE,
        _ => crate::poe1::price_check::DEFAULT_LEAGUE,
    }
}

/// The native window's size for what it draws. The price check is a fixed 28.75 rem column;
/// the settings window is upstream's 50 × 38 rem.
pub(crate) fn window_size(shown: &Shown) -> Option<(f32, f32)> {
    match shown {
        Shown::None => None,
        Shown::Settings => Some((crate::settings::WIDTH, crate::settings::HEIGHT)),
        Shown::ItemCheck | Shown::PriceCheck | Shown::Poe2 => Some((crate::poe1::price_check::WIDTH, HEIGHT)),
        Shown::Widget(name) => widgets::Kind::from_name(name).map(|kind| widgets::scene_size(GameId::Poe1, kind)),
    }
}

/// Opens `target` with the OS: Electron's `shell.openExternal`/`openPath`.
pub fn open_external(target: &str) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(target);
        command
    };
    // `rundll32` rather than `cmd /C start`: no `cmd.exe` parsing, so a URL's `&` stays in
    // the URL.
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("rundll32");
        command.arg("url.dll,FileProtocolHandler").arg(target);
        command
    };
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(target);
        command
    };
    command.spawn().map(|_| ())
}

/// The user folders the game's files live under, from the environment.
pub fn user_dirs() -> Option<UserDirs> {
    let home = std::env::var_os("HOME").map(PathBuf::from)?;
    Some(UserDirs { documents: home.join("Documents"), app_data: settings::app_data_dir()?, home })
}

/// The app's OS: the platform modules' selector.
pub fn current_os() -> Os {
    Os::current().unwrap_or(Os::Linux)
}

/// The cursor's position in physical desktop pixels, or upstream's `(0, 0)` when the platform
/// cannot say.
fn cursor_point() -> Point {
    keys::cursor_position().map(|(x, y)| Point { x, y }).unwrap_or(Point { x: 0.0, y: 0.0 })
}

/// The overlay's own native window and the game window it sits over. Created in [`run`]
/// before the app, because Rune hands the window over when it creates it (`on_window`), and
/// shared with every [`OverlayPanel`], which lives as long as one press.
#[derive(Default)]
pub struct Panel {
    /// The Rune window, once it exists.
    window: Mutex<Option<rune::WindowHandle>>,
    /// Its native handle (unused off Windows).
    overlay: Mutex<Option<isize>>,
    /// The tracked game window's handle, for [`OverlayHandle::focus_game`].
    game: Mutex<Option<isize>>,
    /// What [`OverlayHandle::is_visible`] answers: the shell's own bookkeeping, since Rune
    /// has no query for it.
    visible: AtomicBool,
    /// The click-through the overlay last asked for, `None` until it asks once. Kept because
    /// the request can arrive before Rune has created the window (`Live::press` activates it
    /// straight away), and re-applied if the window is re-created.
    click_through: Mutex<Option<bool>>,
    /// The overlay is the one taking the keys, so a window created now takes the focus
    /// (`activateOverlay`'s `electronWindow.focus()`).
    take_focus: AtomicBool,
    /// Frames left in which a focus the overlay asked for counts as "in front": the window is
    /// not hidden in between (`electron-overlay-window`'s `focusNext`). A countdown, because a
    /// focus Windows refuses is never answered, and the window must not stay in front forever.
    expect_focus: AtomicU8,
}

impl std::fmt::Debug for Panel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Panel")
            .field("overlay", &self.overlay_hwnd())
            .field("game", &self.game.lock().ok().and_then(|game| *game))
            .field("visible", &self.visible.load(Ordering::Relaxed))
            .field("click_through", &self.click_through.lock().ok().and_then(|click_through| *click_through))
            .finish_non_exhaustive()
    }
}

impl Panel {
    /// Rune created (or re-created) the window: keep it, and on Windows make it the tool
    /// window an overlay should be (no taskbar button, no Alt+Tab entry).
    pub fn attach(&self, window: &rune::WindowHandle) {
        let hwnd = window
            .with_raw_handle(|handle| match handle.as_raw() {
                rune::raw_window_handle::RawWindowHandle::Win32(win32) => Some(win32.hwnd.get()),
                _ => None,
            })
            .ok()
            .flatten();
        if let Some(hwnd) = hwnd {
            window::configure_overlay(hwnd);
        }
        if let Ok(mut current) = self.window.lock() {
            *current = Some(window.clone());
        }
        if let Ok(mut current) = self.overlay.lock() {
            *current = hwnd;
        }
        // A click-through request may have arrived before the window existed.
        if let Some(click_through) = self.click_through.lock().ok().and_then(|click_through| *click_through)
            && let Err(error) = window.set_click_through(click_through)
        {
            eprintln!("wraex: the overlay's click-through could not be set: {error}");
        }
        if self.take_focus.load(Ordering::Relaxed) {
            window.focus();
        }
    }

    fn window(&self) -> Option<rune::WindowHandle> {
        self.window.lock().ok().and_then(|window| window.clone())
    }

    fn overlay_hwnd(&self) -> Option<isize> {
        self.overlay.lock().ok().and_then(|hwnd| *hwnd)
    }

    fn click_through(&self) -> Option<bool> {
        self.click_through.lock().ok().and_then(|click_through| *click_through)
    }

    /// Whether the focus was asked for and has not arrived (yet): `focusNext`. Counted down,
    /// so a focus Windows refuses cannot hold the overlay in front.
    fn expects_focus(&self) -> bool {
        let left = self.expect_focus.try_update(Ordering::Relaxed, Ordering::Relaxed, |left| left.checked_sub(1));
        left.unwrap_or(0) > 0
    }

    /// The focus question was answered — the window or the game took it — so `focusNext` no
    /// longer holds the overlay on screen.
    fn focus_settled(&self) {
        self.expect_focus.store(0, Ordering::Relaxed);
    }

    /// The window's DPI scale, for turning the hook's physical pixels into points.
    fn scale(&self) -> f64 {
        self.overlay_hwnd().map_or(1.0, window::dpi_scale)
    }
}

/// [`OverlayHandle`] over the live window: what [`OverlayController`] asks the OS for — show,
/// hide, click-through, the game's focus back.
#[derive(Debug)]
struct OverlayPanel {
    panel: Arc<Panel>,
}

impl OverlayPanel {
    fn new(panel: Arc<Panel>) -> Self {
        Self { panel }
    }
}

impl OverlayHandle for OverlayPanel {
    fn show(&mut self) {
        if let Some(window) = self.panel.window() {
            window.show();
        }
        self.panel.visible.store(true, Ordering::Relaxed);
    }

    fn hide(&mut self) {
        if let Some(window) = self.panel.window() {
            window.hide();
        }
        self.panel.visible.store(false, Ordering::Relaxed);
    }

    /// `activateOverlay`/`focusTarget`: taking the mouse also takes the keyboard, and giving
    /// the mouse back gives the game the focus.
    fn set_click_through(&mut self, click_through: bool) {
        if let Ok(mut current) = self.panel.click_through.lock() {
            *current = Some(click_through);
        }
        // The overlay takes the keyboard while it takes the mouse.
        self.panel.take_focus.store(!click_through, Ordering::Relaxed);
        self.panel.expect_focus.store(if click_through { 0 } else { FOCUS_PATIENCE }, Ordering::Relaxed);
        // No window yet: `Panel::attach` applies both when Rune creates one.
        let Some(window) = self.panel.window() else { return };
        if let Err(error) = window.set_click_through(click_through) {
            eprintln!("wraex: the overlay's click-through could not be changed: {error}");
            return;
        }
        if !click_through {
            window.focus();
        }
    }

    fn focus_game(&mut self) {
        let Some(game) = self.panel.game.lock().ok().and_then(|game| *game) else { return };
        if !window::focus_window(game) && cfg!(target_os = "windows") {
            eprintln!("wraex: could not give the game's window its focus back");
        }
    }

    fn is_visible(&self) -> bool {
        self.panel.visible.load(Ordering::Relaxed)
    }
}

/// `--game poe1|poe2`, from the process's arguments. Anything else (or no flag) is PoE1,
/// the app upstream ships first.
pub fn game_from_args(args: &[String]) -> GameId {
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let value = match arg.strip_prefix("--game=") {
            Some(value) => Some(value),
            None if arg == "--game" => args.next().map(String::as_str),
            None => None,
        };
        if let Some(value) = value {
            return match value {
                "poe2" => GameId::Poe2,
                _ => GameId::Poe1,
            };
        }
    }
    GameId::Poe1
}

/// Run the app for real: a tray, global hotkeys, and windows opened on demand. Upstream ships
/// one app per game; this binary picks with `--game`.
pub fn run(game: GameId) {
    let startup = config::startup(game);
    eprintln!("wraex: {:?}, config {} ({:?})", game, startup.store.path().display(), startup.source);
    let configured_league = startup
        .config
        .get("leagueId")
        .and_then(|value| value.as_str())
        .filter(|league| !league.is_empty())
        .map(str::to_owned);

    let size = (crate::poe1::price_check::WIDTH, HEIGHT);
    let spec = AppSpec::<App> {
        title: "Wraex",
        size,
        // The window is one widget at a time and is resized when another one opens.
        min_size: Some((crate::poe1::price_check::WIDTH, 200.0)),
        init: App::default,
        view,
        scenes: &[],
        fonts: crate::theme::FONTS,
        families: crate::theme::FAMILIES,
        setup: Some(|app, waker| app.set_waker(waker)),
        poll: Some(|app, clock, tx| app.poll_with(clock, tx)),
        on_key: Some(App::on_key),
    };
    // The overlay's window, and the game's, are handed to `Live` as one shared value: Rune
    // passes the window over when it creates it, before `Live` exists.
    let panel = Arc::new(Panel::default());
    let window_panel = panel.clone();
    let mut app = rune::App::new(spec)
        // Nothing shows until a hotkey or the tray opens a widget; the app lives in the tray.
        .windowless()
        .keep_running()
        // Transparent, borderless, on top and click-through: the widget window is the
        // overlay's (`electron-overlay-window`'s `OVERLAY_WINDOW_OPTS`).
        .window_options(window_options())
        .on_window(move |window| window_panel.attach(window))
        // The overlay's own focus, for the library's hide-on-blur rule (`App::on_focus`).
        .on_focus(App::on_focus)
        // Upstream binds `Shift + Space` by default, which Rune's parser refuses without ⌘/⌃/⌥.
        .allow_unmodified_shortcuts()
        .standard_menus()
        .on_close(|app, _tx| app.close_windows());
    let handle = app.handle();

    let live = Live::new(handle.clone(), startup, panel);
    for warning in &live.warnings {
        eprintln!("wraex: {warning}");
    }
    app = app.init(move || App::live(live));

    // The tray and the hotkeys need the running host: this update runs on the UI thread as
    // soon as the loop starts.
    handle.update(|app, _tx| {
        if let Some(live) = &mut app.live {
            live.boot();
        }
    });

    spawn_startup_work(handle.clone(), game, configured_league);
    spawn_update_check(handle.clone());
    app.run();
}

/// The app window's native options. On Windows the window is the overlay over the game:
/// transparent, borderless, always on top, click-through, never focused on open and with no
/// minimum size (the widget inside it is placed by the shell). Off Windows it is the ordinary
/// window the port has always had.
fn window_options() -> rune::WindowOptions {
    let mut options = rune::WindowOptions::default();
    if cfg!(target_os = "windows") {
        options.transparent = true;
        options.borderless = true;
        options.click_through = true;
        options.always_on_top = true;
        options.resizable = false;
        options.min_size = Some((0.0, 0.0));
    }
    options
}

/// [`Live::poll`]'s startup work on a worker: the trade site's leagues, the game data EE2's
/// parser needs, and poe.ninja's prices.
fn spawn_startup_work(handle: AppHandle<App>, game: GameId, configured: Option<String>) {
    std::thread::Builder::new()
        .name("wraex-startup".to_owned())
        .spawn(move || {
            let transport = wraex_platform::http::HttpTransport::default();
            let source = match game {
                GameId::Poe2 => LeagueSource::Poe2,
                _ => LeagueSource::Poe1,
            };
            let mut leagues = Leagues::default();
            let fallback = leagues.load(&transport, source, TRADE_HOST, configured.as_deref());
            let league = fallback.or(configured).or_else(|| leagues.list.first().map(|league| league.id.clone()));

            let mut overviews: Option<Vec<(&'static str, String)>> = None;
            if game == GameId::Poe2 {
                // `TradeData.ts`: the trade site's items and stats, which the parser and
                // `createPresets` read. A failure leaves the empty sets, as upstream's does.
                let _ = crate::poe2::price_check::data(Mode::Live);
                if let Some(league) = &league {
                    overviews = crate::poe2::price_check::prices::fetch(league).ok();
                }
            }

            handle.update(move |app, _tx| app.apply_startup(leagues, league, overviews));
        })
        .expect("start the startup worker");
}

/// The update worker: one check at launch, then one per [`updates::CHECK_INTERVAL_MS`].
fn spawn_update_check(handle: AppHandle<App>) {
    let notify = handle.clone();
    let worker = updates::Updates::spawn(
        move |info| {
            let notify = notify.clone();
            notify.update(move |app, _tx| app.apply_update(info));
        },
        Some(NoDownloadReason::NotSupported),
    );
    handle.update(move |app, _tx| {
        if let Some(live) = &mut app.live {
            live.updates = worker;
        }
    });
}

#[cfg(test)]
mod tests;
