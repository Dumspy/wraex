//! Upstream's images: the reference's `renderer/public/images` and the game's item art.
//!
//! The reference windows draw pictures in three shapes, and this module gives each one a
//! Rune element:
//!
//! - **Local art** (`/images/chaos.png`, `stash.png`, `dust.png`, the influence badges, the
//!   oils' and catalysts' art) ships in `ref/<game>/renderer/public/images`. The port reads it
//!   from the reference through the data layer ([`wraex_poe1::data::image_path`],
//!   [`wraex_poe2::data::image_path`]), the way it reads the data files.
//! - **Item art** is a `https://web.poecdn.com/...` URL from the game data (an item's `icon`)
//!   that the reference's browser fetches itself, outside `/proxy`. A window fetches it once
//!   per URL on a worker thread ([`wraex_platform::http::HttpTransport::get_bytes`]) and keeps
//!   the decoded image; a headless run reads the recording the screenshot harness made
//!   (`tools/golden/corpus/screens/<game>/images/`), so a scene never touches the network.
//! - Anything not loaded yet, and anything that failed, is the dark square
//!   [`icon::image_placeholder`] draws, so the layout never jumps.
//!
//! Decoding is Rune's [`ImageCache`]: created once with the app's waker, so an image that
//! finishes decoding wakes the window. Art read from disk (the reference's files, the
//! harness's recordings) is waited for, so a `--snap` render and a test frame show their
//! pictures instead of racing the decoder; a live download never blocks a frame.
//!
//! A ready image is an element whose role is `img` and whose label is its source; the
//! placeholder is an ordinary box. Tests tell them apart that way.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use rune::{El, Image, ImageCache, SurfaceFit, Waker, el};
use wraex_core::GameId;
use wraex_core::images::is_cdn_url;

use crate::trade::Mode;
use crate::ui::icon;

/// The ctx-free registry: one entry per source the window has asked for.
///
/// `images` keeps the decoded handle alive for as long as the window draws it; `failed` and
/// `pending` make a source that cannot load (or is loading) cost one lookup per frame.
#[derive(Default)]
struct Registry {
    images: HashMap<String, Image>,
    failed: HashSet<String>,
    /// Fetches (live) or reads (replay) that have not finished.
    pending: HashSet<String>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

/// The decode worker's cache; [`install_waker`] creates it with the app's waker.
static CACHE: OnceLock<ImageCache> = OnceLock::new();

/// The app's waker (`AppSpec::setup`), called when a worker finishes a fetch or a decode.
static WAKE: Mutex<Option<Waker>> = Mutex::new(None);

/// Whether CDN art is fetched or replayed. One process runs one mode (a window sends, a
/// headless run replays), so the window that is built tells the images which it is rather than
/// every widget taking a `Mode` argument.
static MODE: AtomicU8 = AtomicU8::new(MODE_UNSET);
const MODE_UNSET: u8 = 0;
const MODE_LIVE: u8 = 1;
const MODE_REPLAY: u8 = 2;

/// The mode a window was built with: [`crate::trade::Mode::Replay`] reads the screenshot
/// harness's recordings, [`crate::trade::Mode::Live`] fetches from the CDN. The first caller
/// wins; every later one is the same window's re-render.
pub fn set_mode(mode: Mode) {
    let value = match mode {
        Mode::Live => MODE_LIVE,
        Mode::Replay => MODE_REPLAY,
    };
    let _ = MODE.compare_exchange(MODE_UNSET, value, Ordering::Relaxed, Ordering::Relaxed);
}

/// [`set_mode`]'s value, or what a headless run would use when no window was built.
fn mode() -> Mode {
    match MODE.load(Ordering::Relaxed) {
        MODE_LIVE => Mode::Live,
        MODE_REPLAY => Mode::Replay,
        _ => Mode::for_run(),
    }
}

/// Give the image cache the waker the window redraws with: `AppSpec::setup`'s. Called once at
/// startup; a test that never calls it still renders, because it waits for its images.
pub fn install_waker(waker: Waker) {
    *WAKE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(waker);
    cache();
}

/// The shared image cache, created on first use with the installed waker (or a no-op one).
fn cache() -> &'static ImageCache {
    CACHE.get_or_init(|| {
        let wake: Waker = Arc::new(|| {
            let waker = WAKE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
            if let Some(waker) = waker {
                waker();
            }
        });
        ImageCache::new(wake)
    })
}

/// Where a picture comes from.
#[derive(Debug, Clone)]
enum Source {
    /// `ref/<game>/renderer/public/images/<name>`.
    Local { game: GameId, name: String },
    /// An item's art on the CDN (`web.poecdn.com`).
    Cdn { game: GameId, url: String },
}

