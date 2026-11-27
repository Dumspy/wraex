//! `ref/apt/renderer/src/web/settings`: the settings window, as one Rune scene per page.
//!
//! `SettingsWindow.vue` edits a clone of the app's config and only writes it on Save
//! (`updateConfig(configClone)` then `saveConfig()`); Cancel throws the clone away. This keeps
//! the same split: [`Settings::config`] is the working copy, [`Settings::saved`] the config the
//! window opened on, and [`Settings::save`] writes the working copy through
//! [`ConfigStore`](wraex_core::config_store::ConfigStore) exactly as `saveConfig` does
//! (`JSON.stringify(AppConfig())`, so compact JSON).
//!
//! The pages are [`Page`]s, each with its own `settings-poe1-<page>` / `settings-poe2-<page>`
//! scene. The live shell opens the window with [`Settings::open_live`] (the tray's
//! "Settings/League", a widget's edit button), and after it closes re-reads the file and
//! re-applies the host half, which is upstream's `updateConfig`/`pushHostConfig`
//! ([`crate::shell::Live`]). Save itself still stops at the store.
//!
//! Ported for both games: general, hotkeys (with the recorder), price check, chat, map check,
//! item check, debug and about, plus EE2's Leveling page for PoE2. Still missing: EE2's Help
//! page, the league list
//! (`useLeagues`), the patrons strip's names, the debug log tail, and poe.ninja's currency
//! icons on EE2's primary-currency radios.
//!
//! A scene renders at the settings window's own size ([`WIDTH`] × [`HEIGHT`], upstream's
//! `.window` at most 50 × 38 rem): `--snap --scene settings-…` needs no `--size`, and the
//! price-check scenes keep the price-check window's size.
//!
//! One known layout difference: upstream's page is a block in normal flow, where adjacent
//! vertical margins collapse to the larger one, while Rune lays the page out as a flex column
//! and adds them. A page with an `mt` next to an `mb` therefore runs a few points taller than
//! its screenshot; the horizontal geometry, colours and labels match.

pub mod hotkey;
pub mod pages;
pub mod window;

use std::collections::BTreeMap;
use std::path::PathBuf;

use rune::{Latest, TextEdit, Waker};
use serde_json::Value;
use wraex_core::GameId;
use wraex_core::config_store::ConfigStore;
use wraex_core::updates::UpdateInfo;

use crate::trade::Mode;

/// The settings window's maximum size: `.window`'s `max-width: 50rem; max-height: 38rem`.
pub const WIDTH: f32 = 50.0 * crate::theme::REM;
pub const HEIGHT: f32 = 38.0 * crate::theme::REM;

/// How a view reaches the settings state: Rune runs a handler with the app's state (`&mut S`),
/// so a window that isn't the whole app is rendered with the projection that finds it.
pub type Project<S> = fn(&mut S) -> &mut Settings;

/// One menu page (`SettingsWindow.vue`'s `menuByType` default branch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Hotkeys,
    Chat,
    General,
    PriceCheck,
    Maps,
    ItemCheck,
    /// EE2's Leveling page (`web/leveling/settings-leveling.vue`), PoE2 only.
    Leveling,
    Debug,
    About,
}

impl Page {
    /// Every page, in menu order.
    pub const ALL: [Page; 9] = [
        Page::Hotkeys,
        Page::Chat,
        Page::General,
        Page::PriceCheck,
        Page::Maps,
        Page::ItemCheck,
        Page::Leveling,
        Page::Debug,
        Page::About,
    ];

    /// The menu's groups: a separator goes under each group (`flatJoin`).
    pub const MENU: [&'static [Page]; 5] = [
        &[Page::Hotkeys, Page::Chat],
        &[Page::General],
        &[Page::PriceCheck, Page::Maps, Page::ItemCheck],
        &[Page::Leveling],
        &[Page::Debug, Page::About],
    ];

    /// Whether the game has this page: APT has no leveling widget, so its settings window has
    /// no Leveling page either.
    pub fn applies_to(self, game: GameId) -> bool {
        self != Page::Leveling || game == GameId::Poe2
    }

    /// The pages of `game`, in menu order.
    pub fn for_game(game: GameId) -> impl Iterator<Item = Page> {
        Page::ALL.into_iter().filter(move |page| page.applies_to(game))
    }

