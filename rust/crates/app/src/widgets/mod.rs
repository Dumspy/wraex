//! `ref/apt/renderer/src/web/overlay/` and the widgets it hosts, in Rune.
//!
//! The overlay is one page full of *widgets*: `AppConfig().widgets` lists them, each with a
//! `wmType`, a `wmTitle`, a `wmWants` (`show`/`hide`) and an `anchor`. `OverlayWindow.vue` draws
//! every visible one at its anchor; `Widget.vue` is the frame around each (the hover actions
//! panel, the edit/move modes, the move handles). Four of them are ported here; the item and
//! stash searches live in their own modules:
//!
//! - [`menu`]: `WidgetMenu.vue`, the launcher bar — a button per hidden widget, the cog that
//!   opens the settings window, the "add widget…" popover and the price-check paste box.
//! - [`stopwatch`]: `stopwatch/WidgetTimer.vue` — a timer with play/pause/reset and a `paused`
//!   badge.
//! - [`image_strip`]: `WidgetImageStrip.vue` — a titled strip of pictures ("cheat sheets").
//! - [`delve_grid`]: `WidgetDelveGrid.vue` — the 11×9 cell grid over the Delve map.
//! - [`item_search`]: `item-search/WidgetItemSearch.vue` — the name search that prices a base
//!   type, with the strip of picks that dispatch `MAIN->CLIENT::item-text`.
//! - [`stash_search`]: `stash-search/WidgetStashSearch.vue` — the configured stash searches, one
//!   `CLIENT->MAIN::user-action` per press.
//!
//! Each is a scene of its own (`widget-menu`, `widget-stopwatch`, `widget-image-strip`,
//! `widget-delve-grid`, `widget-item-search`, `widget-stash-search`, and `poe2-widget-…` for
//! EE2's copies), one widget per window: the
//! screenshots in `tools/golden/screens/widgets.mjs` crop exactly one widget's card, so a
//! scene's window *is* that card (see [`scene_size`]), filled with what the reference's overlay
//! paints behind its widgets ([`crate::theme::overlay_background`]) so a translucent widget
//! composites the same way. What a scene does not draw is the rest of the overlay: the anchors
//! only decide which side a widget's actions panel hangs on, and `wmZorder` is not modelled,
//! because no scene shows two widgets at once.
//!
//! One consequence of a card-sized window: the hover actions panel hangs *outside* the card (as
//! `Widget.vue`'s `position: absolute` panel does), so in a scene it is clipped. It is still in
//! the element tree, and a test that presses its buttons renders in a window with room for it
//! (`tests::runtime_panel`); the app's own window is the whole overlay, where it shows as
//! upstream's does.
//!
//! Widgets act through the app shell, as upstream's do: showing, hiding or creating a widget
//! edits the config the shell owns (`wm.show` in `OverlayWindow.vue`) and records an [`Action`]
//! for the shell, so a scene or a test reads what happened without a window. Dragging a widget
//! to a new anchor, the image strip's file picker and the drag-and-drop reordering of its images
//! stay with the shell and are not ported (they record an [`Action`] each).
//!
//! The widgets' own hotkeys (`toggleKey`, `resetKey`) are the platform's to bind; what the main
//! process sends while they are bound, `MAIN->CLIENT::widget-action`, is
//! [`Overlay::widget_action`].
//!
//! Still missing from the reference's widget frame: the fade-in of its panel (`Widget.vue`'s
//! `:hover` with no transition upstream either), the `delete` countdown's 1 s progress gradient
//! (the port deletes on release), and the DnD library's reorder animation.

pub mod delve_grid;
pub mod icon;
pub mod image_strip;
pub mod item_search;
pub mod menu;
pub mod stash_search;
pub mod stopwatch;

use std::collections::BTreeMap;

use rune::{Color, Cx, El, TextEdit, col, el, row, text};
use serde_json::{Value, json};
use wraex_core::GameId;

use crate::theme;
use crate::ui::art;

/// How a view reaches the overlay's state: Rune runs a handler with the app's state (`&mut S`),
/// so a window that isn't the whole app is rendered with the projection that finds it.
pub type Project<S> = fn(&mut S) -> &mut Overlay;

/// A widget this module draws, by its scene name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Menu,
    Stopwatch,
    ImageStrip,
    DelveGrid,
    ItemSearch,
    StashSearch,
}

impl Kind {
    /// Every ported widget.
    pub const ALL: [Kind; 6] =
        [Kind::Menu, Kind::Stopwatch, Kind::ImageStrip, Kind::DelveGrid, Kind::ItemSearch, Kind::StashSearch];

