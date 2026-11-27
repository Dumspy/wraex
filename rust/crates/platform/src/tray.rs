//! The tray icon and its menu.
//!
//! Upstream: `main/src/AppTray.ts`. Upstream's "Open in Browser" entry is gone
//! because Wraex has no local web server. The entries only report a
//! [`TrayAction`]; showing the settings hint and opening the config folder is the
//! app's job, so this module knows nothing about dialogs or paths.
//!
//! The "Update ready" entry is Wraex's own: upstream only shows that state on the settings
//! window's About page, while the port also offers the restart from the tray, which is where
//! a tray-only app is quit from.
//!
//! On macOS the tray must be created on the main thread with an event loop
//! already running. On Linux `tray-icon`'s default backend needs GTK 3 and
//! `libayatana-appindicator3` at runtime.

use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use wraex_core::GameId;

/// The product name in the tooltip. Upstream shows its own app name there
/// (`AppTray.ts:26`); Wraex is one product for both games.
const PRODUCT: &str = "Wraex";

/// What a menu entry does, once the app gets to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    /// `AppTray.ts`: the "Settings/League" message box.
    ShowSettingsHint,
    OpenConfigFolder,
    /// The verified update is waiting: restart into it (`autoUpdater.quitAndInstall(false)`).
    /// Wraex's own entry (upstream shows the state only on the About page).
    InstallUpdate,
    Quit,
}

impl TrayAction {
    pub const ALL: [TrayAction; 4] =
        [TrayAction::ShowSettingsHint, TrayAction::OpenConfigFolder, TrayAction::InstallUpdate, TrayAction::Quit];

    /// Stable id for the native menu item; it survives a menu rebuild.
    pub const fn menu_id(self) -> &'static str {
        match self {
            TrayAction::ShowSettingsHint => "settings-league",
            TrayAction::OpenConfigFolder => "open-config-folder",
            TrayAction::InstallUpdate => "install-update",
            TrayAction::Quit => "quit",
        }
    }

    /// The action behind a [`MenuEvent`] id, if it is one of ours.
    pub fn from_menu_id(id: &str) -> Option<TrayAction> {
        TrayAction::ALL.into_iter().find(|a| a.menu_id() == id)
    }
}

/// One entry of the context menu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuEntry {
    Item { label: String, action: TrayAction },
    Separator,
}

/// The menu, `AppTray.ts`'s template order minus "Open in Browser". With a downloaded update
/// the first entry says so and installs it on a restart; upstream shows that state only on the
/// About page, so this entry is Wraex's own.
pub fn menu_items(update_ready: Option<&str>) -> Vec<MenuEntry> {
    let mut items = Vec::new();
    if let Some(version) = update_ready {
        items.push(MenuEntry::Item {
            label: format!("Update ready (v{version}): restart to install"),
            action: TrayAction::InstallUpdate,
        });
        items.push(MenuEntry::Separator);
    }
    items.extend([
        MenuEntry::Item { label: "Settings/League".to_owned(), action: TrayAction::ShowSettingsHint },
        MenuEntry::Separator,
        MenuEntry::Item { label: "Open config folder".to_owned(), action: TrayAction::OpenConfigFolder },
        MenuEntry::Item { label: "Quit".to_owned(), action: TrayAction::Quit },
    ]);
    items
}

/// `<product> v<version>`, upstream's wording with Wraex as the product.
///
/// Upstream ships one app per game under its own name; Wraex is one product, so
/// the tooltip does not name the game.
pub fn tooltip(version: &str) -> String {
    format!("{PRODUCT} v{version}")
}

/// The message box body of "Settings/League" (`AppTray.ts:43`).
pub fn settings_hint(game: GameId, overlay_key: &str) -> String {
    let game_name = match game {
        GameId::Poe1 => "Path of Exile",
        GameId::Poe2 => "Path of Exile 2",
    };
    format!("Open {game_name} and press \"{overlay_key}\". Click on the button with cog icon there.")
}

/// The native tray. Keep it alive for as long as the icon should be visible;
/// dropping it removes the icon.
pub struct Tray {
    tray: TrayIcon,
    overlay_key: String,
    /// The version in the "Update ready" entry, once a download is verified.
    update_ready: Option<String>,
    /// Dropping a menu item removes it from the native menu, so the builders'
    /// output has to outlive [`Self::tray`].
    items: Vec<Box<dyn IsMenuItem>>,
}

impl std::fmt::Debug for Tray {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tray")
            .field("overlay_key", &self.overlay_key)
            .field("update_ready", &self.update_ready)
            .finish_non_exhaustive()
    }
}

impl Tray {
    /// Creates the icon and its menu. `icon_rgba` is `width * height` RGBA
    /// pixels; macOS wants a small icon (22x22, `AppTray.ts:22`).
    pub fn new(version: &str, overlay_key: &str, icon_rgba: Vec<u8>, width: u32, height: u32) -> Result<Tray, String> {
        let icon = Icon::from_rgba(icon_rgba, width, height).map_err(|e| format!("tray icon: {e}"))?;
        let (menu, items) = native_menu(None)?;
        let tray = TrayIconBuilder::new()
            .with_tooltip(tooltip(version))
            .with_icon(icon)
            .with_menu(Box::new(menu))
            .build()
            .map_err(|e| format!("tray: {e}"))?;
        Ok(Tray { tray, overlay_key: overlay_key.to_owned(), update_ready: None, items })
    }

