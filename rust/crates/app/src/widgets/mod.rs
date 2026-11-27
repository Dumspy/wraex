//! `ref/apt/renderer/src/web/overlay/` and the widgets it hosts, in Rune.
//!
//! The overlay is one page full of *widgets*: `AppConfig().widgets` lists them, each with a
//! `wmType`, a `wmTitle`, a `wmWants` (`show`/`hide`) and an `anchor`. `OverlayWindow.vue` draws
//! every visible one at its anchor; `Widget.vue` is the frame around each (the hover actions
//! panel, the edit/move modes, the move handles). Seven of them are ported here; the item and
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
//! `widget-delve-grid`, `widget-item-search`, `widget-stash-search`, `poe2-widget-xp-tracker`,
//! and `poe2-widget-…` for EE2's copies), one widget per window: the
//! screenshots in `tools/golden/screens/widgets.mjs` crop exactly one widget's card, so a
//! scene's window *is* that card (see [`scene_size`]), filled with what the reference's overlay
//! paints behind its widgets ([`crate::theme::overlay_background`]) so a translucent widget
//! composites the same way. A live overlay window draws every widget whose `wmWants` is `show`
//! at once, each at its own `anchor`: [`card`] is one widget's box, which the shell places
//! ([`crate::shell::App::cards_for`]); [`view`] is the scene's single card.
//!
//! `Widget.vue` draws any widget at its config's `anchor` (`top: y%`, `left: x%`, shifted by
//! the `translate` its `pos` picks); the shell reads that for every card ([`Widget::anchor`],
//! [`Widget::anchor_pos`]). `wmZorder` is the paint order: a card with a number paints above
//! [`Widget::zorder`]'s `auto` ones (the reference's CSS `z-index`), and pressing a card brings
//! it to the top (`bring_to_top`).
//!
//! One consequence of a card-sized window: the hover actions panel hangs *outside* the card (as
//! `Widget.vue`'s `position: absolute` panel does), so in a scene it is clipped. It is still in
//! the element tree, and a test that presses its buttons renders in a window with room for it
//! (`tests::runtime_panel`); the app's own window is the whole overlay, where it shows as
//! upstream's does.
//!
//! Widgets act through the app shell, as upstream's do: showing, hiding or creating a widget
//! edits the config the shell owns (`wm.show` in `OverlayWindow.vue`) and records an [`Action`]
//! for the shell, so a scene or a test reads what happened without a window. The image strip's
//! file picker and the drag-and-drop reordering of its images stay with the shell and are not
//! ported (they record an [`Action`] each).
//!
//! Moving a widget is `Widget.vue`'s move mode: the `move` button draws the handles its
//! `move-handles` prop picks ([`move_handles`]), a press on a handle puts that corner under the
//! pointer and every move writes the pointer's place into the widget's `anchor` percentages
//! ([`Overlay::begin_move`], [`Overlay::drag_to`]), and a press anywhere on a card raises it
//! ([`Overlay::bring_to_top`]). Both write the widget's config; the overlay marks it
//! ([`Overlay::take_config_dirty`]) and the shell saves the file, as upstream's `saveConfig()`
//! does on the overlay's blur.
//!
//! The widgets' own hotkeys (`toggleKey`, `resetKey`) are the platform's to bind; what the main
//! process sends while they are bound, `MAIN->CLIENT::widget-action`, is
//! [`Overlay::widget_action`].
//!
//! Still missing from the reference's widget frame: the fade-in of its panel (`Widget.vue`'s
//! `:hover` with no transition upstream either), the `delete` countdown's 1 s progress gradient
//! (the port deletes on release), the DnD library's reorder animation, and the floating
//! `mover.active` handle: upstream hides the in-card handle for the current `anchor.pos` and
//! draws that one fixed on the overlay instead, while the port draws every declared handle
//! inside the card.

pub mod delve_grid;
pub mod icon;
pub mod image_strip;
pub mod item_search;
pub mod menu;
pub mod stash_search;
pub mod stopwatch;
pub mod xp_tracker;

use std::collections::BTreeMap;

use rune::{Color, Cx, Drag, El, Phase, TextEdit, col, el, row, text};
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
    /// EE2's experience tracker, the first client-log-driven widget.
    XpTracker,
}

