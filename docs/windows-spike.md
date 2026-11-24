# Windows overlay spike — manual checklist

PLAN.md phase 2 is a go/no-go on the platform: the game's window is found and followed, the
widget window is a real overlay over it, hotkeys only exist while the game is focused, and the
focus comes back after a widget was used. Everything else (parsers, filters, prices) already
works; this file is only about the OS.

What is implemented where:

| Piece | Code |
|---|---|
| Finding the game's window by title (`windowTitle`), following focus/move/resize/destroy | `rust/crates/platform/src/window/win32.rs` (`SetWinEventHook` on its own thread) |
| The host's reaction to it (focus changes, bounds, destruction) | `rust/crates/platform/src/window/mod.rs`, `overlay.rs` |
| The overlay window (transparent, on top, click-through, covering the game's client area) | `rust/crates/app/src/shell/mod.rs` (`Panel`, `OverlayPanel`, `sync_window`) |
| Hotkeys registered only while the game is focused | `rust/crates/app/src/shell/mod.rs` (`sync_registration`) |
| Key synthesis allowed by default there, opt-in elsewhere | `rust/crates/app/src/shell/keys.rs` |
| Focus hand-back (`SetForegroundWindow` + the foreground-lock workaround) | `rust/crates/platform/src/window/win32.rs` (`focus_window`) |

## Before the first run

1. Start Path of Exile (or Path of Exile 2) **in windowed or borderless-windowed mode** once,
   so the game has written its config; fullscreen is the last step, not the first.
2. Build and run the app from a terminal, and keep the log visible:

   ```powershell
   cd rust
   cargo run -p wraex-app -- --game poe1        # or: --game poe2
   ```

   The app writes everything to stderr. In a second terminal:

   ```powershell
   cargo run -p wraex-app -- --game poe1 2> wraex.log
   Get-Content wraex.log -Wait
   ```

   A packaged build is the same: `wraex.exe --game poe1 2> wraex.log`.

3. What the log says on a healthy start (a few lines each):

   - `wraex: following the "Path of Exile" window` — the hook started.
   - `wraex: following window "Path of Exile" of class "…" (access: yes)` — the window was
     found (on the game's next title change, or immediately if the game was already in
     front). If this line never appears, either the game is not running, or its title differs
     from the configured `windowTitle`: fix it in
     `%APPDATA%\Wraex\Path of Exile\config.json` (PoE2:
     `%APPDATA%\Wraex\Path of Exile 2\config.json`) and restart the app.
   - `(access: no, the game may run elevated)` — the game runs as administrator and Wraex does
     not. The overlay cannot reach it; restart Wraex as administrator (upstream shows the same
     error box). Expected log line: `wraex: the game is running with administrator rights: …`.
   - `wraex: 1 hotkeys registered: Ctrl + D` — hotkeys registered because the game has the
     focus. They are **unregistered** as soon as the game loses it (that is the point).

   The class name in that line is logged so a class can be pinned in
   `WindowTarget::with_class` later; upstream matches the title alone, and so does Wraex by
   default.

## Checklist

Work down the list; each item is one phase-2 exit criterion. Keep an eye on the log while
doing each step.

### 1. The hotkey copies an item and the panel appears at the cursor

1. Hover an item in the game's inventory or on the ground so its tooltip is open.
2. Press the price-check hotkey (default `Ctrl + D`; recorded in the settings window).
3. **Look for**: the panel appears with the item's name and prices, starting at the cursor
   (12 px below and right) and inside the game's window. No title bar, no taskbar button, no
   Alt+Tab entry for it.
4. **Look for** in the log: `wraex: focus game=true overlay=false hotkey=false` (the app saw
   the game when you started playing), then, as the panel takes the mouse,
   `wraex: focus game=false overlay=true hotkey=false`.

If pressing the hotkey does nothing: the log must still say `N hotkeys registered` from when
the game came to the front. If it does not, the game is not focused from the hook's point of
view — click into the game once. The window title is case- and space-exact (`Path of Exile`,
not `Path of exile`).

### 2. The panel is clickable, and only the panel is

1. Move the mouse onto the panel. **Look for**: it has the mouse already (hover states on the
   filters and listings work), and clicks inside it do not reach the game.
2. Scroll inside the listings. **Look for**: the panel scrolls, the game's camera/skills do
   not react.
3. Click a filter row. **Look for**: the row answers, the game does nothing.

### 3. The focus goes back to the game

1. With the panel open and hovered, press `Escape` (or `Ctrl + W`).
2. **Look for**: the panel stops taking clicks (it is still drawn, as upstream leaves it) and
   the game is in front again — moving the character works immediately, without clicking.
3. **Look for** in the log: `wraex: focus game=true overlay=false hotkey=false`, then
   `wraex: N hotkeys registered: …` (the hotkeys come back with the game's focus).

The overlay key (`Shift + Space` by default) toggles the same two states from inside the
overlay and is marked `hotkey=true` in that log line.

### 4. Borderless windowed, resizing and moving

1. Play in borderless-windowed (the normal way to play with an overlay) and repeat step 1.
2. While the panel is open, drag the game's window to another monitor and back. **Look for**:
   the panel follows the game's client area (it is placed over the game, so it moves with it),
   and the next widget appears at the cursor's new position.
3. Resize the game window (odd sizes included: 1280×720, 1920×1080, 2560×1440). **Look for**:
   the panel stays inside the game and is not cut off; the widget is clamped to the window.
4. Switch the game to exclusive fullscreen and back. **Look for**: the panel still lands on
   the game (the client area is the whole screen) and the hotkey keeps copying items.
5. **Look for** on a monitor whose Windows scale is not 100 % (e.g. 150 %): the panel is the
   same physical size and lands at the cursor, not offset by the scale.

### 5. Leaving the game

1. Alt+Tab to another application. **Look for**: the panel disappears with the game, and the
   price-check hotkey does **nothing** there (press it in Notepad: no panel, and no `Ctrl + D`
   typed into Notepad either). This is upstream's rule: the hotkeys exist only while the game
   has the focus.
2. Alt+Tab back. **Look for**: the panel comes back where it was, and the hotkey works again.

### 6. The game closes and comes back

1. Quit the game. **Look for**: `wraex: focus game=false …`, the panel hides, Wraex stays in
   the tray.
2. Start the game again. **Look for**: `wraex: following window "Path of Exile" … (access:
   yes)` and, once it is in front, `wraex: focus game=true …`; the hotkey works again without
   restarting Wraex.

### 7. Everything else still works

1. Tray → Settings. **Look for**: the settings window opens (over the game, centred, with no
   title bar — it is the same overlay window) and is usable: pages switch, a hotkey can be
   recorded, Save closes it.
2. Record a different price-check hotkey, save, and repeat step 1 with it. **Look for**: the
   new chord works and the old one does not.
3. `Alt + F4` on the settings window closes the window and leaves Wraex in the tray; Tray →
   Quit exits the app and leaves the game running.
4. Quit the game, then Tray → Settings again. **Look for**: the settings window still opens and
   is usable — it is the only way in while no hotkey is registered (hotkeys need the game's
   focus). It is still the overlay-style window (no title bar, on top), sized to the settings.

## When something fails

Collect, in the issue or report:

- the whole `wraex.log` from start to the failing step;
- the game's mode (borderless / fullscreen), resolution and Windows display scale;
- the `windowTitle` from the config, and whether Wraex runs as administrator;
- the class name the log printed when it attached.

## Known gaps in this spike

These are deliberate, and each is a follow-up rather than a bug:

- **Settings and the widget share one window.** Upstream has a separate overlay window and a
  separate settings window; the port draws both in the overlay (settings centred). Splitting
  them is phase 5 work.
- **No global mouse or wheel hooks.** Upstream closes the widget when the mouse moves away
  from it and turns Ctrl+wheel into stash-tab changes; both need a low-level hook that is not
  written yet. Until then `Escape`, `Ctrl + W` and the overlay key put the game back in front.
  A widget that opens under the cursor takes the mouse immediately (upstream waits for the
  first mouse move onto it, which in practice is the same moment).
- **The Alt peek** (`OverlayVisibility`) runs in the overlay state machine but has no key hook
  feeding it, so holding Alt does not hide the widget yet.
- **No screenshot/OCR** (`OverlayController.screenshot()`, the heist gem finder).
- **macOS, X11 and Wayland** have no window hook (`wraex_platform::window::Watcher::start`
  reports `Unsupported` there). Rune's interactive overlays are unsupported on Wayland, which
  PLAN.md notes as a known loss.
- **A widget opened from the tray is centred**, not at the cursor: only a hotkey knows where
  the cursor was.
