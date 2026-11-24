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

/// `.table-stripped tbody tr:nth-child(odd)` in `web/App.vue`: #353f52, between
/// `gray-800` and `gray-700`.
pub fn table_stripe() -> Color {
    c("#353f52")
}

/// Tailwind's `indigo` ramp (an enchant tag).
pub fn indigo(shade: u16) -> Color {
    c(match shade {
        200 => "#c3dafe",
        _ => panic!("no `indigo.{shade}` in upstream's tailwind.config.js"),
    })
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

/// The sans and mono slots. Named after the face files themselves: `ref/apt`'s
/// `@font-face` rules call the families `FontinSmallCaps` etc., but a TTF's own name
/// table (name ID 16/1) is what a font collection registers, and that is
/// `Fontin SmallCaps`, `Fontin`, `Exo 2` and `FrizQuadrataITC`.
///
/// Upstream has no monospaced face. Rune's styled text has no per-span family, only a `mono`
/// flag selecting this slot, so the slot holds Exo 2: `ItemModifierText.vue` draws rolled
/// numbers in `font-exo2 font-semibold`, and a span can reach that face through `mono`.
pub const FAMILIES: (&str, &str) = ("Fontin SmallCaps, FrizQuadrataITC", "Exo 2");

/// The fonts `ref/apt/renderer/src/assets/font` ships, compiled in.
///
/// They live in the `ref/apt` submodule (CI checks it out), not in `node_modules`.
pub const FONTS: &[&[u8]] = &[
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-SmallCaps-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-Regular-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-Bold-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Fontin-Italic-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/FrizQuadrataITC-Hinted.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Exo2-Regular.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Exo2-Medium.ttf") as &[u8],
    include_bytes!("../../../../ref/apt/renderer/src/assets/font/Exo2-SemiBold.ttf") as &[u8],
];