impl Kind {
    /// Every ported widget.
    pub const ALL: [Kind; 7] = [
        Kind::Menu,
        Kind::Stopwatch,
        Kind::ImageStrip,
        Kind::DelveGrid,
        Kind::ItemSearch,
        Kind::StashSearch,
        Kind::XpTracker,
    ];

    /// The `--scene` suffix (`widget-<name>`) and the enum's name.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Menu => "menu",
            Kind::Stopwatch => "stopwatch",
            Kind::ImageStrip => "image-strip",
            Kind::DelveGrid => "delve-grid",
            Kind::ItemSearch => "item-search",
            Kind::StashSearch => "stash-search",
            Kind::XpTracker => "xp-tracker",
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
            Kind::XpTracker => xp_tracker::WM_TYPE,
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    /// The kind a config's `wmType` names. It differs from [`Kind::name`] for the stopwatch
    /// (the config says `timer`, the scene `stopwatch`).
    pub fn from_wm_type(wm_type: &str) -> Option<Self> {
        Kind::ALL.into_iter().find(|kind| kind.wm_type() == wm_type)
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

    /// `wmZorder: 'exclusive'`: `OverlayWindow.vue` draws this widget alone while it is shown
    /// (settings, the price check and the item check in the reference's registry).
    pub fn is_exclusive(&self) -> bool {
        self.config.get("wmZorder") == Some(&Value::String("exclusive".to_owned()))
    }

    /// `wmZorder` as the number `Widget.vue` sets as the card's `z-index`, if it has one. The
    /// overlay paints the cards in this order; `None` is CSS' `auto` (the config's order).
    pub fn zorder(&self) -> Option<f64> {
        self.config.get("wmZorder").and_then(Value::as_f64)
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

    /// `anchor.pos` (`tl`, `tc`, `cc`, ...): which corner of the card `anchor.x`/`anchor.y`
    /// name. `Widget.vue` picks the `translate` with it; a widget without an anchor (the
    /// settings window, which is not drawn as a card) is top-left.
    pub fn anchor_pos(&self) -> String {
        self.config
            .get("anchor")
            .and_then(|anchor| anchor.get("pos"))
            .and_then(Value::as_str)
            .unwrap_or("tl")
            .to_owned()
    }

    /// `props.config.anchor.pos = pos` and `updatePosition`'s x/y write, as one edit. Returns
    /// whether the widget changed.
    pub fn set_anchor(&mut self, pos: &str, x: f32, y: f32) -> bool {
        if self.anchor_pos() == pos && self.anchor() == (x, y) {
            return false;
        }
        let anchor = json!({ "pos": pos, "x": x, "y": y });
        match self.config.as_object_mut() {
            Some(object) => {
                object.insert("anchor".to_owned(), anchor);
            }
            None => return false,
        }
        true
    }

    /// `wmZorder` as `OverlayWindow.vue`'s `bringToTop` writes it. Returns whether the widget
    /// changed.
    pub fn set_zorder(&mut self, zorder: f64) -> bool {
        if self.zorder() == Some(zorder) {
            return false;
        }
        let Some(object) = self.config.as_object_mut() else { return false };
        object.insert("wmZorder".to_owned(), json!(zorder));
        true
    }
}

/// `Widget.vue`'s `startMove`: the anchor a move-handle press began from. A cancelled gesture
/// puts the widget back there; a released one keeps the new anchor and asks the shell to save
/// it.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveStart {
    pub wm_id: u32,
    pub pos: String,
    pub x: f32,
    pub y: f32,
}

/// `Widget.vue`'s `updatePosition`: a pointer's place in the overlay as `anchor.x`/`anchor.y`
/// percentages, clamped to the overlay's edges
/// (`Math.min(Math.max(p / size, 0), 1) * 100`). A zero-sized overlay (a scene before its size
/// is known) puts the anchor at the top-left, as upstream's `0 / 0` would be `NaN` there.
pub fn pointer_percent(point: [f32; 2], size: (f32, f32)) -> (f32, f32) {
    let axis = |value: f32, limit: f32| {
        if limit.is_finite() && limit > 0.0 { (value / limit).clamp(0.0, 1.0) * 100.0 } else { 0.0 }
    };
    (axis(point[0], size.0), axis(point[1], size.1))
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
    /// The `wmId` of the widget the window was built around: a scene picks one, and a live app
    /// focuses the one a hotkey or the menu asks for. A live overlay over the game draws every
    /// widget whose `wmWants` is `show`, each at its anchor, not only this one.
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
    /// The overlay window's size in points: the whole the `anchor` percentages are of. The
    /// shell sets it from the game's client area every frame ([`crate::shell::Live`]); a scene's
    /// window is the card itself.
    pub size: (f32, f32),
    /// `Widget.vue`'s `startMove`: the anchor a handle press began from, kept so a cancelled
    /// gesture (Rune takes the pointer away when the window loses the focus) can put the widget
    /// back where it was.
    pub move_start: Option<MoveStart>,
    /// A widget's `anchor` or `wmZorder` changed: upstream saves the config for those
    /// (`saveConfig()` on the overlay's blur). [`Overlay::take_config_dirty`] hands it to the
    /// shell, which writes the file.
    config_dirty: bool,
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
    /// The client-log snapshot the experience tracker draws (`crate::client_log`), and its
    /// level input's text. A live shell copies each snapshot in ([`Overlay::set_xp`]); a scene
    /// starts on a fixed one.
    pub xp: crate::client_log::State,
    pub xp_input: TextEdit,
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
        let mut overlay = Self::new(game, widgets);
        if kind == Kind::XpTracker {
            // The scene's fixed client-log state: a level 72 character in a level 80 area.
            overlay.set_xp(xp_tracker::scene_state());
        }
        // A scene's window is the card: its own size is the overlay the anchor percentages are
        // of, so a move handle pressed in a scene lands where the reference's would.
        overlay.size = scene_size(game, kind);
        if let Some(id) = shown {
            // `wm.show` when the scene opens, without an [`Action`]: the scene is the widget
            // being on screen, not a button press.
            overlay.activate(id, crate::trade::Mode::Replay);
        }
        overlay
    }

