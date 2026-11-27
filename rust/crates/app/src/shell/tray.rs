//! The tray icon's pixels: the game data's `icon.png`, decoded to RGBA.
//!
//! `AppTray.ts` builds it with Electron's `nativeImage.createFromPath(icon.png).resize({width:
//! 22, height: 22})` on macOS; the other platforms take the file as it ships. The path comes
//! from the game's data layer ([`wraex_poe1::data::icon_path`],
//! [`wraex_poe2::data::icon_path`]): `assets/<game>/icon.png` in a packaged install, the
//! reference's `renderer/public/icon.png` in development.
//!
//! If the icon cannot be decoded, the shell logs the error and continues without a tray.

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
    let rgba = rune::image::png_rgba8(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
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
            #[cfg(not(target_os = "macos"))]
            {
                use sha1::{Digest, Sha1};
                let expected = match game {
                    GameId::Poe1 => "4f16b67e59ba3618e98f86b61c338e0b2bfbdeeb",
                    GameId::Poe2 => "47ac9013d89c7575a44648bd09f7e118b016c613",
                };
                assert_eq!((width, height), (78, 78));
                assert_eq!(format!("{:x}", Sha1::digest(&rgba)), expected);
            }
            #[cfg(target_os = "macos")]
            assert_eq!((width, height), (MACOS_ICON, MACOS_ICON), "AppTray.ts' 22 pt icon");
        }
    }
}
