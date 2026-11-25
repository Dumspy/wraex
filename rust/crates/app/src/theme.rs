//! Upstream's look: Tailwind's palette and type scale, plus the fonts Awakened PoE
//! Trade ships.
//!
//! Every widget in `ref/apt/renderer/src/web` is styled with the Tailwind classes and
//! the palette in `tailwind.config.js`. This module holds those values as Rune
//! colours, sizes and [`TextStyle`]s so a port can copy a class (`p-4`, `text-sm`,
//! `bg-gray-800`) instead of inventing numbers. Nothing here is PoE-specific: the
//! PoE2 widgets use the same palette (EE2 keeps upstream's `tailwind.config.js`).

use rune::Color;
use rune::color::hex;
use rune::tok::{Family, TextStyle};

/// The root font size. Upstream sets `html { font-size: <AppConfig().fontSize>px }`
/// (`web/Config.ts`), and a fresh install's `fontSize` is 16.
pub const REM: f32 = 16.0;

/// Tailwind's `p-4`, `w-40`, ... in points, at upstream's root font size.
pub fn rem(n: f32) -> f32 {
    n * REM
}

/// Tailwind's `rounded` (0.25 rem).
pub const ROUNDED: f32 = 0.25 * REM;

fn c(value: &str) -> Color {
    hex(value).expect("a `#rrggbb` colour")
}

/// Tailwind's `gray` ramp.
pub fn gray(shade: u16) -> Color {
    c(match shade {
        100 => "#f7fafc",
        200 => "#edf2f7",
        300 => "#e2e8f0",
        400 => "#cbd5e0",
        500 => "#a0aec0",
        600 => "#718096",
        700 => "#4a5568",
        800 => "#2d3748",
        900 => "#1a202c",
        _ => panic!("no `gray.{shade}` in upstream's tailwind.config.js"),
    })
}

/// Tailwind's `red` ramp; the shades the ported widgets use so far.
pub fn red(shade: u16) -> Color {
    c(match shade {
        100 => "#fff5f5",
        400 => "#fc8181",
        500 => "#f56565",
        600 => "#e53e3e",
        700 => "#c53030",
        800 => "#9b2c2c",
        _ => panic!("no `red.{shade}` in upstream's tailwind.config.js"),
    })
}

/// Tailwind's `yellow` ramp (`filters` tier badges, `variant` tags).
pub fn yellow(shade: u16) -> Color {
    c(match shade {
        100 => "#fffff0",
        500 => "#ecc94b",
        700 => "#b7791f",
        _ => panic!("no `yellow.{shade}` in upstream's tailwind.config.js"),
    })
}

/// Tailwind's `orange` ramp (a fractured tag, an afk seller, a warned online filter).
pub fn orange(shade: u16) -> Color {
    c(match shade {
        300 => "#fbd38d",
        400 => "#f6ad55",
        500 => "#ed8936",
        600 => "#dd6b20",
        700 => "#c05621",
        _ => panic!("no `orange.{shade}` in upstream's tailwind.config.js"),
    })
}

/// Tailwind's `blue` ramp (crafted and synthesised tags, a listing's quality).
pub fn blue(shade: u16) -> Color {
    c(match shade {
        100 => "#ebf8ff",
        400 => "#4299e1",
        600 => "#3182ce",
        _ => panic!("no `blue.{shade}` in upstream's tailwind.config.js"),
    })
}

/// Tailwind's `green` ramp (`PriceTrend.vue`'s up chevrons, EE2's desecrated tag).
pub fn green(shade: u16) -> Color {
    c(match shade {
        100 => "#f0fff4",
        500 => "#48bb78",
        600 => "#38a169",
        700 => "#2f855a",
        900 => "#22543d",
        _ => panic!("no `green.{shade}` in upstream's tailwind.config.js"),
    })
}

/// Tailwind's `slate` ramp (EE2's `settings.general` experimental-features box).
pub fn slate(shade: u16) -> Color {
    c(match shade {
        800 => "#1e293b",
        _ => panic!("no `slate.{shade}` in EE2's tailwind.config.js"),
    })
}

/// `.table-stripped tbody tr:nth-child(odd)` in `web/App.vue`: #353f52, between
/// `gray-800` and `gray-700`.
pub fn table_stripe() -> Color {
    c("#353f52")
}

/// What an overlay widget is drawn over: `OverlayWindow.vue` paints `AppConfig()`'s
/// `overlayBackground` (`rgba(129, 139, 149, 0.15)` on a fresh install) over the page's dark
/// canvas (`<meta name="color-scheme" content="dark">`), and the screenshot harness's page
/// flattens that to this. A widget scene fills its window with it, so a translucent widget (the
/// delve grid, the stopwatch) composites as the reference's screenshot does.
pub fn overlay_background() -> Color {
    c("#232426")
}

/// Tailwind's `indigo` ramp (an enchant tag).
pub fn indigo(shade: u16) -> Color {
    c(match shade {
        200 => "#c3dafe",
        _ => panic!("no `indigo.{shade}` in upstream's tailwind.config.js"),
    })
}

/// Tailwind's `pink` ramp (`account-status.online` in EE2's `TradeListing.vue`).
pub fn pink(shade: u16) -> Color {
    c(match shade {
        400 => "#f687b3",
        _ => panic!("no `pink.{shade}` in EE2's tailwind.config.js"),
    })
}

