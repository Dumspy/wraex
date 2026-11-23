//! The game's own files: `main/src/host-files/{GameConfig,GameLogWatcher,utils}.ts`
//! of both references, which differ only in paths and EE2's extras (log reading
//! can be switched off and re-read from the start).
//!
//! - [`guess_file_location`]: exactly one readable candidate, else none
//!   (standalone and Steam both installed is ambiguous).
//! - [`show_mods_key`]: the "show advanced item descriptions" hotkey from the
//!   game's INI, which the app presses when copying items.
//! - [`LogWatcher`]: new lines of `Client.txt`, read from where the file ended
//!   when watching started. The caller polls it ([`LOG_POLL_INTERVAL_MS`]).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::GameId;
use crate::keys::{code_to_key, hotkey_to_string};

/// The folders the default locations are under (Electron's `app.getPath`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserDirs {
    pub home: PathBuf,
    pub documents: PathBuf,
    /// `appData`: `~/Library/Application Support` on macOS.
    pub app_data: PathBuf,
}

/// Which operating system's default locations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    MacOs,
}

impl Os {
    pub const fn current() -> Option<Os> {
        if cfg!(windows) {
            Some(Os::Windows)
        } else if cfg!(target_os = "linux") {
            Some(Os::Linux)
        } else if cfg!(target_os = "macos") {
            Some(Os::MacOs)
        } else {
            None
        }
    }
}

/// `POSSIBLE_PATH` of `GameConfig.ts`: where the game keeps its settings.
pub fn config_paths(game: GameId, os: Os, dirs: &UserDirs) -> Vec<PathBuf> {
    let (folder, file, steam_id) = match game {
        GameId::Poe1 => ("Path of Exile", "production_Config.ini", "238960"),
        GameId::Poe2 => ("Path of Exile 2", "poe2_production_Config.ini", "2694490"),
    };
    match os {
        Os::Windows => vec![dirs.documents.join("My Games").join(folder).join(file)],
        Os::Linux => vec![
            dirs.documents.join("My Games").join(folder).join(file),
            dirs.home
                .join(format!(
                    ".local/share/Steam/steamapps/compatdata/{steam_id}/pfx/drive_c/users/steamuser/Documents/My Games"
                ))
                .join(folder)
                .join(file),
        ],
        Os::MacOs => vec![dirs.app_data.join(folder).join("Preferences").join(file)],
    }
}

/// `POSSIBLE_PATH` of `GameLogWatcher.ts`: where the game writes `Client.txt`.
/// PoE2 on macOS shares PoE1's cache folder upstream.
pub fn log_paths(game: GameId, os: Os, dirs: &UserDirs) -> Vec<PathBuf> {
    let folder = match game {
        GameId::Poe1 => "Path of Exile",
        GameId::Poe2 => "Path of Exile 2",
    };
    match os {
        Os::Windows => vec![
            PathBuf::from(format!(r"C:\Program Files (x86)\Grinding Gear Games\{folder}\logs\Client.txt")),
            PathBuf::from(format!(r"C:\Program Files (x86)\Steam\steamapps\common\{folder}\logs\Client.txt")),
        ],
        Os::Linux => vec![
            dirs.home.join(format!(".wine/drive_c/Program Files (x86)/Grinding Gear Games/{folder}/logs/Client.txt")),
            dirs.home.join(format!(".local/share/Steam/steamapps/common/{folder}/logs/Client.txt")),
        ],
        Os::MacOs => vec![dirs.home.join("Library/Caches/com.GGG.PathOfExile/Logs/Client.txt")],
    }
}

/// `guessFileLocation`: the one readable candidate; none if zero or several.
pub fn guess_file_location(paths: &[PathBuf]) -> Option<PathBuf> {
    let mut found = None;
    for path in paths {
        if File::open(path).is_ok() {
            if found.is_some() {
                return None;
            }
            found = Some(path.clone());
        }
    }
    found
}

/// The value of `key` in `[section]` of an INI text (the `ini` package's
/// reading for these files: `key=value` lines, both trimmed).
fn ini_value<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut in_section = false;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            in_section = name.trim() == section;
            continue;
        }
        if in_section
            && let Some((k, v)) = line.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim());
        }
    }
    None
}

/// `parseConfigHotkey`: `"<code> <modifier>"`, the modifier 1 Shift, 2 Ctrl,
/// 3 Alt. `None` (logged upstream) for codes it doesn't know.
pub fn parse_config_hotkey(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let mut parts = value.split(' ');
    let key = code_to_key(parts.next().unwrap_or(""))?;
    let modifier = match parts.next() {
        None => None,
        Some("1") => Some("Shift"),
        Some("2") => Some("Ctrl"),
        Some("3") => Some("Alt"),
        Some(_) => return None,
    };
    Some(hotkey_to_string(&[key], modifier == Some("Ctrl"), modifier == Some("Shift"), modifier == Some("Alt")))
}