    /// The overlay a live window draws: the config's widgets, none drawn until the overlay is
    /// active (then the menu, `OverlayWindow.vue`'s `visibilityState`) or a hotkey or a
    /// widget's own button asks for one (`wm.show`).
    pub fn live(game: GameId, config: &Value) -> Self {
        let widgets: Vec<Widget> = config
            .get("widgets")
            .and_then(Value::as_array)
            .map(|widgets| widgets.iter().filter_map(Widget::from_config).collect())
            .unwrap_or_default();
        Self::new(game, widgets)
    }

    fn new(game: GameId, widgets: Vec<Widget>) -> Self {
        Self {
            game,
            widgets,
            shown: 0,
            hover: None,
            panel_hover: None,
            editing: None,
            moving: None,
            size: (0.0, 0.0),
            move_start: None,
            config_dirty: false,
            add_open: false,
            paste: TextEdit::default(),
            title: TextEdit::default(),
            timers: BTreeMap::new(),
            item_search: None,
            xp: crate::client_log::State::default(),
            xp_input: TextEdit::new("1"),
            actions: Vec::new(),
        }
    }

    /// Draw `wm_id`'s widget, with the running state its type needs (`WidgetTimer.vue`'s
    /// clock, `WidgetItemSearch.vue`'s prices). Marks it `show`, as `wm.show` does.
    pub fn activate(&mut self, wm_id: u32, mode: crate::trade::Mode) {
        if let Some(widget) = self.widget_mut(wm_id) {
            widget.set_wants(Wants::Show);
        }
        self.focus(wm_id, mode);
    }

    /// Draw `wm_id`'s widget without changing its `wmWants`: the window follows the widget the
    /// config says is shown, and a widget's own state is built the first time it is drawn.
    pub fn focus(&mut self, wm_id: u32, mode: crate::trade::Mode) {
        self.shown = wm_id;
        self.ensure_state(wm_id, mode);
        let title = self.widget(wm_id).map(|widget| widget.wm_title.clone()).unwrap_or_default();
        self.title = TextEdit::new(title);
    }