/// Tailwind's `purple` ramp (an enchant tag in EE2's `FilterModifier.vue`).
pub fn purple(shade: u16) -> Color {
    c(match shade {
        100 => "#faf5ff",
        600 => "#805ad5",
        _ => panic!("no `purple.{shade}` in EE2's tailwind.config.js"),
    })
}

/// Tailwind's `rose` ramp (a mutated tag in EE2's `FilterModifier.vue`).
pub fn rose(shade: u16) -> Color {
    c(match shade {
        900 => "#881337",
        _ => panic!("no `rose.{shade}` in EE2's tailwind.config.js"),
    })
}

/// EE2's `colors.fire` (`#c80000`): the elemental-dps option button. APT's
/// `tailwind.config.js` has no elemental colours, so these only appear in a PoE2 window.
pub fn fire() -> Color {
    c("#c80000")
}

/// EE2's `colors.cold` (`#5aadff`).
pub fn cold() -> Color {
    c("#5aadff")
}

/// EE2's `colors.lightning` (`#ffff00`).
pub fn lightning() -> Color {
    c("#ffff00")
}

/// Tailwind's `text-*` sizes: font size and line height, both in points.
pub mod text {
    use rune::tok::TextStyle;

    /// `text-xs` (0.75 rem / 1 rem)
    pub const XS: TextStyle = TextStyle::new(12.0, 400.0).lh(16.0);
    /// `text-sm` (0.875 rem / 1.25 rem), the app's base size (`App.vue`).
    pub const SM: TextStyle = TextStyle::new(14.0, 400.0).lh(20.0);
    /// `text-base` (1 rem / 1.5 rem)
    pub const BASE: TextStyle = TextStyle::new(16.0, 400.0).lh(24.0);
    /// `text-lg` (1.125 rem / 1.75 rem)
    pub const LG: TextStyle = TextStyle::new(18.0, 400.0).lh(28.0);
    /// `text-xl` (1.25 rem / 1.75 rem)
    pub const XL: TextStyle = TextStyle::new(20.0, 400.0).lh(28.0);
    /// `text-2xl` (1.5 rem / 2 rem)
    pub const XXL: TextStyle = TextStyle::new(24.0, 400.0).lh(32.0);
}

/// `leading-none`: the line box is exactly the font size.
pub fn leading_none(style: TextStyle) -> TextStyle {
    style.lh(style.size)
}

/// `font-poe-sc` (`assets/font.css`): Fontin SmallCaps, with FrizQuadrataITC behind
/// it for the glyphs Fontin lacks (upstream lists it for Cyrillic).
///
/// This is the family behind the sans slot ([`FAMILIES`]), so it is what plain
/// `text()` draws in, as `#app { text-sm font-poe-sc }` draws the whole app.
pub const FAMILY_POE_SC: Family = Family::Named("Fontin SmallCaps, FrizQuadrataITC");
/// `font-poe`: Fontin, then FrizQuadrataITC.
pub const FAMILY_POE: Family = Family::Named("Fontin, FrizQuadrataITC");
/// `font-exo2`: Exo 2.
pub const FAMILY_EXO2: Family = Family::Named("Exo 2");
/// Font Awesome's family (`@fortawesome/fontawesome-free`): `fas`/`far` in upstream's
/// templates, weight 900 for solid and 400 for regular, both under the same family name.
pub const FAMILY_FA: Family = Family::Named("Font Awesome 6 Free");
/// `font-mono`'s face (`WidgetTimer.vue`): Tailwind's default stack is
/// `ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New",
/// monospace`, the platform's own monospaced face. Upstream bundles no monospaced font, so the
/// port names the same families in the same order and Rune loads the first one installed: Menlo
/// (what the reference's Chromium screenshots draw) on macOS, Consolas on Windows, Liberation
/// Mono or DejaVu Sans Mono on Linux, the generic `monospace`'s usual faces. A machine with none
/// of them falls back to the sans slot.
pub const FAMILY_SYSTEM_MONO: Family =
    Family::Named("Menlo, Monaco, Consolas, Liberation Mono, Courier New, DejaVu Sans Mono");

/// The sans and mono slots. Named after the face files themselves: `ref/apt`'s
/// `@font-face` rules call the families `FontinSmallCaps` etc., but a TTF's own name
/// table (name ID 16/1) is what a font collection registers, and that is
/// `Fontin SmallCaps`, `Fontin`, `Exo 2` and `FrizQuadrataITC`.
///
/// Upstream has no monospaced face. Rune's styled text has no per-span family, only a `mono`
/// flag selecting this slot, so the slot holds Exo 2: `ItemModifierText.vue` draws rolled
/// numbers in `font-exo2 font-semibold`, and a span can reach that face through `mono`.
pub const FAMILIES: (&str, &str) = ("Fontin SmallCaps, FrizQuadrataITC", "Exo 2");

/// The fonts the widgets draw with, compiled in: the faces `ref/apt/renderer/src/assets/font`
/// ships (they live in the `ref/apt` submodule, which CI checks out, not in `node_modules`),
/// then Font Awesome's solid and regular faces.
///
/// Font Awesome is upstream's `node_modules` dependency, so the two faces the templates use are
/// vendored in `assets/fontawesome/` (`fas` = weight 900, `far` = 400, one family name).
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-SmallCaps-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-Regular-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-Bold-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-Italic-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/FrizQuadrataITC-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Exo2-Regular.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Exo2-Medium.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Exo2-SemiBold.ttf") as &[u8],
    include_bytes!("../assets/fontawesome/fa-solid-900.ttf") as &[u8],
    include_bytes!("../assets/fontawesome/fa-regular-400.ttf") as &[u8],
];