    /// The key the settings hint quotes.
    pub fn overlay_key(&self) -> &str {
        &self.overlay_key
    }

    /// The overlay key is configurable, so upstream rebuilds the menu on every
    /// change (`AppTray.ts:27`). Nothing in the labels depends on it yet, but the
    /// menu is replaced the same way so the two stay in step.
    pub fn set_overlay_key(&mut self, key: impl Into<String>) -> Result<(), String> {
        self.overlay_key = key.into();
        self.rebuild_menu()
    }

    /// A verified download changes the first entry (see [`menu_items`]); `None` removes it
    /// again, which a shell that starts over needs.
    pub fn set_update_ready(&mut self, version: Option<String>) -> Result<(), String> {
        self.update_ready = version;
        self.rebuild_menu()
    }

    /// Replaces the native menu after a label changed, keeping the entries' build-out alive.
    fn rebuild_menu(&mut self) -> Result<(), String> {
        let (menu, items) = native_menu(self.update_ready.as_deref())?;
        self.tray.set_menu(Some(Box::new(menu)));
        self.items = items;
        Ok(())
    }

    /// The next menu click, if there is one. Call it from the event loop: an
    /// empty queue is normal, not an error.
    pub fn poll_action(&self) -> Option<TrayAction> {
        while let Ok(event) = MenuEvent::receiver().try_recv() {
            if let Some(action) = TrayAction::from_menu_id(event.id.as_ref()) {
                return Some(action);
            }
        }
        None
    }
}

/// Deliver every menu click to `f` instead of leaving it in the queue. The platform runs `f`
/// on its own event thread; an app forwards it to its UI thread (Rune's `AppHandle::update`)
/// rather than polling. Replaces an earlier handler.
pub fn set_menu_handler(f: impl Fn(TrayAction) + Send + Sync + 'static) {
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        if let Some(action) = TrayAction::from_menu_id(event.id.as_ref()) {
            f(action);
        }
    }));
}

/// The native menu, and the items backing it.
fn native_menu(update_ready: Option<&str>) -> Result<(Menu, Vec<Box<dyn IsMenuItem>>), String> {
    let items: Vec<Box<dyn IsMenuItem>> = menu_items(update_ready)
        .into_iter()
        .map(|entry| match entry {
            MenuEntry::Item { label, action } => {
                Box::new(MenuItem::with_id(action.menu_id(), label, true, None)) as Box<dyn IsMenuItem>
            }
            MenuEntry::Separator => Box::new(PredefinedMenuItem::separator()),
        })
        .collect();
    let refs: Vec<&dyn IsMenuItem> = items.iter().map(|item| item.as_ref()).collect();
    let menu = Menu::with_items(&refs).map_err(|e| format!("tray menu: {e}"))?;
    Ok((menu, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tooltip_is_product_and_version() {
        assert_eq!(tooltip("0.1.0"), "Wraex v0.1.0");
    }

    #[test]
    fn menu_is_upstreams_without_open_in_browser() {
        assert_eq!(
            menu_items(None),
            vec![
                MenuEntry::Item { label: "Settings/League".to_owned(), action: TrayAction::ShowSettingsHint },
                MenuEntry::Separator,
                MenuEntry::Item { label: "Open config folder".to_owned(), action: TrayAction::OpenConfigFolder },
                MenuEntry::Item { label: "Quit".to_owned(), action: TrayAction::Quit },
            ]
        );
    }

    /// A verified download adds Wraex's own "Update ready" entry above upstream's.
    #[test]
    fn menu_shows_a_ready_update_first() {
        assert_eq!(
            menu_items(Some("0.2.0")),
            vec![
                MenuEntry::Item {
                    label: "Update ready (v0.2.0): restart to install".to_owned(),
                    action: TrayAction::InstallUpdate,
                },
                MenuEntry::Separator,
                MenuEntry::Item { label: "Settings/League".to_owned(), action: TrayAction::ShowSettingsHint },
                MenuEntry::Separator,
                MenuEntry::Item { label: "Open config folder".to_owned(), action: TrayAction::OpenConfigFolder },
                MenuEntry::Item { label: "Quit".to_owned(), action: TrayAction::Quit },
            ]
        );
    }

    #[test]
    fn menu_ids_round_trip() {
        for action in TrayAction::ALL {
            assert_eq!(TrayAction::from_menu_id(action.menu_id()), Some(action));
        }
        assert_eq!(TrayAction::from_menu_id("open-in-browser"), None);
        assert_eq!(TrayAction::from_menu_id(""), None);
    }

    #[test]
    fn settings_hint_names_the_game_and_the_key() {
        assert_eq!(
            settings_hint(GameId::Poe1, "Shift + Space"),
            "Open Path of Exile and press \"Shift + Space\". Click on the button with cog icon there."
        );
        assert_eq!(
            settings_hint(GameId::Poe2, "Ctrl + D"),
            "Open Path of Exile 2 and press \"Ctrl + D\". Click on the button with cog icon there."
        );
    }
}
