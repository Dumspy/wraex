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
//! What is *not* here is PLAN.md's phase 2: finding and following the game window, a real
//! click-through overlay, and low-level key/wheel hooks. The window is an ordinary Rune
//! window placed near the cursor ([`keys::cursor_position`]), hotkeys are registered all the
//! time instead of only while the game is focused, and [`OverlayWindow`] only records what
//! [`OverlayController`] asks for. Where a hotkey cannot be synthesized (Linux, or macOS
//! without Input Monitoring permission), a price check prices the item already on the
//! clipboard instead of pressing the game's copy key.
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
use std::sync::{Arc, Mutex};

use rune::{AppHandle, AppSpec, Tx};
use serde_json::json;
use wraex_core::GameId;
use wraex_core::game_files::{self, Os, UserDirs};
use wraex_core::leagues::{LeagueSource, Leagues};
use wraex_core::updates::{NoDownloadReason, UpdateInfo};
use wraex_platform::clipboard::{Clipboard, is_poe_item};
use wraex_platform::overlay::{OverlayController, OverlayHandle};
use wraex_platform::shortcuts::{Action, HostConfig, ItemText, PressContext, ShortcutAction, ShortcutEvent, Shortcuts};
use wraex_platform::tray::{Tray, TrayAction};
use wraex_platform::window::{GameWindow, Point};

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
    /// The next window belongs at the cursor (a hotkey opened it), not where the last one was.
    at_cursor: bool,
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
    /// the prices the first price check will use.
    pub fn new(handle: AppHandle<App>, startup: config::Startup) -> Self {
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
            at_cursor: false,
        }
    }

    /// The app's log line for a problem: printed once, and kept for the debug page.
    pub fn warn(&mut self, message: impl Into<String>) {
        let message = message.into();
        eprintln!("wraex: {message}");
        self.warnings.push(message);
    }

    /// The first turn of the event loop: the tray and the configured hotkeys, both of which
    /// need Rune's host to exist (the tray's native menu on macOS, `register_shortcut`).
    pub fn boot(&mut self) {
        self.start_tray();
        self.register_hotkeys();
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
        if let Some(tray) = &mut self.tray
            && let Err(error) = tray.set_overlay_key(self.host.overlay_key.clone())
        {
            self.warn(format!("rebuilding the tray menu failed: {error}"));
        }
        self.register_hotkeys();
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
                return self.deliver(app, dispatch::shortcut_requests(events));
            }
            return self.warn(format!(
                "the {target} hotkey needs synthetic keys on this build: copy the item in the game first"
            ));
        }

        let show_mods = show_mods_key(self.game, &self.startup.config);
        let mut overlay_handle = OverlayWindow::default();
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
        self.at_cursor = true;
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
        let mut overlay_handle = OverlayWindow::default();
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
    }

    /// The native window's visibility and size follow what the app draws. Closing a widget
    /// hides the window (the app stays in the tray); opening one shows it, at the cursor when
    /// a hotkey asked for it.
    fn sync_window(&mut self, app: &App, tx: &mut Tx) {
        let shown = app.shown();
        if shown != self.shown {
            self.shown = shown.clone();
            if let Some((width, height)) = window_size(&shown) {
                tx.resize_window_instant((width, height));
            }
        }
        if shown == Shown::None {
            if self.handle.is_visible() {
                self.handle.hide();
            }
            return;
        }
        if !self.handle.is_visible() {
            if self.at_cursor
                && let Some((x, y)) = keys::cursor_position()
            {
                self.handle.set_position((x.round() as i32 + CURSOR_OFFSET, y.round() as i32 + CURSOR_OFFSET));
            }
            self.at_cursor = false;
            self.handle.open();
        }
    }

    /// `AppSpec::poll`'s live half: tray clicks, update answers, landed prices, the windows'
    /// recorded actions, and the window itself.
    fn poll(&mut self, app: &mut App, tx: &mut Tx) -> bool {
        let mut changed = false;

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

    /// A global hotkey fired: one press, one action, as upstream's `Shortcuts.press`.
    fn on_hotkey(&mut self, entry: &ShortcutAction) {
        let Some(mut live) = self.live.take() else { return };
        live.press(self, entry);
        self.live = Some(live);
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
        live.at_cursor = false;
    }

    /// `MAIN->CLIENT::item-text` for `price-check`: the window for the copied item, in the
    /// league the trade site lists.
    fn open_price_check(&mut self, live: &mut Live, text: &str) {
        let warm = live.prices.take();
        let overviews = live.poe2_prices.take();
        let league = live.league.clone();
        self.open_price_check_for(live.game, &league, Mode::Live, text, warm, overviews);
        self.settings = None;
        live.at_cursor = true;
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
            live.at_cursor = true;
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
        live.at_cursor = false;
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
        live.at_cursor = false;
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
fn window_size(shown: &Shown) -> Option<(f32, f32)> {
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

/// The overlay's OS half, as far as this milestone goes: what [`OverlayController`] asked
/// for, without an overlay window (PLAN.md phase 2 owns the window, click-through and focus).
#[derive(Debug, Default)]
struct OverlayWindow {
    click_through: bool,
    visible: bool,
}

impl OverlayHandle for OverlayWindow {
    fn show(&mut self) {
        self.visible = true;
    }

    fn hide(&mut self) {
        self.visible = false;
    }

    fn set_click_through(&mut self, click_through: bool) {
        self.click_through = click_through;
    }

    fn focus_game(&mut self) {}

    fn is_visible(&self) -> bool {
        self.visible
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
        on_key: None,
    };
    let mut app = rune::App::new(spec)
        // Nothing shows until a hotkey or the tray opens a widget; the app lives in the tray.
        .windowless()
        .keep_running()
        // Upstream binds `Shift + Space` by default, which Rune's parser refuses without ⌘/⌃/⌥.
        .allow_unmodified_shortcuts()
        .standard_menus()
        .on_close(|app, _tx| app.close_windows());
    let handle = app.handle();

    let live = Live::new(handle.clone(), startup);
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