    /// Give a widget the running state its card reads before it is drawn: the stopwatch's clock
    /// (`WidgetTimer.vue`'s `instances: 'multi'` state) and the item search's prices. The overlay
    /// draws every shown widget, so this runs for each of them, not only the focused one. True
    /// when it built one.
    pub fn ensure_state(&mut self, wm_id: u32, mode: crate::trade::Mode) -> bool {
        let wm_type = self.widget(wm_id).map(|widget| widget.wm_type.clone());
        let mut added = false;
        if wm_type.as_deref() == Some(stopwatch::WM_TYPE)
            && let std::collections::btree_map::Entry::Vacant(entry) = self.timers.entry(wm_id)
        {
            entry.insert(stopwatch::Timer::default());
            added = true;
        }
        if wm_type.as_deref() == Some(item_search::WM_TYPE) && self.item_search.is_none() {
            // A scene reads the prices the screenshots recorded; a live window fetches them.
            self.item_search = Some(item_search::State::new(self.game, mode));
            added = true;
        }
        added
    }

    /// [`Overlay::ensure_state`] for every widget whose `wmWants` is `show`: the cards the
    /// overlay draws, whose view reads their state. True when one was built.
    pub fn ensure_states(&mut self, mode: crate::trade::Mode) -> bool {
        let ids: Vec<u32> =
            self.widgets.iter().filter(|widget| widget.wm_wants == Wants::Show).map(|widget| widget.wm_id).collect();
        let mut added = false;
        for id in ids {
            added |= self.ensure_state(id, mode);
        }
        added
    }

    /// `OverlayWindow.vue`'s `bringToTop`: the pressed widget paints above the others
    /// (`Widget.vue`'s `mousedown`). An exclusive widget already wins the whole overlay, so it
    /// keeps its `wmZorder`; the topmost numbered widget is left alone, as upstream returns when
    /// the press is on it. A change asks the shell to save the config.
    pub fn bring_to_top(&mut self, wm_id: u32) {
        if self.widget(wm_id).is_some_and(Widget::is_exclusive) {
            return;
        }
        let Some(top) = self
            .widgets
            .iter()
            .filter_map(Widget::zorder)
            .fold(None, |top: Option<f64>, z| Some(top.map_or(z, |top| top.max(z))))
        else {
            return;
        };
        if self.widget(wm_id).and_then(Widget::zorder) == Some(top) {
            return;
        }
        if self.widget_mut(wm_id).is_some_and(|widget| widget.set_zorder(top + 1.0)) {
            self.config_dirty = true;
        }
    }

    /// `Widget.vue`'s `startMove(pos, e)`: the handle under the pointer becomes the widget's
    /// `anchor.pos`, the widget comes to the top and follows the pointer at once. The anchor it
    /// started from is kept for a gesture Rune cancels.
    pub fn begin_move(&mut self, wm_id: u32, pos: &str, point: [f32; 2]) {
        let Some(widget) = self.widget(wm_id) else { return };
        self.move_start =
            Some(MoveStart { wm_id, pos: widget.anchor_pos(), x: widget.anchor().0, y: widget.anchor().1 });
        self.moving = Some(wm_id);
        self.bring_to_top(wm_id);
        self.drag_to(wm_id, pos, point);
    }

    /// `Widget.vue`'s `updatePosition`: the widget's anchor follows the pointer every frame, as
    /// percentages of the overlay. Returns whether the widget changed. An overlay whose size is
    /// not known yet (a live app between two polls) keeps the anchor where it is.
    pub fn drag_to(&mut self, wm_id: u32, pos: &str, point: [f32; 2]) -> bool {
        if self.size.0 <= 0.0 || self.size.1 <= 0.0 {
            return false;
        }
        let (x, y) = pointer_percent(point, self.size);
        self.widget_mut(wm_id).is_some_and(|widget| widget.set_anchor(pos, x, y))
    }

