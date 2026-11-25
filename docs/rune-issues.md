# Rune issues

Problems found in Rune (pinned rev in `rust/Cargo.toml`) while building Wraex, with the
workaround in this repo. They are tracked here, not filed with Rune. When Rune is bumped,
re-check each entry: drop the workaround if the bug is gone.

## Partial frames leave old frames on a transparent Win32 window

- **Seen:** the first in-game Windows test (2026-11-25). The previous price-check card stayed
  beside the next one; copies of the widget menu stayed where it had been drawn; a closed card
  could stay on screen.
- **Cause:** Rune's Win32 windows present only the rectangles a frame changed (`Present1` dirty
  rectangles, `window/win32/damage.rs`). On the transparent overlay the pixels outside those
  rectangles kept earlier frames' contents.
- **Proof:** running with `RUNE_WHOLE_FRAMES=1` (Rune's own switch: every frame presented whole)
  made the ghosts go.
- **Workaround:** `wraex_app::main` sets `RUNE_WHOLE_FRAMES=1` for a live run on Windows before
  Rune starts, unless the user set it (`rust/crates/app/src/lib.rs`). The overlay draws a few
  cards, so whole frames cost little.

## Removed elements fade out by default

- **Seen:** the same test, before the entry above was understood.
- **Cause:** not a bug: every Rune element has a default exit transition (a 0.15 s fade) and
  layout moves spring, unless a commit is `Tx::instant`. Upstream's cards appear and close at
  once.
- **Workaround:** a live window's cards use `El::instant_changes()` (`crate::view`), so cards
  swap and close in the same frame; hover colours keep their springs.

## No window style to hide a window from the taskbar

- **Seen:** while porting the overlay (`skipTaskbar` upstream).
- **Cause:** `WindowOptions` has no taskbar option for an ordinary window.
- **Workaround:** `wraex_platform::window::configure_overlay` sets `WS_EX_TOOLWINDOW` itself and
  re-shows a window that was already visible, as the taskbar keeps the button of a window whose
  style changed while shown. (The taskbar button seen in the test turned out to be the console
  window; the release build is now a GUI program.)

## No text shadow

- **Seen:** porting the stopwatch (`.timer`'s `text-shadow: 0 1px 3px`).
- **Cause:** Rune has a box shadow (`El::shadow`) but no text shadow.
- **Workaround:** none; the stopwatch is drawn without it (accepted).
