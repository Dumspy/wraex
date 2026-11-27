//! `main/src/host-files/ConfigStore.ts`, the same in both references: the
//! settings as one JSON file.
//!
//! Upstream keeps it at `<appData>/<productName>/apt-data/config.json` (EE2
//! kept APT's folder name). Wraex keeps its own file and can read upstream's
//! ([`upstream_config_path`]) so users keep their settings when switching.

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::GameId;

/// Where the upstream app for `game` keeps its settings, under Electron's
/// `appData` (`%APPDATA%`, `~/.config`, `~/Library/Application Support`).
pub fn upstream_config_path(game: GameId, app_data: &Path) -> PathBuf {
    let product = match game {
        GameId::Poe1 => "Awakened PoE Trade",
        GameId::Poe2 => "Exiled Exchange 2",
    };
    app_data.join(product).join("apt-data").join("config.json")
}

/// The settings file (`ConfigStore`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigStore {
    path: PathBuf,
    is_tmp_file: bool,
}

impl ConfigStore {
    pub const fn new(path: PathBuf) -> Self {
        Self { path, is_tmp_file: false }
    }

    /// Where saves go now (`.tmp` after a temporary save).
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// `load`: the file's text, `None` only when it is missing.
    pub fn load(&self) -> io::Result<Option<String>> {
        match std::fs::read_to_string(&self.path) {
            Ok(contents) => Ok(Some(contents)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// `save`. A temporary save (the defaults standing in for an unreadable
    /// file) switches to `config.json.tmp` for good, so the original stays.
    /// Upstream quits the app when the write fails; this returns the error.
    pub fn save(&mut self, contents: &str, tmp: bool) -> std::io::Result<()> {
        if tmp && !self.is_tmp_file {
            let mut name = self.path.clone().into_os_string();
            name.push(".tmp");
            self.path = PathBuf::from(name);
            self.is_tmp_file = true;
        }
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut file = tempfile::NamedTempFile::new_in(self.path.parent().unwrap_or_else(|| Path::new(".")))?;
        file.write_all(contents.as_bytes())?;
        file.as_file().sync_all()?;
        file.persist(&self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temporary_saves_keep_the_original() {
        let dir = std::env::temp_dir().join(format!("wraex-cfgstore-{}", std::process::id()));
        let mut store = ConfigStore::new(dir.join("apt-data/config.json"));
        assert_eq!(store.load().unwrap(), None);
        store.save("{}", false).unwrap();
        assert_eq!(store.load().unwrap().as_deref(), Some("{}"));
        store.save("{\"tmp\":1}", true).unwrap();
        store.save("{\"tmp\":2}", false).unwrap();
        assert!(store.path().ends_with("config.json.tmp"));
        assert_eq!(std::fs::read_to_string(dir.join("apt-data/config.json")).unwrap(), "{}");
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(
            upstream_config_path(GameId::Poe2, Path::new("/a")),
            PathBuf::from("/a/Exiled Exchange 2/apt-data/config.json")
        );
    }
}