    /// The end of a move-handle gesture. A released one keeps the pointer's place and asks the
    /// shell to save the config (`saveConfig()`); a cancelled one — Rune takes the pointer away
    /// when the window loses the focus — puts the widget back where the press found it.
    pub fn end_move(&mut self, wm_id: u32, pos: &str, point: [f32; 2], cancelled: bool) {
        let start = self.move_start.take().filter(|start| start.wm_id == wm_id);
        if cancelled {
            if let Some(start) = start
                && let Some(widget) = self.widget_mut(wm_id)
            {
                widget.set_anchor(&start.pos, start.x, start.y);
            }
            return;
        }
        self.drag_to(wm_id, pos, point);
        // The moves already wrote the anchor; the gesture as a whole is what has to be saved.
        let changed = start.is_some_and(|start| {
            self.widget(wm_id)
                .is_some_and(|widget| widget.anchor_pos() != start.pos || widget.anchor() != (start.x, start.y))
        });
        if changed {
            self.config_dirty = true;
        }
    }

    /// Whether a widget's `anchor` or `wmZorder` changed since the shell last saved: upstream's
    /// `saveConfig()` has work to do. Emptied by [`Overlay::take_config_dirty`].
    pub fn config_dirty(&self) -> bool {
        self.config_dirty
    }

    pub fn take_config_dirty(&mut self) -> bool {
        std::mem::take(&mut self.config_dirty)
    }

    /// [`Overlay::config_dirty`] after a save that failed, so the next poll tries again.
    pub fn mark_config_dirty(&mut self) {
        self.config_dirty = true;
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

    /// The client-log snapshot the experience tracker draws, as the live shell published it.
    /// The input follows the character level a level-up set, as the Vue computed does; a scene
    /// that wants a typed level sets it after this.
    pub fn set_xp(&mut self, state: crate::client_log::State) {
        self.xp_input = TextEdit::new(wraex_core::js::number_to_string(state.player_level));
        self.xp = state;
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
        self.config_dirty = true;
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
        let count = self.widgets.len();
        self.widgets.retain(|widget| widget.wm_id != wm_id);
        self.config_dirty |= count != self.widgets.len();
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
            self.config_dirty |= object.insert("alwaysShow".to_owned(), Value::Bool(on)) != Some(Value::Bool(on));
        }
    }

    /// `v-model="config.wmTitle"` (`WidgetImageStrip.vue`'s title input).
    pub fn set_title(&mut self, wm_id: u32, title: &str) {
        if let Some(widget) = self.widget_mut(wm_id) {
            widget.wm_title = title.to_owned();
            if let Some(object) = widget.config.as_object_mut() {
                let previous = object.insert("wmTitle".to_owned(), Value::String(title.to_owned()));
                self.config_dirty |= previous.as_ref().and_then(Value::as_str) != Some(title);
            }
        }
    }