    /// The `--scene` suffix (`widget-<name>`) and the enum's name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Menu => "menu",
            Kind::Stopwatch => "stopwatch",
            Kind::ImageStrip => "image-strip",
            Kind::DelveGrid => "delve-grid",
            Kind::ItemSearch => "item-search",
            Kind::StashSearch => "stash-search",
        }
    }

    /// The `wmType` the config lists it under (`WidgetSpec.type`).
    pub fn wm_type(self) -> &'static str {
        match self {
            Kind::Menu => menu::WM_TYPE,
            Kind::Stopwatch => stopwatch::WM_TYPE,
            Kind::ImageStrip => image_strip::WM_TYPE,
            Kind::DelveGrid => delve_grid::WM_TYPE,
            Kind::ItemSearch => item_search::WM_TYPE,
            Kind::StashSearch => stash_search::WM_TYPE,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// `wmWants`: whether the overlay draws the widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Wants {
    Show,
    Hide,
}

impl Wants {
    pub fn as_str(self) -> &'static str {
        match self {
            Wants::Show => "show",
            Wants::Hide => "hide",
        }
    }

    /// `wmWants` as the config writes it; anything else is `hide`, as upstream's `=== 'show'`.
    pub fn parse(text: Option<&str>) -> Self {
        match text {
            Some("show") => Wants::Show,
            _ => Wants::Hide,
        }
    }
}

/// One overlay widget, as `AppConfig().widgets` holds it (`widgets.ts`'s `Widget`).
///
/// Only the fields the ported widgets read are lifted out; each widget's own settings
/// (`anchor`, `images`, `alwaysShow`, `toggleKey`, ...) stay in [`Widget::config`], the JSON the
/// config has, so a new widget type needs no change here.
#[derive(Debug, Clone, PartialEq)]
pub struct Widget {
    pub wm_id: u32,
    /// `wmType`: `menu`, `settings`, `timer`, `image-strip`, `delve-grid`, ...
    pub wm_type: String,
    /// `wmTitle`, which may carry an icon: `{icon=fa-cog}`.
    pub wm_title: String,
    pub wm_wants: Wants,
    /// `wmFlags`: `invisible-on-blur`, `hide-on-focus`, `menu::skip`, ...
    pub wm_flags: Vec<String>,
    /// The whole widget object the config holds.
    pub config: Value,
}

impl Widget {
    /// A widget from its config object; `None` when it has no `wmId`/`wmType`.
    pub fn from_config(config: &Value) -> Option<Self> {
        Some(Self {
            wm_id: u32::try_from(config.get("wmId")?.as_u64()?).ok()?,
            wm_type: config.get("wmType")?.as_str()?.to_owned(),
            wm_title: config.get("wmTitle").and_then(Value::as_str).unwrap_or_default().to_owned(),
            wm_wants: Wants::parse(config.get("wmWants").and_then(Value::as_str)),
            wm_flags: flags(config),
            config: config.clone(),
        })
    }

    pub fn has_flag(&self, flag: &str) -> bool {
        self.wm_flags.iter().any(|it| it == flag)
    }

    /// `wm.setFlag`: adds or removes one flag; returns whether the widget changed.
    pub fn set_flag(&mut self, flag: &str, on: bool) -> bool {
        let has = self.has_flag(flag);
        if on == has {
            return false;
        }
        if on {
            self.wm_flags.push(flag.to_owned());
        } else {
            self.wm_flags.retain(|it| it != flag);
        }
        if let Some(flags) = self.config.get_mut("wmFlags").and_then(Value::as_array_mut) {
            *flags = self.wm_flags.iter().cloned().map(Value::String).collect();
        }
        true
    }

    /// `wmWants = ...`, as `wm.show`/`wm.hide` write it; returns whether the widget changed.
    pub fn set_wants(&mut self, wants: Wants) -> bool {
        if self.wm_wants == wants {
            return false;
        }
        self.wm_wants = wants;
        if let Some(object) = self.config.as_object_mut() {
            object.insert("wmWants".to_owned(), Value::String(wants.as_str().to_owned()));
        }
        true
    }

    /// `wmTitle.match(/^(?:\{icon=(?<icon>[^}]+)\}\s*)?(?<title>.*)$/)`, then `WidgetMenu.vue`'s
    /// fallback: the title (or `#<wmId>` when there is neither an icon nor a title).
    ///
    /// Returns `(title, icon)`. An icon-only title (`{icon=fa-cog}`) has no title.
    pub fn title_and_icon(&self) -> (Option<String>, Option<String>) {
        let (icon, title) = match self.wm_title.strip_prefix("{icon=").and_then(|rest| rest.split_once('}')) {
            Some((icon, title)) => (Some(icon.to_owned()), title.trim_start().to_owned()),
            None => (None, self.wm_title.clone()),
        };
        let title = if !title.is_empty() {
            Some(title)
        } else if icon.is_none() {
            Some(format!("#{}", self.wm_id))
        } else {
            None
        };
        (title, icon)
    }

    /// `anchor.x`, `anchor.y`: the percentages `Widget.vue` positions the card at, and which
    /// side its actions panel hangs on.
    pub fn anchor(&self) -> (f32, f32) {
        let anchor = self.config.get("anchor");
        let axis = |name: &str| anchor.and_then(|a| a.get(name)).and_then(Value::as_f64).unwrap_or(0.0) as f32;
        (axis("x"), axis("y"))
    }
}