/// The game's "show advanced item descriptions" hotkey (`showModsKeyNullable`),
/// from its config file. `None` if the file or the key can't be read; the app
/// then uses `Alt` (`showModsKey`).
pub fn show_mods_key(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    // `trimStart()` drops a BOM.
    let text = text.trim_start_matches(|c: char| c == '\u{feff}' || crate::js::is_whitespace(c));
    parse_config_hotkey(ini_value(text, "ACTION_KEYS", "show_advanced_item_descriptions")?)
}

/// How often upstream checks the log for changes (`watchFile` interval).
pub const LOG_POLL_INTERVAL_MS: u64 = 450;

/// Bytes read at a time (`readBuff`).
const READ_CHUNK: usize = 64 * 1024;

/// Watches `Client.txt` for new lines (`GameLogWatcher`).
#[derive(Debug)]
pub struct LogWatcher {
    path: PathBuf,
    file: File,
    offset: u64,
}

impl LogWatcher {
    /// Starts at the current end of the file: old lines are not reported.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let offset = file.metadata()?.len();
        Ok(Self { path: path.to_owned(), file, offset })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `re-parse-log` (EE2): read the whole file again on the next poll.
    pub fn reread_from_start(&mut self) {
        self.offset = 0;
    }

    /// `readToEOF`: the lines written since the last poll, trimmed, empty ones
    /// dropped. Like upstream, each 64 KiB chunk is split on its own, so a line
    /// cut by a chunk boundary comes out as two.
    pub fn poll(&mut self) -> std::io::Result<Vec<Vec<String>>> {
        let mut batches = Vec::new();
        let mut buf = vec![0u8; READ_CHUNK];
        loop {
            self.file.seek(SeekFrom::Start(self.offset))?;
            let read = self.file.read(&mut buf)?;
            if read == 0 {
                break;
            }
            let text = String::from_utf8_lossy(&buf[..read]);
            let lines: Vec<String> =
                text.split('\n').map(|l| crate::js::trim(l).to_owned()).filter(|l| !l.is_empty()).collect();
            batches.push(lines);
            self.offset += read as u64;
        }
        Ok(batches)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn dirs() -> UserDirs {
        UserDirs { home: "/home/u".into(), documents: "/home/u/Documents".into(), app_data: "/home/u/.config".into() }
    }

    #[test]
    fn default_locations_per_game() {
        let c = config_paths(GameId::Poe2, Os::Linux, &dirs());
        assert_eq!(c[0], PathBuf::from("/home/u/Documents/My Games/Path of Exile 2/poe2_production_Config.ini"));
        assert!(c[1].to_string_lossy().contains("compatdata/2694490/"));
        assert_eq!(config_paths(GameId::Poe1, Os::Windows, &dirs()).len(), 1);
        let l = log_paths(GameId::Poe1, Os::MacOs, &dirs());
        assert_eq!(l, [PathBuf::from("/home/u/Library/Caches/com.GGG.PathOfExile/Logs/Client.txt")]);
    }

    #[test]
    fn hotkeys_from_the_config() {
        assert_eq!(parse_config_hotkey("18"), Some("Alt".into()));
        assert_eq!(parse_config_hotkey("68 2"), Some("Ctrl + D".into()));
        assert_eq!(parse_config_hotkey("68 4"), None);
        assert_eq!(parse_config_hotkey("9999"), None);

        let dir = std::env::temp_dir().join(format!("wraex-gamecfg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ini = dir.join("production_Config.ini");
        std::fs::write(
            &ini,
            "\u{feff}[DISPLAY]\nfullscreen=false\n[ACTION_KEYS]\nshow_advanced_item_descriptions=68 3\n",
        )
        .unwrap();
        assert_eq!(show_mods_key(&ini), Some("Alt + D".into()));
        assert_eq!(guess_file_location(&[ini.clone(), dir.join("missing")]), Some(ini.clone()));
        assert_eq!(guess_file_location(&[ini.clone(), ini.clone()]), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_log_reports_only_new_lines() {
        let dir = std::env::temp_dir().join(format!("wraex-log-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("Client.txt");
        std::fs::write(&log, "old line\n").unwrap();
        let mut watcher = LogWatcher::open(&log).unwrap();
        assert!(watcher.poll().unwrap().is_empty());

        let mut f = std::fs::OpenOptions::new().append(true).open(&log).unwrap();
        writeln!(f, "2026/11/24 10:00:00 1 [INFO Client 1] : You have entered Hideout.\r\n  ").unwrap();
        assert_eq!(
            watcher.poll().unwrap(),
            [vec!["2026/11/24 10:00:00 1 [INFO Client 1] : You have entered Hideout.".to_owned()]]
        );
        watcher.reread_from_start();
        assert_eq!(watcher.poll().unwrap()[0][0], "old line");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