    /// The `--scene` suffix and the element keys' prefix.
    pub fn slug(self) -> &'static str {
        match self {
            Page::Hotkeys => "hotkeys",
            Page::Chat => "chat",
            Page::General => "general",
            Page::PriceCheck => "price-check",
            Page::Maps => "maps",
            Page::ItemCheck => "item-check",
            Page::Leveling => "leveling",
            Page::Debug => "debug",
            Page::About => "about",
        }
    }

    pub fn from_slug(slug: &str) -> Option<Self> {
        Page::ALL.into_iter().find(|page| page.slug() == slug)
    }

    /// The menu item's label (`t(component.name)`, where the component names its i18n key).
    pub fn title_key(self) -> &'static str {
        match self {
            Page::Hotkeys => "settings.hotkeys",
            Page::Chat => "settings.chat",
            Page::General => "settings.general",
            Page::PriceCheck => "price_check.name",
            Page::Maps => "map_check.name",
            Page::ItemCheck => "item.info",
            Page::Leveling => "leveling.name",
            Page::Debug => "settings.debug",
            Page::About => "settings.about",
        }
    }
}

/// A field of the config the pages read and write: a top-level key, a widget's key
/// (`findWidget(type, config)`), a chat command's key, or a key of the item check's `maps`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Root(&'static str),
    Widget(&'static str, &'static str),
    Command(usize, &'static str),
    Maps(&'static str),
}

impl Field {
    pub fn read(self, config: &Value) -> &Value {
        match self {
            Field::Root(key) => config.get(key).unwrap_or(&Value::Null),
            Field::Widget(wm_type, key) => widget(config, wm_type).and_then(|w| w.get(key)).unwrap_or(&Value::Null),
            Field::Command(index, key) => {
                config["commands"].get(index).and_then(|command| command.get(key)).unwrap_or(&Value::Null)
            }
            Field::Maps(key) => widget(config, "item-check")
                .and_then(|w| w.get("maps"))
                .and_then(|maps| maps.get(key))
                .unwrap_or(&Value::Null),
        }
    }

    pub fn write(self, config: &mut Value, value: Value) {
        let target = match self {
            Field::Root(_) => Some(config),
            Field::Widget(wm_type, _) => widget_mut(config, wm_type),
            Field::Command(index, _) => config.get_mut("commands").and_then(|commands| commands.get_mut(index)),
            Field::Maps(_) => maps_mut(config),
        };
        let key = match self {
            Field::Root(key) | Field::Maps(key) => key,
            Field::Widget(_, key) | Field::Command(_, key) => key,
        };
        if let Some(target) = target
            && let Some(object) = target.as_object_mut()
        {
            object.insert(key.to_owned(), value);
        }
    }
}

/// `findWidget(type, config)`
pub fn widget<'a>(config: &'a Value, wm_type: &str) -> Option<&'a Value> {
    config["widgets"].as_array()?.iter().find(|w| w["wmType"] == wm_type)
}

/// `findWidget(type, config)`, writable.
pub fn widget_mut<'a>(config: &'a mut Value, wm_type: &str) -> Option<&'a mut Value> {
    config["widgets"].as_array_mut()?.iter_mut().find(|w| w["wmType"] == wm_type)
}

/// The item check's `maps` object (`ItemCheckWidget.maps`).
pub fn maps_mut(config: &mut Value) -> Option<&mut Value> {
    widget_mut(config, "item-check")?.get_mut("maps")
}

/// `t(key)` for the window's game.
pub fn t(game: GameId, key: &str) -> String {
    match game {
        GameId::Poe2 => crate::poe2::i18n::t(key),
        _ => crate::poe1::i18n::t(key),
    }
}

/// `t(key, args)` for the window's game.
pub fn fmt(game: GameId, key: &str, args: &[&str]) -> String {
    match game {
        GameId::Poe2 => crate::poe2::i18n::fmt(key, args),
        _ => crate::poe1::i18n::fmt(key, args),
    }
}

/// The about page's `checkForUpdates` request, off the UI thread: [`Latest`] has no `Debug`,
/// and the window keeps its derive.
struct Check(Latest<UpdateInfo>);

impl std::fmt::Debug for Check {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Check")
    }
}