/// `wmFlags` as strings (`widgets.ts` lists the well-known ones, the edge cases are free-form).
fn flags(config: &Value) -> Vec<String> {
    config
        .get("wmFlags")
        .and_then(Value::as_array)
        .map(|flags| flags.iter().filter_map(|flag| flag.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

/// What a widget's chrome asked the app to do, instead of doing it.
///
/// Upstream's widgets call the widget manager (`wm.show`, `wm.hide`, `wm.create`, `wm.remove`),
/// which edits the app's config and asks the main process to re-publish it; the port records the
/// call so the shell can do exactly that, and a scene or a test can read it.
// The variants that carry text are wider than the ids; boxing them would make every producer
// allocate and every test unwrap.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    /// `wm.show(wmId)`.
    Show(u32),
    /// `wm.hide(wmId)`.
    Hide(u32),
    /// `wm.create(wmType)`: the "add widget…" popover.
    Create(String),
    /// `wm.remove(wmId)`: `Widget.vue`'s `delete` button.
    Remove(u32),
    /// `Widget.vue`'s `edit` on a widget that edits inline (`inline-edit: false`): the timer sets
    /// `settings::widget=<wmId>` on the settings widget and shows it.
    EditInSettings(u32),
    /// `Host.selfDispatch({ name: 'MAIN->CLIENT::item-text', … })`: the menu's paste box, whose
    /// text the price check opens at the box's centre.
    PriceCheck(String),
    /// `Host.importFile(file)`: the image strip's "Choose File". The shell opens the dialog and
    /// appends the chosen file's name to the widget's `images`.
    ImportImage(u32),
    /// `MainProcess.sendEvent({ name: 'CLIENT->MAIN::user-action', payload: { action:
    /// 'stash-search', text } })`: the stash search's button. The shell types the text into the
    /// stash's search box (one action per press).
    StashSearch(String),
    /// `Host.selfDispatch({ name: 'MAIN->CLIENT::item-text', payload: { target: 'price-check',
    /// item } })`: the item search's picked entry, already parsed, which reopens the price check
    /// on it ([`crate::App::poll`] applies it).
    PriceCheckItem(item_search::Picked),
}

impl Action {
    /// The event the app shell sends for this action, for the log and the tests.
    pub fn kind(&self) -> &'static str {
        match self {
            Action::Show(_) => "show",
            Action::Hide(_) => "hide",
            Action::Create(_) => "create",
            Action::Remove(_) => "remove",
            Action::EditInSettings(_) => "edit-in-settings",
            Action::PriceCheck(_) => "price-check",
            Action::ImportImage(_) => "import-image",
            Action::StashSearch(_) => "stash-search",
            Action::PriceCheckItem(_) => "price-check-item",
        }
    }
}

/// The overlay's state: the config's widgets, the one this window draws, and the widgets' own
/// running state (`WidgetTimer.vue`'s clock, `WidgetMenu.vue`'s popover, ...).
#[derive(Debug)]
pub struct Overlay {
    pub game: GameId,
    /// `AppConfig().widgets`, in the config's order.
    pub widgets: Vec<Widget>,
    /// The `wmId` of the widget this window draws. A scene picks one; the app would draw every
    /// widget whose `wmWants` is `show` at its anchor.
    pub shown: u32,
    /// `Widget.vue`'s hover state: the widget whose actions panel shows.
    pub hover: Option<u32>,
    /// The pointer is over the actions panel: upstream's `.widget:hover` covers its own panel,
    /// which hangs outside the card, so the panel keeps showing while it is used.
    pub panel_hover: Option<u32>,
    /// `Widget.vue`'s `isEditing`, for the widgets that edit inline.
    pub editing: Option<u32>,
    /// `Widget.vue`'s `isMoving`: the move handles are drawn.
    pub moving: Option<u32>,
    /// `WidgetMenu.vue`'s "add widget…" popover.
    pub add_open: bool,
    /// `WidgetMenu.vue`'s `<textarea>` (`v-model` is the element's value, cleared on paste).
    pub paste: TextEdit,
    /// `WidgetImageStrip.vue`'s title input, while editing.
    pub title: TextEdit,
    /// `WidgetTimer.vue`'s state, per timer widget (`instances: 'multi'`).
    pub timers: BTreeMap<u32, stopwatch::Timer>,
    /// `WidgetItemSearch.vue`'s state (`instances: 'single'`), once its scene asks for it.
    pub item_search: Option<item_search::State>,
    /// What the widgets asked the shell for, oldest first ([`Overlay::take_actions`]).
    pub actions: Vec<Action>,
}

impl Overlay {
    /// The overlay on `game`'s default config, drawing `kind`'s widget.
    ///
    /// The stopwatch is an `instances: 'multi'` widget that `defaultConfig()` does not create, so
    /// its scene adds one as `WidgetTimer.vue`'s `initInstance` does (and as the menu's "add
    /// widget…" would). Every other scene shows the config's own instance and marks it `show`,
    /// which is what the widget menu's button, or `wm.show`, does before a screenshot.
    pub fn scene(game: GameId, kind: Kind) -> Self {
        let config = crate::settings::default_config(game);
        let mut widgets: Vec<Widget> = config
            .get("widgets")
            .and_then(Value::as_array)
            .map(|widgets| widgets.iter().filter_map(Widget::from_config).collect())
            .unwrap_or_default();
        if !widgets.iter().any(|widget| widget.wm_type == kind.wm_type()) {
            let id = widgets.iter().map(|widget| widget.wm_id).max().unwrap_or(0) + 1;
            widgets.push(Widget::from_config(&initialised(kind.wm_type(), id)).expect("a widget object"));
        }
        let shown = widgets.iter().find(|widget| widget.wm_type == kind.wm_type()).map(|widget| widget.wm_id);
        let mut overlay = Self {
            game,
            widgets,
            shown: shown.unwrap_or(0),
            hover: None,
            panel_hover: None,
            editing: None,
            moving: None,
            add_open: false,
            paste: TextEdit::default(),
            title: TextEdit::default(),
            timers: BTreeMap::new(),
            item_search: None,
            actions: Vec::new(),
        };
        if let Some(id) = shown {
            // `wm.show` when the scene opens, without an [`Action`]: the scene is the widget
            // being on screen, not a button press.
            if let Some(widget) = overlay.widget_mut(id) {
                widget.set_wants(Wants::Show);
            }
            if overlay.widget(id).is_some_and(|widget| widget.wm_type == stopwatch::WM_TYPE) {
                overlay.timers.insert(id, stopwatch::Timer::default());
            }
            if overlay.widget(id).is_some_and(|widget| widget.wm_type == item_search::WM_TYPE) {
                // A scene reads the prices the screenshots recorded; a window would build the
                // state with `Mode::for_run()`.
                overlay.item_search = Some(item_search::State::new(game, crate::trade::Mode::Replay));
            }
            let title = overlay.widget(id).map(|widget| widget.wm_title.clone()).unwrap_or_default();
            overlay.title = TextEdit::new(title);
        }
        overlay
    }

    pub fn widget(&self, wm_id: u32) -> Option<&Widget> {
        self.widgets.iter().find(|widget| widget.wm_id == wm_id)
    }

    pub fn widget_mut(&mut self, wm_id: u32) -> Option<&mut Widget> {
        self.widgets.iter_mut().find(|widget| widget.wm_id == wm_id)
    }

    /// The widget this window draws.
    pub fn shown_widget(&self) -> Option<&Widget> {
        self.widget(self.shown)
    }

    /// The timer widget's state, once it exists.
    pub fn timer(&self, wm_id: u32) -> Option<&stopwatch::Timer> {
        self.timers.get(&wm_id)
    }

    /// `wm.show(wmId)`: the config's `wmWants` becomes `show` and the shell hears about it.
    pub fn show(&mut self, wm_id: u32) {
        if self.widget_mut(wm_id).is_some_and(|widget| widget.set_wants(Wants::Show)) {
            self.actions.push(Action::Show(wm_id));
        }
    }

    /// `wm.hide(wmId)`.
    pub fn hide(&mut self, wm_id: u32) {
        if self.widget_mut(wm_id).is_some_and(|widget| widget.set_wants(Wants::Hide)) {
            self.actions.push(Action::Hide(wm_id));
        }
    }

    /// `wm.setFlag(wmId, flag, state)`.
    pub fn set_flag(&mut self, wm_id: u32, flag: &str, on: bool) {
        if let Some(widget) = self.widget_mut(wm_id) {
            widget.set_flag(flag, on);
        }
    }

    /// `wm.create(wmType)`: a new widget of a `multi` type, which shows itself (its component's
    /// `uninitialized` branch runs on mount).
    pub fn create(&mut self, wm_type: &str) -> u32 {
        let id = self.widgets.iter().map(|widget| widget.wm_id).max().unwrap_or(0) + 1;
        self.widgets.push(Widget::from_config(&initialised(wm_type, id)).expect("a widget object"));
        self.actions.push(Action::Create(wm_type.to_owned()));
        if wm_type == stopwatch::WM_TYPE {
            self.timers.insert(id, stopwatch::Timer::default());
        }
        // The new widget shows itself (`wm.show(props.config.wmId)`).
        self.show(id);
        id
    }

    /// `wm.remove(wmId)`: `Widget.vue`'s delete button, held for a second upstream.
    pub fn remove(&mut self, wm_id: u32) {
        self.widgets.retain(|widget| widget.wm_id != wm_id);
        self.timers.remove(&wm_id);
        if self.hover == Some(wm_id) {
            self.hover = None;
        }
        if self.panel_hover == Some(wm_id) {
            self.panel_hover = None;
        }
        if self.editing == Some(wm_id) {
            self.editing = None;
        }
        self.actions.push(Action::Remove(wm_id));
    }

    /// `WidgetMenu.vue`'s `toggle`: show a hidden widget, hide a shown one.
    pub fn toggle(&mut self, wm_id: u32) {
        match self.widget(wm_id).map(|widget| widget.wm_wants) {
            Some(Wants::Hide) => self.show(wm_id),
            Some(Wants::Show) => self.hide(wm_id),
            None => {}
        }
    }

    /// `WidgetTimer.vue`'s play/pause button: `start` or `stop`.
    pub fn timer_toggle(&mut self, wm_id: u32, now_ms: f64) {
        match self.timers.get(&wm_id).map(|timer| timer.running) {
            Some(true) => self.timer_stop(wm_id, now_ms),
            Some(false) => self.timer_start(wm_id, now_ms),
            None => {}
        }
    }

    /// `start()`: run from `now`, and stay on screen when the overlay loses focus.
    pub fn timer_start(&mut self, wm_id: u32, now_ms: f64) {
        if let Some(timer) = self.timers.get_mut(&wm_id) {
            timer.start(now_ms);
            self.set_flag(wm_id, INVISIBLE_ON_BLUR, false);
        }
    }

    /// `stop()`: take the last tick, and hide with the overlay again when nothing was timed.
    pub fn timer_stop(&mut self, wm_id: u32, now_ms: f64) {
        if let Some(timer) = self.timers.get_mut(&wm_id) {
            timer.update(now_ms);
            timer.running = false;
            if timer.millis < 1000.0 {
                self.set_flag(wm_id, INVISIBLE_ON_BLUR, true);
            }
        }
    }

    /// `reset()`: zero it, or restart at a second while running, and hide it when stopped.
    pub fn timer_reset(&mut self, wm_id: u32, now_ms: f64) {
        if let Some(timer) = self.timers.get_mut(&wm_id) {
            timer.reset(now_ms);
            if !timer.running {
                self.set_flag(wm_id, INVISIBLE_ON_BLUR, true);
            }
        }
    }

    /// `MAIN->CLIENT::widget-action`, the hotkeys the main process sends: the stopwatch's start
    /// and pause (`stopwatch-start-stop:<wmId>`), its reset, and the delve grid's toggle.
    pub fn widget_action(&mut self, target: &str, now_ms: f64) {
        if let Some(id) = target.strip_prefix("stopwatch-start-stop:") {
            if let Ok(id) = id.parse() {
                self.timer_toggle(id, now_ms);
            }
            return;
        }
        if let Some(id) = target.strip_prefix("stopwatch-reset:") {
            if let Ok(id) = id.parse() {
                self.timer_reset(id, now_ms);
            }
            return;
        }
        if target == delve_grid::WM_TYPE
            && let Some(id) = self.widgets.iter().find(|widget| widget.wm_type == delve_grid::WM_TYPE).map(|w| w.wm_id)
        {
            self.toggle(id);
        }
    }

    /// `v-model="config.alwaysShow"` (`WidgetMenu.vue`'s edit mode).
    pub fn set_always_show(&mut self, wm_id: u32, on: bool) {
        if let Some(widget) = self.widget_mut(wm_id)
            && let Some(object) = widget.config.as_object_mut()
        {
            object.insert("alwaysShow".to_owned(), Value::Bool(on));
        }
    }

    /// `v-model="config.wmTitle"` (`WidgetImageStrip.vue`'s title input).
    pub fn set_title(&mut self, wm_id: u32, title: &str) {
        if let Some(widget) = self.widget_mut(wm_id) {
            widget.wm_title = title.to_owned();
            if let Some(object) = widget.config.as_object_mut() {
                object.insert("wmTitle".to_owned(), Value::String(title.to_owned()));
            }
        }
    }

    /// `config.images = config.images.filter(_ => _.id !== id)`.
    pub fn remove_image(&mut self, wm_id: u32, image_id: i64) {
        if let Some(widget) = self.widget_mut(wm_id)
            && let Some(images) = widget.config.get_mut("images").and_then(Value::as_array_mut)
        {
            images.retain(|image| image.get("id").and_then(Value::as_i64) != Some(image_id));
        }
    }

    /// The buttons' actions since the last call, emptied.
    pub fn take_actions(&mut self) -> Vec<Action> {
        std::mem::take(&mut self.actions)
    }

    /// `AppSpec::poll`: advance the running timers to `clock` (seconds). True when a displayed
    /// second changed, which is when the window needs to redraw.
    pub fn poll(&mut self, clock: f64) -> bool {
        let now = clock * 1000.0;
        let mut changed = false;
        for timer in self.timers.values_mut() {
            // `|=` and not `||`: every running timer has to advance, not just the first that did.
            changed |= timer.update(now);
        }
        changed
    }

    /// The widget's actions panel: upstream's `t('widget.hide'|…)` labels, which the panel shows
    /// while the pointer is over the widget.
    pub fn panel_actions(&self, harness: Chrome) -> Vec<(&'static str, &'static str)> {
        let mut actions = Vec::new();
        if harness.hideable {
            actions.push(("hide", "widget.hide"));
        }
        if harness.editable {
            actions.push(("edit", "widget.edit"));
        }
        if harness.has_handles() {
            actions.push(("move", "widget.move"));
        }
        if harness.removable {
            actions.push(("delete", "widget.delete"));
        }
        actions
    }
}

/// `invisible-on-blur`, the flag the stopwatch clears while it runs.
const INVISIBLE_ON_BLUR: &str = "invisible-on-blur";

/// `WidgetSpec.type` of the settings window, which the menu's cog shows and a widget's `edit`
/// opens when it does not edit inline.
const SETTINGS: &str = "settings";

/// A new widget's config: `wm.create`'s `uninitialized` object with the widget's own
/// `initInstance`/mount branch applied, which upstream runs on the component's first mount.
///
/// `WidgetTimer.vue` clears the `uninitialized` flag, takes the centre anchor and leaves the
/// hotkeys unset (the reference picks a random 40–60 % anchor; a port's scene and tests want a
/// fixed one); `WidgetImageStrip.vue` does the same with its top-centre anchor and the default
/// cheat sheet.
fn initialised(wm_type: &str, wm_id: u32) -> Value {
    let mut config = instance(wm_type, wm_id);
    let Some(object) = config.as_object_mut() else { return config };
    match wm_type {
        stopwatch::WM_TYPE => {
            object.insert("wmFlags".to_owned(), json!(["invisible-on-blur"]));
            object.insert("anchor".to_owned(), json!({ "pos": "cc", "x": 50.0, "y": 50.0 }));
            object.insert("toggleKey".to_owned(), Value::Null);
            object.insert("resetKey".to_owned(), Value::Null);
        }
        image_strip::WM_TYPE => {
            object.insert("wmFlags".to_owned(), json!(["invisible-on-blur"]));
            object.insert("anchor".to_owned(), json!({ "pos": "tc", "x": 50.0, "y": 10.0 }));
            object.insert("images".to_owned(), json!([{ "id": 1, "url": "syndicate.jpg" }]));
        }
        _ => {}
    }
    config
}

/// A widget object as `wm.create` (and `WidgetSpec.initInstance`) writes it.
fn instance(wm_type: &str, wm_id: u32) -> Value {
    json!({
        "wmId": wm_id,
        "wmType": wm_type,
        "wmTitle": "",
        "wmWants": "hide",
        "wmZorder": null,
        "wmFlags": ["uninitialized"],
    })
}

/// What `Widget.vue`'s props let a widget's card do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Chrome {
    /// `hideable`: the panel's `hide` button (not for the menu or the timer).
    pub hideable: bool,
    /// `readonly`: no `edit` button (the delve grid).
    pub editable: bool,
    /// `inline-edit`: the panel's `edit` toggles the card's editor; otherwise it opens settings.
    pub inline_edit: bool,
    /// `removable`: the panel's `delete` button.
    pub removable: bool,
    /// `move-handles`: `Some("center"|"corners"|"top-bottom"|"all")`, or `None` for no `move`.
    pub handles: Option<&'static str>,
}