impl Source {
    /// The registry key: the mode, the game and the source, so no two ever share an entry.
    fn key(&self) -> String {
        let mode = match mode() {
            Mode::Live => "live",
            Mode::Replay => "replay",
        };
        match self {
            Source::Local { game, name } => format!("{mode}/{}/images/{name}", game.as_str()),
            Source::Cdn { game, url } => format!("{mode}/{}/cdn/{url}", game.as_str()),
        }
    }

    /// What a test (and the element tree) calls it: the reference's `/images/<name>` or the URL.
    fn label(&self) -> String {
        match self {
            Source::Local { name, .. } => format!("/images/{name}"),
            Source::Cdn { url, .. } => url.clone(),
        }
    }
}

/// `/images/<name>` from the reference's `public/images`, `size` points square.
pub fn local(game: GameId, name: &str, size: f32) -> El {
    draw(Source::Local { game, name: name.to_owned() }, (size, size))
}

/// [`local`] in a box that is not square (an item's `w-8 h-10`).
pub fn local_sized(game: GameId, name: &str, size: (f32, f32)) -> El {
    draw(Source::Local { game, name: name.to_owned() }, size)
}

/// An item's art at `url` (a `web.poecdn.com` image from the game data), `size` points.
pub fn cdn(game: GameId, url: &str, size: (f32, f32)) -> El {
    draw(Source::Cdn { game, url: url.to_owned() }, size)
}

/// A picture a widget was handed, as `ItemQuickPrice.vue` takes its `itemImg` prop: either
/// one of the reference's `/images` files or an item's CDN art.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picture {
    /// `/images/<name>`, shipped by the reference.
    Local(String),
    /// A `https://web.poecdn.com/...` image.
    Cdn(String),
}

impl Picture {
    /// `/images/<name>`.
    pub fn local(name: impl Into<String>) -> Self {
        Picture::Local(name.into())
    }

    /// An item's art on the CDN.
    pub fn cdn(url: impl Into<String>) -> Self {
        Picture::Cdn(url.into())
    }
}

/// [`Picture`] drawn in a `size` box.
pub fn picture(game: GameId, picture: &Picture, size: (f32, f32)) -> El {
    match picture {
        Picture::Local(name) => draw(Source::Local { game, name: name.clone() }, size),
        Picture::Cdn(url) => draw(Source::Cdn { game, url: url.clone() }, size),
    }
}

/// One picture: the decoded image when it is ready, the placeholder until then (and if it
/// never loads). The caller keys the element and adds margins, as it would with an `<img>`.
fn draw(source: Source, size: (f32, f32)) -> El {
    let (w, h) = size;
    match ready(&source) {
        Some(image) => el().size(w, h).pixels_fit(image.pixels(), SurfaceFit::Fit).role("img").label(source.label()),
        None => icon::image_placeholder(w).size(w, h),
    }
}

/// The decoded image for `source`, starting the load the first time it is asked for.
fn ready(source: &Source) -> Option<Image> {
    let key = source.key();
    {
        let mut registry = REGISTRY.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(image) = registry.images.get(&key) {
            // Still decoding: the cache's waker brings the next frame.
            return (image.is_ready() && image.error().is_none()).then(|| image.clone());
        }
        if registry.failed.contains(&key) || !registry.pending.insert(key.clone()) {
            return None;
        }
    }
    load(source, key);
    // Art that is read from disk (the reference's files, the harness's recordings) is waited
    // for, so the frame that first draws it already shows the picture: a `--snap` render and a
    // test frame must not race the decoder. A live CDN fetch is not waited for; the cache's
    // waker brings the next frame when it lands.
    let stored = REGISTRY.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).images.get(&source.key()).cloned();
    if let Some(image) = stored {
        let _ = image.wait();
        if let Some(error) = image.error() {
            eprintln!("wraex: image error for {}: {error}", source.label());
        }
        return (image.error().is_none()).then_some(image);
    }
    None
}

/// Load `source` and file the result under `key`, clearing its pending mark.
fn load(source: &Source, key: String) {
    match source {
        Source::Local { game, name } => {
            let path = image_path(*game, name);
            if !path.is_file() {
                fail(key, format!("{}/{name} is not in the reference's public/images", game.as_str()));
                return;
            }
            // `cache.file` reads and decodes on its worker; PNG art ignores the size, and
            // SVG art (none of the price check's own) rasterizes at 2× the drawn size.
            let image = cache().file(&path, 16.0, 2.0);
            store(key, image);
        }
        Source::Cdn { game, url } => {
            if !is_cdn_url(url) {
                fail(key, format!("{url} is not a web.poecdn.com image"));
                return;
            }
            if mode() == Mode::Replay {
                let path = recorded_path(*game, url);
                match std::fs::read(&path) {
                    Ok(bytes) => {
                        let image = cache().load(bytes);
                        store(key, image);
                    }
                    Err(error) => {
                        fail(key, format!("{}: {error} (run `npm run screens -- --record`)", path.display()));
                    }
                }
            } else {
                fetch(key, url.clone());
            }
        }
    }
}

