//! The folder the app reads its runtime assets from: game data, images and the tray icon.
//!
//! Everything the references serve from `renderer/public/` is read at runtime. A release
//! build ships those files beside the executable, so the binary works wherever it is
//! unpacked; `cargo run` and the tests keep reading the reference checkouts.
//!
//! The root is resolved once, in this order:
//!
//! 1. `WRAEX_ASSETS`, if set. It is taken as given, even when the directory is missing, so
//!    a typo fails loudly (the data loader names the file it could not read) instead of
//!    silently reading a different root.
//! 2. `<exe dir>/assets`, a portable or unpacked release (Windows, Linux, a `.app`'s
//!    `Contents/MacOS` when the packager put the assets there too).
//! 3. `Contents/Resources/assets` beside `<exe dir>` when the executable lives in a macOS
//!    app bundle's `Contents/MacOS`.
//! 4. Nothing: [`Origin::Development`]. The game crates fall back to the source tree they
//!    were built from (`ref/<game>/renderer/public`), which is what `cargo run`, `--snap`
//!    and the tests need.
//!
//! Inside a root, each game owns a directory named after its `GameId::as_str` (`poe1`,
//! `poe2`), laid out like the reference's `renderer/public`:
//!
//! ```text
//! assets/poe1/data/<lang>/{client_strings.js,items.ndjson,stats.ndjson,app_i18n.json}
//! assets/poe1/images/<name>       ref/apt/renderer/public/images
//! assets/poe1/icon.png            ref/apt/renderer/public/icon.png
//! assets/poe2/...                 the same for ref/ee2
//! licences/                       Wraex's, the references' and Font Awesome's notices
//! ```
//!
//! `tools/package/package.py` writes that layout. The games read it through
//! [`crate::data::DataSource`] and their `wraex_poe1::data::*`/`wraex_poe2::data::*`
//! accessors, so this module stays game-agnostic: it only knows where a root is.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The environment variable that names the assets root explicitly.
pub const ENV: &str = "WRAEX_ASSETS";

/// How the assets root was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// [`ENV`] named it.
    Environment,
    /// `<exe dir>/assets`.
    Executable,
    /// `Contents/Resources/assets` in a macOS app bundle.
    MacBundle,
    /// Nothing next to the executable: a development build reads the source tree.
    Development,
}

impl Origin {
    /// A short name for the startup log.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Environment => "WRAEX_ASSETS",
            Self::Executable => "next to the executable",
            Self::MacBundle => "the app bundle",
            Self::Development => "development",
        }
    }
}

/// One assets root, resolved once per process ([`Assets::get`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Assets {
    root: Option<PathBuf>,
    origin: Origin,
}

impl Assets {
    /// Resolve a root without touching the process's environment or executable: `exe` is the
    /// running binary, `env_root` the `WRAEX_ASSETS` value. Kept pure so the search order is
    /// testable.
    pub fn resolve(exe: Option<&Path>, env_root: Option<&Path>) -> Self {
        if let Some(root) = env_root.filter(|root| !root.as_os_str().is_empty()) {
            return Self::at(root.to_path_buf(), Origin::Environment);
        }
        let Some(bin_dir) = exe.and_then(Path::parent) else {
            return Self::development();
        };
        let beside = bin_dir.join("assets");
        if beside.is_dir() {
            return Self::at(beside, Origin::Executable);
        }
        if let Some(bundle) = macos_bundle_assets(bin_dir) {
            return Self::at(bundle, Origin::MacBundle);
        }
        Self::development()
    }

    /// The running process's root: `WRAEX_ASSETS`, else next to the executable, else the
    /// app bundle, else [`Origin::Development`]. Resolved on first use and kept for the
    /// process's lifetime.
    pub fn get() -> &'static Self {
        static ASSETS: OnceLock<Assets> = OnceLock::new();
        ASSETS.get_or_init(|| {
            let env_root = std::env::var_os(ENV).map(PathBuf::from);
            let exe = std::env::current_exe().ok();
            Self::resolve(exe.as_deref(), env_root.as_deref())
        })
    }

    /// The packaged root, or `None` in development.
    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    /// How [`Assets::get`] found it.
    pub const fn origin(&self) -> Origin {
        self.origin
    }

    /// A path inside the packaged root, or `None` in development (the caller falls back to
    /// the source tree).
    pub fn path(&self, relative: impl AsRef<Path>) -> Option<PathBuf> {
        self.root.as_ref().map(|root| root.join(relative))
    }

    /// The startup log's one line: where the assets are, or that this is a development build.
    pub fn summary(&self) -> String {
        self.root.as_deref().map_or_else(
            || {
                format!(
                    "the source tree (no `assets` next to the executable; set {ENV} or run tools/package/package.py)"
                )
            },
            |root| format!("{} (from {})", root.display(), self.origin.as_str()),
        )
    }

    const fn at(root: PathBuf, origin: Origin) -> Self {
        Self { root: Some(root), origin }
    }

    const fn development() -> Self {
        Self { root: None, origin: Origin::Development }
    }
}