impl Chrome {
    fn has_handles(self) -> bool {
        self.handles.is_some()
    }
}

/// The widget's `wmId` as a `Chrome` for the scene's one widget.
pub fn chrome_for(widget: &Widget) -> Chrome {
    match widget.wm_type.as_str() {
        menu::WM_TYPE => {
            Chrome { hideable: false, editable: true, inline_edit: true, removable: false, handles: Some("corners") }
        }
        stopwatch::WM_TYPE => {
            Chrome { hideable: false, editable: true, inline_edit: false, removable: true, handles: Some("center") }
        }
        image_strip::WM_TYPE => {
            Chrome { hideable: true, editable: true, inline_edit: true, removable: true, handles: Some("top-bottom") }
        }
        delve_grid::WM_TYPE => {
            Chrome { hideable: true, editable: false, inline_edit: true, removable: false, handles: None }
        }
        // `WidgetItemSearch.vue`: `move-handles="['tl', 'bl']"`, not removable, no inline edit.
        item_search::WM_TYPE => {
            Chrome { hideable: true, editable: true, inline_edit: false, removable: false, handles: Some("tl-bl") }
        }
        // `WidgetStashSearch.vue`: `move-handles="corners"`, no inline edit.
        stash_search::WM_TYPE => {
            Chrome { hideable: true, editable: true, inline_edit: false, removable: true, handles: Some("corners") }
        }
        _ => Chrome { hideable: true, editable: true, inline_edit: true, removable: true, handles: Some("all") },
    }
}

