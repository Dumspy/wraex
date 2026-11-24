//! The tray icon's pixels: the game data's `icon.png`, decoded to RGBA.
//!
//! `AppTray.ts` builds it with Electron's `nativeImage.createFromPath(icon.png).resize({width:
//! 22, height: 22})` on macOS; the other platforms take the file as it ships. The path comes
//! from the game's data layer ([`wraex_poe1::data::icon_path`],
//! [`wraex_poe2::data::icon_path`]): `assets/<game>/icon.png` in a packaged install, the
//! reference's `renderer/public/icon.png` in development.
//!
//! A missing or undecodable icon is not fatal: the tray shows a text-only menu instead
//! (`Tray::new` wants pixels, so the shell then starts without a tray and says so).

use wraex_core::GameId;

/// The macOS status item's size (`AppTray.ts:22`).
#[cfg(target_os = "macos")]
const MACOS_ICON: u32 = 22;

/// `(rgba, width, height)` for the tray.
pub fn icon(game: GameId) -> Result<(Vec<u8>, u32, u32), String> {
    let path = match game {
        GameId::Poe2 => wraex_poe2::data::icon_path(),
        _ => wraex_poe1::data::icon_path(),
    };
    let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let image = image::load_from_memory_with_format(&bytes, image::ImageFormat::Png)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let rgba = image.to_rgba8();
    #[cfg(target_os = "macos")]
    let rgba = if rgba.width() != MACOS_ICON || rgba.height() != MACOS_ICON {
        image::imageops::resize(&rgba, MACOS_ICON, MACOS_ICON, image::imageops::FilterType::Lanczos3)
    } else {
        rgba
    };
    let (width, height) = rgba.dimensions();
    Ok((rgba.into_raw(), width, height))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both references ship the icon the tray uses.
    #[test]
    fn both_games_icons_decode() {
        for game in [GameId::Poe1, GameId::Poe2] {
            let (rgba, width, height) = icon(game).unwrap_or_else(|e| panic!("{game:?}: {e}"));
            assert!(width > 0 && height > 0);
            assert_eq!(rgba.len(), (width * height * 4) as usize);
            #[cfg(target_os = "macos")]
            assert_eq!((width, height), (MACOS_ICON, MACOS_ICON), "AppTray.ts' 22 pt icon");
        }
    }
}