    /// `config.images = config.images.filter(_ => _.id !== id)`.
    pub fn remove_image(&mut self, wm_id: u32, image_id: i64) {
        if let Some(widget) = self.widget_mut(wm_id)
            && let Some(images) = widget.config.get_mut("images").and_then(Value::as_array_mut)
        {
            let count = images.len();
            images.retain(|image| image.get("id").and_then(Value::as_i64) != Some(image_id));
            self.config_dirty |= count != images.len();
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

/// `invisible-on-blur` (`widgets.ts`): `OverlayWindow.vue` hides it while the overlay is not
/// active (the widget menu has it, so the menu goes when the game takes the focus). The
/// stopwatch clears it while it runs, which is what keeps a running timer on screen.
pub const INVISIBLE_ON_BLUR: &str = "invisible-on-blur";

/// `hide-on-blur` (`widgets.ts`): `OverlayWindow.vue` hides it when the overlay loses the focus —
/// a widget that stays `show` until the user closes it, like a pinned stash search.
pub const HIDE_ON_BLUR: &str = "hide-on-blur";

/// `hide-on-focus` (`widgets.ts`): `OverlayWindow.vue` hides it when the overlay gains the focus,
/// so the delve grid never covers the interactive overlay.
pub const HIDE_ON_FOCUS: &str = "hide-on-focus";

/// `WidgetSpec.type` of the settings window, which the menu's cog shows and a widget's `edit`
/// opens when it does not edit inline. `wmZorder: 'exclusive'` upstream: it wins over the other
/// widgets, and `invisible-on-blur` closes it when the game takes the focus.
pub const SETTINGS: &str = "settings";

/// `WidgetSpec.type` of the price check. `wmZorder: 'exclusive'` upstream, `hide-on-blur`: the
/// window is drawn instead of every other widget and closes when the overlay loses the focus.
pub const PRICE_CHECK: &str = "price-check";

/// `WidgetSpec.type` of the item check, exclusive and `hide-on-blur` like the price check.
pub const ITEM_CHECK: &str = "item-check";

/// Whether the app draws this widget's card itself (`settings`, the price check and the item
/// check are exclusive widgets in the config, but their state lives in the app, not in
/// [`Overlay`]). The rest of the config is the overlay's own widgets.
pub fn app_drawn(wm_type: &str) -> bool {
    matches!(wm_type, SETTINGS | PRICE_CHECK | ITEM_CHECK)
}

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
        // `WidgetXpTracker.vue`'s `initInstance`: bottom centre, `showExp` on. It is a
        // `single` widget, so this only runs for a config that lost its instance.
        xp_tracker::WM_TYPE => {
            object.insert("wmTitle".to_owned(), json!(xp_tracker::TITLE));
            object.insert("anchor".to_owned(), json!({ "pos": "bc", "x": 68.0, "y": 98.0 }));
            object.insert("showExp".to_owned(), json!(true));
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
        // `WidgetXpTracker.vue`: `removable="false"`, `inline-edit="false"`,
        // `move-handles="top-bottom"`.
        xp_tracker::WM_TYPE => {
            Chrome { hideable: true, editable: true, inline_edit: false, removable: false, handles: Some("top-bottom") }
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
    // `Widget.vue`'s `:class="{ 'opacity-75': isMoving }"`: the widget dims while its handles
    // are out, so the game underneath shows through it.
    if overlay.moving == Some(id) {
        frame = frame.opacity(0.75);
    }
    // `Widget.vue`'s `@mousedown="handleMouseDown"`: `wm.bringToTop`. A press anywhere on the
    // card is the widget's, so it never reaches the overlay's background click layer; the
    // handler runs as the button goes down, as Vue's `mousedown` does.
    frame = frame.on_press(move |s: &mut S| project(s).bring_to_top(id)).press_on_down();
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
            // `Widget.vue`'s `:class="{ [$style.active]: isEditing }"` / `isMoving`: the entry
            // whose mode is on has a darker background.
            let active =
                (name == "move" && overlay.moving == Some(id)) || (name == "edit" && overlay.editing == Some(id));
            let mut entry = row()
                .key(format!("widget-{id}-{name}"))
                .px(theme::rem(0.25))
                .focusable(true)
                .role("button")
                .label(label.clone());
            if active {
                entry = entry.fill(Color::BLACK.alpha(0.6));
            }
            panel = panel.child(
                entry
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

/// The move handles `Widget.vue` draws while `isMoving` (`shownHandles`), around the card, and
/// the drag each one runs: `startMove(pos, e)` + `updatePosition` while the pointer moves, then
/// `endMove`.
fn move_handles<S: 'static>(id: u32, handles: Option<&'static str>, project: Project<S>) -> El {
    let size = theme::rem(1.0);
    let handle = |pos: &'static str| {
        el().key(format!("widget-{id}-handle-{pos}"))
            .size(size, size)
            .border(theme::rem(0.25), Color::BLACK.alpha(0.6))
            .fill(Color::BLACK.alpha(0.2))
            .on_drag(move |s: &mut S, drag: &mut Drag, _tx| {
                let overlay = project(s);
                match drag.phase {
                    Phase::Down => overlay.begin_move(id, pos, drag.window_point()),
                    Phase::Move => {
                        overlay.drag_to(id, pos, drag.window_point());
                    }
                    Phase::Up => overlay.end_move(id, pos, drag.window_point(), drag.cancelled()),
                }
            })
    };
    let mut layer = el().key(format!("widget-{id}-handles")).pin(Some(0.0), Some(0.0), Some(0.0), Some(0.0)).center();
    let around = handles.unwrap_or_default();
    if matches!(around, "all" | "corners" | "top-bottom") {
        layer = layer
            .child(handle("tl").pin(Some(-size / 2.0), Some(-size / 2.0), None, None))
            .child(handle("tr").pin(None, Some(-size / 2.0), Some(-size / 2.0), None));
    }
    if matches!(around, "all" | "corners" | "top-bottom") {
        layer = layer
            .child(handle("bl").pin(Some(-size / 2.0), None, None, Some(-size / 2.0)))
            .child(handle("br").pin(None, None, Some(-size / 2.0), Some(-size / 2.0)));
    }
    if around == "all" {
        layer = layer.child(handle("tc").pin(None, Some(-size / 2.0), None, None).center()).child(handle("bc").pin(
            None,
            None,
            None,
            Some(-size / 2.0),
        ));
    }
    // `move-handles: ['tl', 'bl']` (the item search) is the two handles on the left edge.
    if around == "tl-bl" {
        layer = layer
            .child(handle("tl").pin(Some(-size / 2.0), Some(-size / 2.0), None, None))
            .child(handle("bl").pin(Some(-size / 2.0), None, None, Some(-size / 2.0)));
    }
    // `move-handles: 'center'` is the one handle in the middle (`stopwatch/WidgetTimer.vue`).
    if around == "center" {
        layer = layer.child(handle("cc"));
    }
    layer
}

/// The widget's card: the slot `Widget.vue` wraps.
pub fn card<S: 'static>(overlay: &Overlay, widget: &Widget, cx: &mut Cx<S>, project: Project<S>) -> El {
    cx.foreground(theme::gray(100));
    let body = match widget.wm_type.as_str() {
        stopwatch::WM_TYPE => stopwatch::view(overlay, widget, project),
        image_strip::WM_TYPE => image_strip::view(overlay, widget, project),
        delve_grid::WM_TYPE => delve_grid::view(overlay, widget, project),
        item_search::WM_TYPE => item_search::view(overlay, widget, project),
        stash_search::WM_TYPE => stash_search::view(overlay, widget, project),
        xp_tracker::WM_TYPE => xp_tracker::card(overlay, widget, project),
        _ => menu::view(overlay, widget, project),
    };
    chrome(overlay, widget, body, project)
}

/// [`card`] for the widget the window was built around (`Overlay::shown`).
pub fn view<S: 'static>(overlay: &Overlay, cx: &mut Cx<S>, project: Project<S>) -> El {
    let Some(widget) = overlay.shown_widget().cloned() else { return el().key("widgets") };
    scene_root(card(overlay, &widget, cx, project))
}

/// [`card`] for `wm_id`, the widget a live overlay draws among the others.
pub fn view_card<S: 'static>(overlay: &Overlay, wm_id: u32, cx: &mut Cx<S>, project: Project<S>) -> El {
    let Some(widget) = overlay.widget(wm_id).cloned() else { return el() };
    card(overlay, &widget, cx, project)
}

/// A scene's root: the window is the card. Upstream's `.widget` is content-sized and the
/// screenshot harness crops it, so the scene's root places it at the top left, over the tint the
/// reference's overlay paints behind its widgets.
fn scene_root(card: El) -> El {
    col().key("widgets").w_full().h_full().items_start().fill(theme::overlay_background()).child(card)
}

/// A `widget-<name>` / `poe2-widget-<name>` scene: the game and the widget.
pub fn scene(slug: &str) -> Option<(GameId, Kind)> {
    let (game, rest) = match slug.strip_prefix("poe2-") {
        Some(rest) => (GameId::Poe2, rest),
        None => (GameId::Poe1, slug),
    };
    let name = rest.strip_prefix("widget-")?;
    // Only EE2 has the experience tracker: APT's client log feeds chat and trades, not levels
    // (`ref/apt/renderer/src/web/client-log/client-log.ts`).
    if name == Kind::XpTracker.name() && game == GameId::Poe1 {
        return None;
    }
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
        // `.xpContainer`: `min-w-48` (12 rem) with its border, `min-h-10` (2.5 rem); measured
        // from `target/screens/poe2/xp-tracker.png`.
        Kind::XpTracker => (192.0, 40.0),
    }
}

/// The picture the image strip's card draws, from the reference's `/images` (the widget's
/// `images[].url` is a file name, a URL, or an uploaded file's name, which the port has no store
/// for).
pub fn strip_picture(game: GameId, url: &str, size: (f32, f32)) -> El {
    if url.contains("://") { art::cdn(game, url, size) } else { art::local_cover(game, url, size) }
}