/// `Widget.vue`: the card, its hover actions panel and its move handles.
pub fn chrome<S: 'static>(overlay: &Overlay, widget: &Widget, card: El, project: Project<S>) -> El {
    let id = widget.wm_id;
    let harness = chrome_for(widget);
    let (x, y) = widget.anchor();
    let hovered = overlay.hover == Some(id) || overlay.panel_hover == Some(id);

    let mut frame = row().key(format!("widget-{id}")).on_hover_change(move |s: &mut S, inside: bool, _tx| {
        let overlay = project(s);
        overlay.hover = inside.then_some(id);
    });
    frame = frame.child(card);

    if hovered {
        // `.actionsPanel` is `position: absolute` in `Widget.vue`, hanging off the card's right
        // (`left: 100%`) or left (`right: 100%`) edge, top- or bottom-aligned by the anchor.
        // A zero-width marker after the card is that edge: the panel is positioned inside it.
        let mut panel = col()
            .key(format!("widget-{id}-panel"))
            .py(theme::rem(0.25))
            .radius(theme::ROUNDED)
            .fill(Color::BLACK.alpha(0.4))
            .on_hover_change(move |s: &mut S, inside: bool, _tx| {
                let overlay = project(s);
                overlay.panel_hover = inside.then_some(id);
            });
        for (name, key) in overlay.panel_actions(harness) {
            let label = crate::settings::t(overlay.game, key);
            let action = name;
            panel = panel.child(
                row()
                    .key(format!("widget-{id}-{name}"))
                    .px(theme::rem(0.25))
                    .focusable(true)
                    .role("button")
                    .label(label.clone())
                    .hover(Color::WHITE.alpha(0.15))
                    .on_press(move |s: &mut S| panel_press(project(s), id, action))
                    .child(
                        text(label)
                            .key(format!("widget-{id}-{name}-label"))
                            .font(theme::leading_none(theme::text::SM))
                            .color(theme::gray(100)),
                    ),
            );
        }
        // `left: 100%` hangs off the card's right edge, `right: 100%` off its left; `top: 0` /
        // `bottom: 0` pick the end of the card it starts at.
        let on_right = x <= 50.0;
        let on_top = y <= 50.0;
        panel = match (on_right, on_top) {
            (true, true) => panel.absolute(0.0, 0.0),
            (false, true) => panel.pin(None, Some(0.0), Some(0.0), None),
            (true, false) => panel.pin(Some(0.0), None, None, Some(0.0)),
            (false, false) => panel.pin(None, None, Some(0.0), Some(0.0)),
        };
        frame = frame.child(row().key(format!("widget-{id}-panel-anchor")).child(panel));
    }

    if overlay.moving == Some(id) {
        frame = frame.child(move_handles(id, harness.handles, project));
    }
    frame
}