/// The settings window's state.
#[derive(Debug)]
pub struct Settings {
    pub game: GameId,
    pub page: Page,
    /// `wmWants`: the window shows while this is true.
    pub open: bool,
    /// `configClone`: the working copy the pages edit.
    pub config: Value,
    /// The config the window opened on (or last saved): Cancel and a reopen go back to it.
    pub saved: Value,
    /// Text fields by element key: their text is the working copy's value, and each edit
    /// writes it back (`v-model`/`v-model.number`).
    pub edits: BTreeMap<String, TextEdit>,
    /// The hotkey field that is capturing keys (`HotkeyInput.vue`'s focused input).
    pub recording: Option<hotkey::Recording>,
    /// The `<select>` whose list is open, by element key.
    pub select_open: Option<&'static str>,
    /// An unreadable file was replaced by the defaults: saves go to `config.json.tmp`.
    pub save_temporary: bool,
    /// Where Save writes (`ConfigStore`).
    pub store: ConfigStore,
    /// HTTPS or the recorded answers, for the league list, prices and the update check.
    pub mode: Mode,
    /// `Host.updateInfo`: the about page's update state.
    pub update: UpdateInfo,
    /// The update check running off the UI thread; its answer lands in [`Settings::poll`].
    check: Check,
    /// `settings-maps`' area mods, read from the game data the first time the page shows.
    pub area_mods: Option<Vec<pages::maps::AreaMod>>,
    /// `settings-maps`' search text and Only-selected toggle (view state, not config).
    pub maps_search: String,
    pub maps_only_selected: bool,
    /// Why the last Save failed, shown in the footer (upstream quits instead).
    pub save_error: Option<String>,
}

impl Settings {
    /// A window on `config`, with `page` selected and its text fields filled in.
    pub fn new(game: GameId, page: Page, config: Value, store: ConfigStore, mode: Mode) -> Self {
        let mut settings = Self {
            game,
            page,
            open: true,
            saved: config.clone(),
            config,
            edits: BTreeMap::new(),
            recording: None,
            select_open: None,
            save_temporary: false,
            store,
            mode,
            update: UpdateInfo::Initial,
            check: Check(Latest::default()),
            area_mods: None,
            maps_search: String::new(),
            maps_only_selected: false,
            save_error: None,
        };
        settings.seed_edits();
        settings.load_area_mods();
        settings
    }

    /// A scene/test window: the default config (`config::default_config`), in a temp-dir store
    /// so nothing reads or writes the user's real settings file. Its answers are the recorded
    /// ones, like the price checks a scene opens beside it.
    pub fn scene(game: GameId, page: Page) -> Self {
        Self::new(game, page, default_config(game), temp_store(game), Mode::Replay)
    }

    /// A window over the user's config file: `initConfig()` (load, upgrade, or the defaults).
    /// The live shell opens it from the tray and from a widget's edit button.
    pub fn open_live(game: GameId, mode: Mode) -> Self {
        let store = ConfigStore::new(config_path(game).unwrap_or_else(|| fallback_path(game)));
        let loaded = load_store(game, &store).unwrap_or_else(|| load(game, None));
        let mut settings = Self::new(game, Page::Hotkeys, loaded.config, store, mode);
        settings.save_temporary = loaded.save_temporary;
        settings
    }

    /// The waker a landed update check uses to run `AppSpec::poll`.
    pub fn set_waker(&self, waker: Waker) {
        self.check.0.set_waker(waker);
    }

    /// Takes a landed update check, if any: `poll` returns true when it applied one.
    pub fn poll(&mut self) -> bool {
        let Some(update) = self.check.0.take() else { return false };
        self.update = update;
        true
    }

    /// `checkForUpdates`: [`UpdateInfo::CheckingForUpdate`] shows at once, and the one request
    /// runs on a worker (the answer lands in [`Settings::poll`]).
    pub fn begin_update_check(&mut self) {
        self.update = UpdateInfo::CheckingForUpdate;
        let now = crate::trade::now(self.mode);
        let mode = self.mode;
        self.check.0.request((), move |()| update_check(mode, now));
    }

    /// `saveConfig()`: the working copy becomes the config, written as compact JSON.
    pub fn save(&mut self) -> std::io::Result<()> {
        let json = serde_json::to_string(&self.config).expect("the config is JSON");
        self.store.save(&json, self.save_temporary)?;
        self.saved = self.config.clone();
        self.open = false;
        Ok(())
    }

    /// `cancel()`: hide the window; the working copy is dropped on the next open.
    pub fn cancel(&mut self) {
        self.open = false;
        self.recording = None;
        self.select_open = None;
    }

    /// Reopen on the saved config, as `watch(wmWants)` rebuilds `configClone`.
    pub fn reopen(&mut self) {
        self.config = self.saved.clone();
        self.open = true;
        self.recording = None;
        self.select_open = None;
        self.seed_edits();
    }

