//! Upstream's images: the reference's `renderer/public/images` and the game's item art.
//!
//! The reference windows draw pictures in three shapes, and this module gives each one a
//! Rune element:
//!
//! - **Local art** (`/images/chaos.png`, `stash.png`, `dust.png`, the influence badges, the
//!   oils' and catalysts' art) ships in the reference's `renderer/public/images` and is read
//!   from the assets root (`wraex_core::assets`; the reference checkout in development,
//!   `assets/<game>/images` in a release) through [`wraex_poe1::data::image_path`],
//!   [`wraex_poe2::data::image_path`].
//! - **Item art** is a `https://web.poecdn.com/...` URL from the game data (an item's `icon`)
//!   that the reference's browser fetches itself, outside `/proxy`. A window fetches it once
//!   per URL on a worker thread ([`wraex_platform::http::HttpTransport::get_bytes`]) and keeps
//!   the decoded image; a headless run reads the recording the screenshot harness made
//!   (`tools/golden/corpus/screens/<game>/images/`), so a scene never touches the network.
//! - **Map screenshots** (`MapCheck.vue`) are `https://i.imgur.com/...` URLs from the game
//!   data's `area.screenshot`/`map.screenshot`, loaded the same way, and drawn filling a
//!   21:9 box (`object-fit: cover`).
//! - **Greyscale art** is local art drawn through CSS `filter: grayscale(1)` (`TradeListing.vue`'s
//!   stash icon, `TradeBulk.vue`'s currency icons). Rune has no colour filter, so [`local_grey`]
//!   decodes the picture, applies the filter's colour matrix to its pixels and caches the result
//!   beside the colour copy.
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
use std::sync::{Arc, LazyLock, Mutex, OnceLock, PoisonError};

use rune::{El, Image, ImageCache, SurfaceFit, Waker, el};
use wraex_core::GameId;
use wraex_core::images::{is_cdn_url, is_map_screenshot_url};
use wraex_core::num::{f32_to_u8, f32_to_u32, f64_to_f32};

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

/// The mode a window was built with.
///
/// [`crate::trade::Mode::Replay`] reads the screenshot harness's recordings,
/// [`crate::trade::Mode::Live`] fetches from the CDN. The first caller wins; every later one is
/// the same window's re-render.
pub fn set_mode(mode: Mode) {
    let value = match mode {
        Mode::Live => MODE_LIVE,
        Mode::Replay => MODE_REPLAY,
    };
    // The first caller wins; every later one is the same window's re-render, so a value already
    // set is the expected case and needs nothing done with it.
    if MODE.compare_exchange(MODE_UNSET, value, Ordering::Relaxed, Ordering::Relaxed).is_err() {
        // Nothing to do: an earlier window set the mode.
    }
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
    *WAKE.lock().unwrap_or_else(PoisonError::into_inner) = Some(waker);
    cache();
}