/// One panel button's press (`hide`, `edit`, `move`, `delete`), as `Widget.vue` runs it.
fn panel_press(overlay: &mut Overlay, id: u32, action: &'static str) {
    match action {
        "hide" => overlay.hide(id),
        "move" => {
            overlay.moving = (overlay.moving != Some(id)).then_some(id);
            overlay.editing = None;
        }
        "delete" => overlay.remove(id),
        "edit" => {
            let Some(widget) = overlay.widget(id) else { return };
            let inline = chrome_for(widget).inline_edit;
            overlay.moving = None;
            if inline {
                overlay.editing = (overlay.editing != Some(id)).then_some(id);
                if overlay.editing == Some(id) {
                    let title = overlay.widget(id).map(|widget| widget.wm_title.clone()).unwrap_or_default();
                    overlay.title = TextEdit::new(title);
                }
            } else {
                // `toggleEdit`: `wm.setFlag(settings, 'settings::widget=<wmId>', true)` then
                // `wm.show(settings)`, so the settings window opens on that widget's page.
                if let Some(settings) =
                    overlay.widgets.iter().find(|widget| widget.wm_type == SETTINGS).map(|w| w.wm_id)
                {
                    overlay.set_flag(settings, &format!("settings::widget={id}"), true);
                    overlay.show(settings);
                }
                overlay.actions.push(Action::EditInSettings(id));
            }
        }
        _ => {}
    }
}