    /// Switch page (`selectedComponent.value = component`). The page's fields are re-seeded
    /// from the working copy, because a field is found by its element key.
    pub fn open_page(&mut self, page: Page) {
        self.page = page;
        self.recording = None;
        self.select_open = None;
        self.save_error = None;
        self.seed_edits();
        self.load_area_mods();
    }

    /// `settings-maps`' `findAllAreaMods()`: only the maps page reads the game data, so the
    /// window loads it when that page first shows.
    pub fn load_area_mods(&mut self) {
        if self.page == Page::Maps && self.area_mods.is_none() {
            self.area_mods = Some(pages::maps::area_mods(self.game, self.mode));
        }
    }

    /// Rebuild this page's text fields from the working copy (`addCommand`/`removeCommand` and
    /// a page switch both change which fields exist).
    pub fn seed_edits(&mut self) {
        self.edits.clear();
        let text = |field: Field| match field.read(&self.config) {
            Value::String(text) => text.clone(),
            Value::Null => String::new(),
            other => other.to_string(),
        };
        let number = |field: Field| match field.read(&self.config) {
            Value::Number(number) => wraex_core::js::number_to_string(number.as_f64().unwrap_or_default()),
            other => other.to_string(),
        };
        let mut edits = BTreeMap::new();
        match self.page {
            Page::General => {
                edits.insert("general-font-size-input".to_owned(), TextEdit::new(number(Field::Root("fontSize"))));
                edits.insert("general-client-log".to_owned(), TextEdit::new(text(Field::Root("clientLog"))));
                edits.insert("general-game-config".to_owned(), TextEdit::new(text(Field::Root("gameConfig"))));
                edits.insert("general-window-title-input".to_owned(), TextEdit::new(text(Field::Root("windowTitle"))));
            }
            Page::PriceCheck => {
                edits.insert("price-check-league-custom".to_owned(), TextEdit::new(text(Field::Root("leagueId"))));
                edits.insert("price-check-account".to_owned(), TextEdit::new(text(Field::Root("accountName"))));
                edits.insert(
                    "price-check-stat-range-input".to_owned(),
                    TextEdit::new(number(Field::Widget("price-check", "searchStatRange"))),
                );
                edits.insert(
                    "price-check-api-latency-input".to_owned(),
                    TextEdit::new(number(Field::Widget("price-check", "apiLatencySeconds"))),
                );
                edits.insert(
                    "price-check-initial-delay-input".to_owned(),
                    TextEdit::new(number(Field::Widget("price-check", "initialDelay"))),
                );
            }
            Page::Chat => {
                for (index, command) in self.commands().iter().enumerate() {
                    let text = command.get("text").and_then(Value::as_str).unwrap_or_default();
                    edits.insert(format!("chat-command-{index}"), TextEdit::new(text));
                }
            }
            Page::Maps => {
                edits.insert("maps-search".to_owned(), TextEdit::new(self.maps_search.clone()));
            }
            Page::Leveling => {
                edits.insert("leveling-client-log".to_owned(), TextEdit::new(text(Field::Root("clientLog"))));
            }
            _ => {}
        }
        self.edits = edits;
    }

    /// `props.config.commands`
    pub fn commands(&self) -> &[Value] {
        self.config["commands"].as_array().map(Vec::as_slice).unwrap_or_default()
    }

    /// The fields' texts, by element key.
    pub fn edit(&self, key: &str) -> Option<&TextEdit> {
        self.edits.get(key)
    }

    pub fn edit_mut(&mut self, key: &str) -> &mut TextEdit {
        self.edits.get_mut(key).unwrap_or_else(|| panic!("no text field keyed `{key}`"))
    }

    /// The text of a field (`v-model`'s value), or empty.
    pub fn edit_text(&self, key: &str) -> String {
        self.edits.get(key).map(|edit| edit.text.clone()).unwrap_or_default()
    }

    /// The boolean at `field` (`configModelValue(..., key)` with a checkbox).
    pub fn bool(&self, field: Field) -> bool {
        field.read(&self.config).as_bool().unwrap_or(false)
    }

    /// The string at `field`; null and missing are empty.
    pub fn string(&self, field: Field) -> String {
        field.read(&self.config).as_str().unwrap_or_default().to_owned()
    }

    /// Write a JSON value, as a `configModelValue` setter does.
    pub fn set(&mut self, field: Field, value: Value) {
        field.write(&mut self.config, value);
    }

    pub fn set_bool(&mut self, field: Field, value: bool) {
        self.set(field, Value::Bool(value));
    }

    pub fn set_string(&mut self, field: Field, value: &str) {
        self.set(field, Value::String(value.to_owned()));
    }

