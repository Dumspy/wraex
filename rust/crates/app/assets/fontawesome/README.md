# Font Awesome Free 6.7.1 (webfonts)

The icon font upstream draws its glyphs with. Upstream imports
`@fortawesome/fontawesome-free` from `node_modules`, which CI does not have, so the
two faces the widgets use are vendored here instead.

- Source: `@fortawesome/fontawesome-free@6.7.1`, the version `ref/apt/renderer/package-lock.json`
  pins (`ref/ee2` pins 6.7.2; its glyphs and codepoints are the same).
- `fa-solid-900.ttf` — `fas` / `fa-solid`, `@font-face` weight 900, family
  `Font Awesome 6 Free`.
- `fa-regular-400.ttf` — `far` / `fa-regular`, weight 400, the same family. Upstream
  uses it for `far fa-square` and `far fa-circle`.
- `LICENSE.txt` — Font Awesome Free's own licence file, kept as shipped. The fonts are
  SIL OFL 1.1; the icons are CC BY 4.0; the code is MIT. See
  <https://fontawesome.com/license/free>.

Codepoints are Font Awesome's own (`fa-chevron-up` = U+F077, ...); `metadata/icon-families.json`
in the package maps names to them. `rust/crates/app/src/theme.rs` compiles the faces in
(`FONTS`) and names the family (`FAMILY_FA`); `ui/icon.rs` and `widgets/icon.rs` draw them.

Do not edit these files. To bump, copy the same two faces and `LICENSE.txt` out of the
newer package and update this README.