/// The shared image cache, created on first use with the installed waker (or a no-op one).
fn cache() -> &'static ImageCache {
    CACHE.get_or_init(|| {
        let wake: Waker = Arc::new(|| {
            let waker = WAKE.lock().unwrap_or_else(PoisonError::into_inner).clone();
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
    /// A map's screenshot on imgur (`i.imgur.com`), from the game data's `area.screenshot`.
    Screenshot { game: GameId, url: String },
}

impl Source {
    /// The registry key: the mode, the game and the source, so no two ever share an entry.
    fn key(&self) -> String {
        let mode = match mode() {
            Mode::Live => "live",
            Mode::Replay => "replay",
        };
        match self {
            Self::Local { game, name } => format!("{mode}/{}/images/{name}", game.as_str()),
            Self::Cdn { game, url } => format!("{mode}/{}/cdn/{url}", game.as_str()),
            Self::Screenshot { game, url } => format!("{mode}/{}/screenshot/{url}", game.as_str()),
        }
    }

    /// What a test (and the element tree) calls it: the reference's `/images/<name>` or the URL.
    fn label(&self) -> String {
        match self {
            Self::Local { name, .. } => format!("/images/{name}"),
            Self::Cdn { url, .. } | Self::Screenshot { url, .. } => url.clone(),
        }
    }

    /// Whether `url` is the kind of remote image this source is.
    fn accepts(&self, url: &str) -> bool {
        match self {
            Self::Cdn { .. } => is_cdn_url(url),
            Self::Screenshot { .. } => is_map_screenshot_url(url),
            Self::Local { .. } => false,
        }
    }
}

/// `/images/<name>` from the reference's `public/images`, `size` points square.
pub fn local(game: GameId, name: &str, size: f32) -> El {
    draw(&Source::Local { game, name: name.to_owned() }, (size, size))
}

/// [`local`] in a box that is not square (an item's `w-8 h-10`).
pub fn local_sized(game: GameId, name: &str, size: (f32, f32)) -> El {
    draw(&Source::Local { game, name: name.to_owned() }, size)
}

/// `/images/<name>` through CSS `filter: grayscale(1)`.
///
/// `TradeListing.vue`'s stash icon and `TradeBulk.vue`'s currency icons. The picture's own
/// opacity, if the reference adds one (`grayscale(1) opacity(0.5)`), is [`El::opacity`] on the
/// returned element.
pub fn local_grey(game: GameId, name: &str, size: f32) -> El {
    let source = Source::Local { game, name: name.to_owned() };
    ready_grey(&source).map_or_else(
        || icon::image_placeholder(size).size(size, size),
        |image| el().size(size, size).pixels_fit(image.pixels(), SurfaceFit::Fit).role("img").label(source.label()),
    )
}

/// `/images/<name>` cropped to fill `size`, as CSS `object-fit: cover` does:
/// `WidgetImageStrip.vue`'s `FullscreenImage` in a card.
///
/// The picture is scaled so it covers the box and centred; the overflow is left to the caller's
/// clipped box (`El::clip_rounded`), so the returned element may be larger than `size` and is out
/// of flow at the offset it needs. A picture that has not loaded is a grey placeholder filling
/// the box, so the strip never jumps.
pub fn local_cover(game: GameId, name: &str, size: (f32, f32)) -> El {
    let (w, h) = size;
    let source = Source::Local { game, name: name.to_owned() };
    let Some(image) = ready(&source) else {
        return el()
            .absolute(0.0, 0.0)
            .size(w, h)
            .radius(crate::theme::ROUNDED)
            .fill(crate::theme::gray(700))
            .a11y_hidden();
    };
    let [iw, ih] = image.size().unwrap_or_else(|| [f32_to_u32(w.max(1.0)), f32_to_u32(h.max(1.0))]);
    let (iw, ih) = (f64_to_f32(f64::from(iw)), f64_to_f32(f64::from(ih)));
    let scale = (w / iw).max(h / ih);
    let (dw, dh) = (iw * scale, ih * scale);
    el().absolute((w - dw) / 2.0, (h - dh) / 2.0)
        .size(dw, dh)
        .pixels_fit(image.pixels(), SurfaceFit::Fit)
        .role("img")
        .label(source.label())
}

/// An item's art at `url` (a `web.poecdn.com` image from the game data), `size` points.
pub fn cdn(game: GameId, url: &str, size: (f32, f32)) -> El {
    draw(&Source::Cdn { game, url: url.to_owned() }, size)
}

/// A map's screenshot at `url` (an `i.imgur.com` image from the game data's
/// `area.screenshot`), filling `size` and cropping the overflow as `object-fit: cover`
/// does (`MapCheck.vue`'s `FullscreenImage` in a 21:9 box).
pub fn map_screenshot(game: GameId, url: &str, size: (f32, f32)) -> El {
    draw_fit(&Source::Screenshot { game, url: url.to_owned() }, size, SurfaceFit::Fill)
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
        Self::Local(name.into())
    }

    /// An item's art on the CDN.
    pub fn cdn(url: impl Into<String>) -> Self {
        Self::Cdn(url.into())
    }
}

/// [`Picture`] drawn in a `size` box.
pub fn picture(game: GameId, picture: &Picture, size: (f32, f32)) -> El {
    match picture {
        Picture::Local(name) => draw(&Source::Local { game, name: name.clone() }, size),
        Picture::Cdn(url) => draw(&Source::Cdn { game, url: url.clone() }, size),
    }
}

/// One picture: the decoded image when it is ready, the placeholder until then (and if it
/// never loads). The caller keys the element and adds margins, as it would with an `<img>`.
fn draw(source: &Source, size: (f32, f32)) -> El {
    draw_fit(source, size, SurfaceFit::Fit)
}

fn draw_fit(source: &Source, size: (f32, f32), fit: SurfaceFit) -> El {
    let (w, h) = size;
    ready(source).map_or_else(
        || icon::image_placeholder(w).size(w, h),
        |image| el().size(w, h).pixels_fit(image.pixels(), fit).role("img").label(source.label()),
    )
}

/// The decoded image for `source`, starting the load the first time it is asked for.
fn ready(source: &Source) -> Option<Image> {
    let key = source.key();
    let stored = {
        let mut registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(image) = registry.images.get(&key).cloned() {
            Some(image)
        } else if registry.failed.contains(&key) || !registry.pending.insert(key.clone()) {
            return None;
        } else {
            None
        }
    };
    if let Some(image) = stored {
        // A picture another frame or another state started: still decoding here, and art read
        // from disk (the reference's files, the harness's recordings) is waited for, so the
        // frame that first draws it already shows the picture. A live CDN fetch is decoded on
        // its worker too; the cache's waker brings the next frame if this one has to skip it.
        return settle(image);
    }
    load(source, key);
    let stored = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner).images.get(&source.key()).cloned();
    stored.and_then(settle)
}

/// An image that has finished decoding, waiting for it when it is still on its worker.
fn settle(image: Image) -> Option<Image> {
    if image.is_ready() {
        return image.error().is_none().then_some(image);
    }
    match image.wait() {
        Ok(_) => Some(image),
        Err(error) => {
            wraex_platform::log!("wraex: image error: {error}");
            None
        }
    }
}

/// [`ready`] of `source`'s greyscale copy.
///
/// The conversion is CPU work on the reference's own art (a 1.25-rem icon), so it happens in the
/// frame that first asks for it and the result is filed in the registry like any other picture:
/// every later frame — and every later listing row sharing the icon — draws the same handle.
fn ready_grey(source: &Source) -> Option<Image> {
    let key = format!("{}#greyscale", source.key());
    let stored = {
        let mut registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(image) = registry.images.get(&key).cloned() {
            Some(image)
        } else if registry.failed.contains(&key) || !registry.pending.insert(key.clone()) {
            return None;
        } else {
            None
        }
    };
    if let Some(image) = stored {
        return settle(image);
    }
    match greyscale(source) {
        Ok(image) => {
            store(key, image.clone());
            settle(image)
        }
        Err(why) => {
            fail(key, &why);
            None
        }
    }
}

/// `source` decoded and put through CSS `filter: grayscale(1)`'s colour matrix.
///
/// The filter shorthand's operations run in sRGB, and `grayscale(amount)` replaces every channel
/// with `0.2126 R + 0.7152 G + 0.0722 B` at `amount = 1` (`Filter Effects`' luminance weights),
/// leaving alpha alone — so the conversion is per pixel on the 8-bit sRGB values.
fn greyscale(source: &Source) -> Result<Image, String> {
    let Source::Local { game, name } = source else {
        return Err(format!("{} is not the reference's own art", source.label()));
    };
    let path = image_path(*game, name);
    let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
    // The reference's greyscale art is PNG (`stash.png`, the currency icons), so the PNG decoder
    // alone is enough — the same one `ImageCache` would use, without linking every codec.
    let decoded = rune::image::png_rgba8(&bytes).map_err(|error| format!("{}: {error}", path.display()))?;
    let (width, height) = decoded.dimensions();
    let mut rgba = decoded.into_raw();
    to_greyscale(&mut rgba);
    Ok(cache().rgba(width, height, rgba))
}

/// `filter: grayscale(1)` over straight-alpha sRGB pixels, in place.
#[expect(
    clippy::suboptimal_flops,
    reason = "the weights are the filter matrix's separate multiply and add; `mul_add` rounds once instead of twice"
)]
fn to_greyscale(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let luma = 0.2126 * f32::from(pixel[0]) + 0.7152 * f32::from(pixel[1]) + 0.0722 * f32::from(pixel[2]);
        let luma = f32_to_u8(luma.round().clamp(0.0, 255.0));
        pixel[0] = luma;
        pixel[1] = luma;
        pixel[2] = luma;
    }
}