/// The move handles `Widget.vue` draws while `isMoving` (`shownHandles`), around the card.
fn move_handles<S: 'static>(id: u32, handles: Option<&'static str>, _project: Project<S>) -> El {
    let size = theme::rem(1.0);
    let handle = |key: String| {
        el().key(key).size(size, size).border(theme::rem(0.25), Color::BLACK.alpha(0.6)).fill(Color::BLACK.alpha(0.2))
    };
    let mut layer = el().key(format!("widget-{id}-handles")).pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center();
    let around = handles.unwrap_or_default();
    if matches!(around, "all" | "corners" | "top-bottom") {
        layer = layer
            .child(handle(format!("widget-{id}-handle-tl")).pin(Some(-size / 2.0), Some(-size / 2.0), None, None))
            .child(handle(format!("widget-{id}-handle-tr")).pin(None, Some(-size / 2.0), Some(-size / 2.0), None));
    }
    if matches!(around, "all" | "corners" | "top-bottom") {
        layer = layer
            .child(handle(format!("widget-{id}-handle-bl")).pin(Some(-size / 2.0), None, None, Some(-size / 2.0)))
            .child(handle(format!("widget-{id}-handle-br")).pin(None, None, Some(-size / 2.0), Some(-size / 2.0)));
    }
    if around == "all" {
        layer = layer
            .child(handle(format!("widget-{id}-handle-tc")).pin(None, Some(-size / 2.0), None, None).center())
            .child(handle(format!("widget-{id}-handle-bc")).pin(None, None, None, Some(-size / 2.0)));
    }
    // `move-handles: ['tl', 'bl']` (the item search) is the two handles on the left edge.
    if around == "tl-bl" {
        layer = layer
            .child(handle(format!("widget-{id}-handle-tl")).pin(Some(-size / 2.0), Some(-size / 2.0), None, None))
            .child(handle(format!("widget-{id}-handle-bl")).pin(Some(-size / 2.0), None, None, Some(-size / 2.0)));
    }
    // `move-handles: 'center'` is the one handle in the middle (`stopwatch/WidgetTimer.vue`).
    if around == "center" {
        layer = layer.child(handle(format!("widget-{id}-handle-cc")));
    }
    layer
}