/// Fetch a CDN image on a worker thread, then wake the window.
fn fetch(key: String, url: String) {
    let spawned = std::thread::Builder::new().name("wraex-image".to_owned()).spawn(move || {
        let result = wraex_platform::http::HttpTransport::default().get_bytes(&url);
        match result {
            Ok(bytes) => store(key, cache().load(bytes)),
            Err(error) => fail(key, error),
        }
        wake();
    });
    if let Err(error) = spawned {
        eprintln!("wraex: could not start the image worker: {error}");
    }
}

fn store(key: String, image: Image) {
    let mut registry = REGISTRY.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.pending.remove(&key);
    registry.images.insert(key, image);
}

fn fail(key: String, why: String) {
    eprintln!("wraex: no image: {why}");
    let mut registry = REGISTRY.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    registry.pending.remove(&key);
    registry.failed.insert(key);
}

fn wake() {
    // The cache's own waker is the app's; this is for a finished download before decoding.
    let waker = WAKE.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    if let Some(waker) = waker {
        waker();
    }
}

/// `ref/<game>/renderer/public/images/<name>` (see the per-game data modules).
fn image_path(game: GameId, name: &str) -> std::path::PathBuf {
    match game {
        GameId::Poe1 => wraex_poe1::data::image_path(name),
        GameId::Poe2 => wraex_poe2::data::image_path(name),
    }
}

/// The recording of a CDN image: `tools/golden/corpus/screens/<game>/images/<hash>-<name>`.
///
/// `tools/golden/screens/images.mjs` writes it, so both sides name files the same way.
pub fn recorded_path(game: GameId, url: &str) -> std::path::PathBuf {
    std::path::Path::new(crate::replay::CORPUS).join(game.as_str()).join("images").join(recorded_name(url))
}

/// `<first 8 hex of sha1(url)>-<the file's own name>`, the harness's naming.
pub fn recorded_name(url: &str) -> String {
    use sha1::{Digest, Sha1};

    let digest = Sha1::digest(url.as_bytes());
    let mut hash = String::with_capacity(8);
    for byte in &digest[..4] {
        hash.push_str(&format!("{byte:02x}"));
    }
    let file = url.rsplit('/').next().unwrap_or_default();
    let mut name = String::with_capacity(file.len());
    for c in file.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') {
            name.push(c);
        } else {
            name.push('_');
        }
    }
    format!("{hash}-{name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recordings_are_named_after_the_url_and_the_file() {
        let url = "https://web.poecdn.com/gen/image/WzI1LDE0LHsiZiI6IjJESXRlbXMvUmluZ3MvTGFrZVJpbmcxIn1d/40b81baee3/LakeRing1.png";
        assert_eq!(recorded_name(url), "1f462dd1-LakeRing1.png");
        assert!(recorded_path(GameId::Poe1, url).ends_with("poe1/images/1f462dd1-LakeRing1.png"));
        // The same file name on two paths stays two files.
        let other = url.replace("40b81baee3", "0123456789");
        assert_ne!(recorded_name(&other), recorded_name(url));
    }

    #[test]
    fn the_reference_ships_the_art_the_windows_draw() {
        for name in ["chaos.png", "divine.png", "stash.png", "dust.png", "influence-Shaper.png"] {
            assert!(image_path(GameId::Poe1, name).is_file(), "{name}");
        }
        for name in ["exa.png", "chaos.png", "divine.png", "annul.png", "404.png"] {
            assert!(image_path(GameId::Poe2, name).is_file(), "{name}");
        }
        assert!(image_path(GameId::Poe2, "augments/empty-socket.png").is_file());
    }

    #[test]
    fn a_local_image_loads_and_a_missing_one_fails() {
        set_mode(Mode::Replay);
        let image = ready(&Source::Local { game: GameId::Poe1, name: "chaos.png".to_owned() });
        let image = image.expect("the reference's chaos orb decodes");
        assert_eq!(image.size(), Some([47, 47]));
        let key = Source::Local { game: GameId::Poe1, name: "nope.png".to_owned() }.key();
        assert!(ready(&Source::Local { game: GameId::Poe1, name: "nope.png".to_owned() }).is_none());
        assert!(REGISTRY.lock().unwrap().failed.contains(&key), "a miss is remembered");
    }

    #[test]
    fn a_foreign_or_unrecorded_url_is_a_placeholder() {
        set_mode(Mode::Replay);
        let foreign = Source::Cdn { game: GameId::Poe1, url: "https://example.com/x.png".to_owned() };
        assert!(ready(&foreign).is_none());
        let unrecorded =
            Source::Cdn { game: GameId::Poe1, url: "https://web.poecdn.com/gen/image/nothing/abc/none.png".to_owned() };
        assert!(ready(&unrecorded).is_none());
    }
}