/// Load `source` and file the result under `key`, clearing its pending mark.
fn load(source: &Source, key: String) {
    match source {
        Source::Local { game, name } => {
            let path = image_path(*game, name);
            if !path.is_file() {
                fail(
                    key,
                    &format!("{} does not exist (the assets' {}/images directory)", path.display(), game.as_str()),
                );
                return;
            }
            // `cache.file` reads and decodes on its worker; PNG art ignores the size, and
            // SVG art (none of the price check's own) rasterizes at 2× the drawn size.
            let image = cache().file(&path, 16.0, 2.0);
            store(key, image);
        }
        Source::Cdn { .. } | Source::Screenshot { .. } => remote(source, key),
    }
}

/// A remote picture: the screenshot harness's recording in replay, the host in a live window.
fn remote(source: &Source, key: String) {
    let (Source::Cdn { game, url } | Source::Screenshot { game, url }) = source else { return };
    if !source.accepts(url) {
        fail(key, &format!("{url} is not an image the app loads"));
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
                fail(key, &format!("{}: {error} (run `npm run screens -- --record`)", path.display()));
            }
        }
    } else {
        fetch(key, url.clone());
    }
}

/// Fetch a CDN image on a worker thread, then wake the window.
fn fetch(key: String, url: String) {
    let spawned = std::thread::Builder::new().name("wraex-image".to_owned()).spawn(move || {
        let result = wraex_platform::http::HttpTransport::default().get_bytes(&url);
        match result {
            Ok(bytes) => store(key, cache().load(bytes)),
            Err(error) => fail(key, &error),
        }
        wake();
    });
    if let Err(error) = spawned {
        wraex_platform::log!("wraex: could not start the image worker: {error}");
    }
}