/// The widget's card: the slot `Widget.vue` wraps.
pub fn view<S: 'static>(overlay: &Overlay, cx: &mut Cx<S>, project: Project<S>) -> El {
    cx.foreground(theme::gray(100));
    let Some(widget) = overlay.shown_widget().cloned() else { return el().key("widgets") };
    let card = match widget.wm_type.as_str() {
        stopwatch::WM_TYPE => stopwatch::view(overlay, &widget, project),
        image_strip::WM_TYPE => image_strip::view(overlay, &widget, project),
        delve_grid::WM_TYPE => delve_grid::view(overlay, &widget, project),
        item_search::WM_TYPE => item_search::view(overlay, &widget, project),
        stash_search::WM_TYPE => stash_search::view(overlay, &widget, project),
        _ => menu::view(overlay, &widget, project),
    };
    // The window is the card: upstream's `.widget` is content-sized and the screenshot harness
    // crops it, so the scene's root places it at the top left and does not stretch it.
    col()
        .key("widgets")
        .w_full()
        .h_full()
        .items_start()
        .fill(theme::overlay_background())
        .child(chrome(overlay, &widget, card, project))
}

/// A `widget-<name>` / `poe2-widget-<name>` scene: the game and the widget.
pub fn scene(slug: &str) -> Option<(GameId, Kind)> {
    let (game, rest) = match slug.strip_prefix("poe2-") {
        Some(rest) => (GameId::Poe2, rest),
        None => (GameId::Poe1, slug),
    };
    let name = rest.strip_prefix("widget-")?;
    Some((game, Kind::from_name(name)?))
}

/// The size a widget scene renders at: the card's own, which is what
/// `tools/golden/screens/widgets.mjs` screenshots (`#widget-<wmId> > div`).
///
/// The menu's width is its buttons' and textarea's content width, which Rune measures for
/// itself; these are the reference's own sizes, measured from `target/screens/<game>/menu.png`
/// (`470×78` for PoE1's four buttons and `509×78` for EE2's five, a wider `XP` bar). A ported
/// card a few points narrower leaves the window's right edge empty rather than hiding the
/// difference.
pub fn scene_size(game: GameId, kind: Kind) -> (f32, f32) {
    match kind {
        Kind::Menu => match game {
            GameId::Poe2 => (509.0, 78.0),
            _ => (470.0, 78.0),
        },
        Kind::Stopwatch => (170.0, 40.0),
        Kind::ImageStrip => (200.0, 136.0),
        Kind::DelveGrid => {
            let metrics = delve_grid::Metrics::for_overlay(crate::HEIGHT);
            (metrics.width, metrics.height)
        }
        // The cards' own sizes, measured from `target/screens/<game>/item-search.png` and
        // `stash-search.png`: the item search is `min-width: 24rem` plus its padding and the
        // switch's room, the stash search the width of its entries.
        Kind::ItemSearch => (384.0, 125.0),
        Kind::StashSearch => match game {
            GameId::Poe2 => (160.0, 176.0),
            _ => (166.0, 176.0),
        },
    }
}

/// The picture the image strip's card draws, from the reference's `/images` (the widget's
/// `images[].url` is a file name, a URL, or an uploaded file's name, which the port has no store
/// for).
pub fn strip_picture(game: GameId, url: &str, size: (f32, f32)) -> El {
    if url.contains("://") { art::cdn(game, url, size) } else { art::local_cover(game, url, size) }
}