/// `Contents/Resources/assets` when `bin_dir` is a macOS bundle's `Contents/MacOS`.
fn macos_bundle_assets(bin_dir: &Path) -> Option<PathBuf> {
    let is_macos_dir = bin_dir.file_name().is_some_and(|name| name == "MacOS");
    let is_contents_dir = bin_dir.parent().and_then(Path::file_name).is_some_and(|name| name == "Contents");
    let resources = bin_dir.parent()?.join("Resources").join("assets");
    (is_macos_dir && is_contents_dir && resources.is_dir()).then_some(resources)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A temp directory per test, removed by the guard.
    struct Dir(PathBuf);

    impl Dir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!("wraex-assets-{name}-{}", std::process::id()));
            drop(std::fs::remove_dir_all(&path));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }

        fn dir(&self, relative: &str) -> PathBuf {
            let path = self.0.join(relative);
            std::fs::create_dir_all(&path).unwrap();
            path
        }
    }

    impl Drop for Dir {
        fn drop(&mut self) {
            drop(std::fs::remove_dir_all(&self.0));
        }
    }

    #[test]
    fn the_environment_wins() {
        let dir = Dir::new("env");
        let beside = dir.dir("bin/assets");
        let env = dir.dir("elsewhere");
        let assets = Assets::resolve(Some(&dir.path().join("bin/wraex")), Some(&env));
        assert_eq!(assets.origin(), Origin::Environment);
        assert_eq!(assets.root(), Some(env.as_path()));
        assert_eq!(assets.path("poe1/data"), Some(env.join("poe1/data")));
        assert!(beside.is_dir(), "the executable's assets are ignored when the env names a root");
    }

    #[test]
    fn a_portable_release_reads_assets_beside_the_binary() {
        let dir = Dir::new("portable");
        let assets = dir.dir("bin/assets");
        let resolved = Assets::resolve(Some(&dir.path().join("bin/wraex")), None);
        assert_eq!(resolved.origin(), Origin::Executable);
        assert_eq!(resolved.root(), Some(assets.as_path()));
        assert_eq!(resolved.path("poe2/icon.png"), Some(assets.join("poe2/icon.png")));
    }

    #[test]
    fn a_macos_bundle_reads_contents_resources() {
        let dir = Dir::new("bundle");
        let resources = dir.dir("Wraex.app/Contents/Resources/assets");
        let exe = dir.path().join("Wraex.app/Contents/MacOS/wraex");
        let resolved = Assets::resolve(Some(&exe), None);
        assert_eq!(resolved.origin(), Origin::MacBundle);
        assert_eq!(resolved.root(), Some(resources.as_path()));
    }

    #[test]
    fn a_build_without_assets_uses_the_source_tree() {
        let dir = Dir::new("dev");
        let resolved = Assets::resolve(Some(&dir.path().join("target/debug/wraex")), None);
        assert_eq!(resolved.origin(), Origin::Development);
        assert_eq!(resolved.root(), None);
        assert_eq!(resolved.path("poe1/data"), None, "the caller falls back to ref/");
        assert!(resolved.summary().contains(ENV));
        // An empty override is not a root either.
        assert_eq!(Assets::resolve(Some(&dir.path().join("wraex")), Some(Path::new(""))).origin(), Origin::Development);
        // No executable to look beside at all.
        assert_eq!(Assets::resolve(None, None).origin(), Origin::Development);
    }

    #[test]
    fn this_checkout_resolves_to_the_source_tree() {
        // Tests run from `rust/target/debug/deps`, where no `assets` directory is.
        assert_eq!(Assets::get().origin(), Origin::Development);
        assert!(Assets::get().root().is_none());
    }
}