fn store(key: String, image: Image) {
    let mut registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    registry.pending.remove(&key);
    registry.images.insert(key, image);
}

fn fail(key: String, why: &str) {
    wraex_platform::log!("wraex: no image: {why}");
    let mut registry = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
    registry.pending.remove(&key);
    registry.failed.insert(key);
}

fn wake() {
    // The cache's own waker is the app's; this is for a finished download before decoding.
    let waker = WAKE.lock().unwrap_or_else(PoisonError::into_inner).clone();
    if let Some(waker) = waker {
        waker();
    }
}

/// `assets/<game>/images/<name>` (or the reference checkout in development): see the
/// per-game data modules.
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
    // The first eight hex digits: the digest's first four bytes, big-endian.
    let head = digest.get(..4).unwrap_or_default();
    let hash = format!("{:08x}", u32::from_be_bytes(head.try_into().unwrap_or_default()));
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

    /// `filter: grayscale(1)`: the Filter Effects matrix's luminance weights over sRGB, alpha
    /// untouched (`0.2126 · 255` is 54.2 for pure red, `0.7152 · 255` is 182.4 for pure green).
    #[test]
    fn greyscale_keeps_luminance_and_alpha() {
        let mut rgba = [255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 255, 255, 255, 255];
        to_greyscale(&mut rgba);
        assert_eq!(rgba, [54, 54, 54, 255, 182, 182, 182, 128, 18, 18, 18, 0, 255, 255, 255, 255]);
    }

    /// The greyscale copy is a picture of its own: it decodes from the same file, is filed under
    /// its own key (so the colour copy still draws in colour) and is remembered like one.
    #[test]
    fn a_greyscale_copy_is_cached_beside_the_colour_one() {
        set_mode(Mode::Replay);
        let source = Source::Local { game: GameId::Poe1, name: "stash.png".to_owned() };
        let grey = ready_grey(&source).expect("the stash icon has a greyscale copy");
        assert_eq!(grey.size(), Some([48, 48]));
        assert!(ready(&source).is_some(), "the colour copy still loads");
        let keys = REGISTRY.lock().unwrap().images.keys().cloned().collect::<Vec<_>>();
        assert!(keys.contains(&format!("{}#greyscale", source.key())), "{keys:?}");
        assert_eq!(grey.pixels().size(), (48, 48), "the pixels follow the decoded size");
        // Asking again hands back the same picture, not a second conversion.
        let again = ready_grey(&source).expect("a cached greyscale copy");
        assert_eq!(again.id(), grey.id());
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

    /// A map's screenshot is an imgur recording, drawn into a 21:9 box.
    #[test]
    fn a_map_screenshot_reads_its_recording() {
        set_mode(Mode::Replay);
        let url = "https://i.imgur.com/qZNQGdD.jpg";
        let source = Source::Screenshot { game: GameId::Poe1, url: url.to_owned() };
        assert!(source.accepts(url));
        let image = ready(&source).expect("the Beach screenshot is recorded");
        assert_eq!(source.label(), url);
        assert!(image.size().is_some());
        // Outside the recording is still a miss, not a network request.
        let miss = Source::Screenshot { game: GameId::Poe1, url: "https://i.imgur.com/nope.jpg".to_owned() };
        assert!(ready(&miss).is_none());
    }
}