    /// A number, as `v-model.number` writes it.
    pub fn set_f64(&mut self, field: Field, value: f64) {
        if let Some(number) = serde_json::Number::from_f64(value) {
            self.set(field, Value::Number(number));
        }
    }

    /// `v-model.number`: the parsed number, if the text is one.
    pub fn set_number(&mut self, field: Field, text: &str) {
        if let Ok(number) = text.trim().parse::<f64>()
            && let Some(number) = serde_json::Number::from_f64(number)
        {
            self.set(field, Value::Number(number));
        }
    }
}

/// The update check's one request: GitHub over HTTPS in a window, the recorded answers in a
/// headless run (there is none for `api.github.com`, so a scene's click shows the offline error
/// rather than reaching the network).
fn update_check(mode: Mode, now: u64) -> UpdateInfo {
    match mode {
        Mode::Live => wraex_core::updates::check(&wraex_platform::http::HttpTransport::default(), now, None),
        Mode::Replay => wraex_core::updates::check(&crate::replay::ReplayTransport::poe1(), now, None),
    }
}

/// `defaultConfig()`, per game.
pub fn default_config(game: GameId) -> Value {
    match game {
        GameId::Poe2 => wraex_poe2::config::default_config(),
        _ => wraex_poe1::config::default_config(),
    }
}

pub(crate) fn load_store(game: GameId, store: &ConfigStore) -> Option<Loaded> {
    match store.load() {
        Ok(contents) => contents.map(|text| load(game, Some(&text))),
        Err(error) => {
            wraex_platform::log!("could not read {}: {error}", store.path().display());
            Some(Loaded { config: default_config(game), save_temporary: true })
        }
    }
}

/// `initConfig()`'s reading of a file: the config and whether an unreadable file made it a
/// temporary save.
#[derive(Debug, Clone, PartialEq)]
pub struct Loaded {
    pub config: Value,
    pub save_temporary: bool,
}

/// `initConfig()`'s reading of a file, per game.
pub fn load(game: GameId, contents: Option<&str>) -> Loaded {
    // A broken upgrade keeps the defaults rather than panicking in the UI; the reference quits.
    let loaded = match game {
        GameId::Poe2 => wraex_poe2::config::load(contents)
            .map(|loaded| Loaded { config: loaded.config, save_temporary: loaded.save_temporary }),
        _ => wraex_poe1::config::load(contents)
            .map(|loaded| Loaded { config: loaded.config, save_temporary: loaded.save_temporary }),
    };
    loaded.unwrap_or_else(|_| Loaded { config: default_config(game), save_temporary: true })
}

/// Where Wraex keeps its settings: Electron's `appData` under `Wraex/<the game's folder>`,
/// beside the folder the reference uses (which [`ConfigStore::load`] can import from).
pub fn config_path(game: GameId) -> Option<PathBuf> {
    Some(config_path_in(game, &app_data_dir()?))
}

/// [`config_path`] under an explicit `appData`, so a test can point it at a temp dir.
pub fn config_path_in(game: GameId, app_data: &std::path::Path) -> PathBuf {
    let folder = match game {
        GameId::Poe2 => "Path of Exile 2",
        _ => "Path of Exile",
    };
    app_data.join("Wraex").join(folder).join("config.json")
}

/// Where the HTTP transport saves the session's expiring cookies: `cookies.json` in
/// `Wraex`, beside the per-game settings folders. Electron's default session is one
/// session for both games, so the jar is one file too.
pub fn cookies_path() -> Option<PathBuf> {
    Some(cookies_path_in(&app_data_dir()?))
}

/// [`cookies_path`] under an explicit `appData`, so a test can point it at a temp dir.
pub fn cookies_path_in(app_data: &std::path::Path) -> PathBuf {
    app_data.join("Wraex").join("cookies.json")
}

/// `app.getPath('appData')`.
pub fn app_data_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
    }
}

/// A path a scene can always write, when `appData` is unset.
fn fallback_path(game: GameId) -> PathBuf {
    std::env::temp_dir().join("wraex").join(game_folder(game)).join("config.json")
}

fn game_folder(game: GameId) -> &'static str {
    match game {
        GameId::Poe2 => "poe2",
        _ => "poe1",
    }
}

/// A store under the system temp dir: scenes and tests use it so they never touch the user's
/// real config file (`AGENTS.md`).
pub fn temp_store(game: GameId) -> ConfigStore {
    ConfigStore::new(std::env::temp_dir().join("wraex-settings").join(game_folder(game)).join("config.json"))
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
