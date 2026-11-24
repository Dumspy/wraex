//! Where the live app's settings come from: Wraex's own file, upstream's, or the defaults.
//!
//! `ConfigStore.ts`'s `initConfig()` reads `<appData>/<product>/apt-data/config.json`. Wraex
//! keeps its own file instead ([`crate::settings::config_path`]) and, on a machine that has
//! never run Wraex, **imports** upstream's file: it reads it, upgrades it the way the settings
//! window's `load` does, and writes a copy to Wraex's path. Upstream's file is only ever read,
//! so the user keeps both apps side by side and going back changes nothing.
//!
//! The import is a copy, not a link: from then on the two files diverge, and the settings
//! window's Save writes Wraex's.

use std::path::{Path, PathBuf};

use serde_json::Value;
use wraex_core::GameId;
use wraex_core::config_store::{ConfigStore, upstream_config_path};

use crate::settings::{self, Loaded};

/// Where the config the app loaded came from, for the startup log and the tray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Wraex's own file.
    Wraex,
    /// Upstream's file, copied to Wraex's path on this first run.
    ImportedUpstream,
    /// `defaultConfig()`: no file to read.
    Defaults,
}

/// What the shell needs to know about the settings it loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Startup {
    pub game: GameId,
    pub store: ConfigStore,
    pub config: Value,
    /// An unreadable file was replaced by the defaults: saves go to `config.json.tmp`
    /// (`Settings::save_temporary`).
    pub save_temporary: bool,
    pub source: Source,
    /// Upstream's file, when one was there to import from.
    pub upstream: Option<PathBuf>,
}

impl Startup {
    /// The folder the tray's "Open config folder" shows: the one Wraex saves into.
    pub fn folder(&self) -> Option<&Path> {
        self.store.path().parent()
    }
}

/// `initConfig()` for a live run: Wraex's file, else upstream's (copied), else the defaults.
/// `app_data` is Electron's `appData`; [`startup`] gets it from the environment.
pub fn load(game: GameId, app_data: &Path) -> Startup {
    let mut store = ConfigStore::new(settings::config_path_in(game, app_data));
    let upstream = upstream_config_path(game, app_data);
    if let Some(contents) = store.load() {
        let loaded = settings::load(game, Some(&contents));
        return Startup {
            game,
            store,
            save_temporary: loaded.save_temporary,
            config: loaded.config,
            source: Source::Wraex,
            upstream: upstream.exists().then_some(upstream),
        };
    }

    // A first run: upstream's settings, when there are some, become Wraex's. `load` upgrades
    // them to the current `configVersion`; a broken upgrade keeps the defaults and is left for
    // the settings window to save as `config.json.tmp`, so neither file is overwritten here.
    if let Ok(contents) = std::fs::read_to_string(&upstream) {
        let Loaded { config, save_temporary } = settings::load(game, Some(&contents));
        if !save_temporary && write(&mut store, &config) {
            return Startup {
                game,
                store,
                save_temporary: false,
                config,
                source: Source::ImportedUpstream,
                upstream: Some(upstream),
            };
        }
        return Startup { game, store, save_temporary, config, source: Source::Defaults, upstream: Some(upstream) };
    }

    Startup {
        game,
        store,
        save_temporary: false,
        config: settings::default_config(game),
        source: Source::Defaults,
        upstream: None,
    }
}

/// [`load`] for the running process.
pub fn startup(game: GameId) -> Startup {
    match settings::app_data_dir() {
        Some(app_data) => load(game, &app_data),
        // No `appData` in the environment: keep the app usable with the defaults, in the temp
        // dir the settings scenes already use.
        None => {
            let store = settings::temp_store(game);
            Startup {
                game,
                store,
                save_temporary: false,
                config: settings::default_config(game),
                source: Source::Defaults,
                upstream: None,
            }
        }
    }
}

/// `ConfigStore.save`: the imported copy, compact JSON as `saveConfig` writes it.
fn write(store: &mut ConfigStore, config: &Value) -> bool {
    serde_json::to_string(config).is_ok_and(|json| store.save(&json, false).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp `appData` per test, removed by the guard.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("wraex-shell-config-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_first_run_imports_upstreams_file_as_a_copy() {
        let dir = Dir::new("import");
        let upstream = upstream_config_path(GameId::Poe1, dir.path());
        std::fs::create_dir_all(upstream.parent().unwrap()).unwrap();
        let original = wraex_poe1::config::default_config().to_string();
        std::fs::write(&upstream, &original).unwrap();

        let startup = load(GameId::Poe1, dir.path());
        assert_eq!(startup.source, Source::ImportedUpstream);
        assert_eq!(startup.upstream.as_ref(), Some(&upstream));
        assert!(!startup.save_temporary);
        // Wraex's copy is there and holds upstream's settings, upgraded to the current format.
        let copied = std::fs::read_to_string(settings::config_path_in(GameId::Poe1, dir.path())).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&copied).unwrap(), startup.config);
        assert_eq!(startup.config["configVersion"], wraex_poe1::config::CONFIG_VERSION);
        // Upstream's own file was not touched.
        assert_eq!(std::fs::read_to_string(&upstream).unwrap(), original);
    }

    #[test]
    fn wraexs_own_file_wins_and_upstream_is_left_alone() {
        let dir = Dir::new("own");
        let path = settings::config_path_in(GameId::Poe2, dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut mine = wraex_poe2::config::default_config();
        mine["overlayKey"] = Value::String("Ctrl + O".to_owned());
        std::fs::write(&path, mine.to_string()).unwrap();
        let upstream = upstream_config_path(GameId::Poe2, dir.path());
        std::fs::create_dir_all(upstream.parent().unwrap()).unwrap();
        std::fs::write(&upstream, "not json at all").unwrap();

        let startup = load(GameId::Poe2, dir.path());
        assert_eq!(startup.source, Source::Wraex);
        assert_eq!(startup.config["overlayKey"], "Ctrl + O");
        assert_eq!(std::fs::read_to_string(&upstream).unwrap(), "not json at all");
        assert_eq!(startup.store.path(), path);
    }

    #[test]
    fn no_file_anywhere_is_the_defaults() {
        let dir = Dir::new("none");
        let startup = load(GameId::Poe1, dir.path());
        assert_eq!(startup.source, Source::Defaults);
        assert_eq!(startup.upstream, None);
        assert_eq!(startup.config, wraex_poe1::config::default_config());
        assert!(!startup.save_temporary);
        assert_eq!(startup.folder(), settings::config_path_in(GameId::Poe1, dir.path()).parent());
    }

    #[test]
    fn a_broken_upstream_file_keeps_the_defaults_and_writes_nothing() {
        let dir = Dir::new("broken");
        let upstream = upstream_config_path(GameId::Poe1, dir.path());
        std::fs::create_dir_all(upstream.parent().unwrap()).unwrap();
        std::fs::write(&upstream, "{\"configVersion\": \"not a number\"}").unwrap();

        let startup = load(GameId::Poe1, dir.path());
        assert_eq!(startup.source, Source::Defaults);
        assert!(startup.save_temporary, "the settings window saves to config.json.tmp");
        assert!(startup.upstream.is_some());
        assert!(!settings::config_path_in(GameId::Poe1, dir.path()).exists(), "nothing imported");
        assert_eq!(std::fs::read_to_string(&upstream).unwrap(), "{\"configVersion\": \"not a number\"}");
    }
}
