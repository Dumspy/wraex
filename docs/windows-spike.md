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
| The overlay window (transparent, on top, click-through, covering the game's client area), shown inactive | `rust/crates/app/src/shell/mod.rs` (`Panel`, `OverlayPanel`, `sync_window`) |
| Global mouse moves, buttons and wheel (`uiohook`'s half) | `rust/crates/platform/src/window/win32.rs` (`WH_MOUSE_LL` on the hook thread) |
| Global key down/up (`uiohook`'s other half): the Alt peek and the key log. Keys Wraex synthesizes are flagged injected and ignored | `rust/crates/platform/src/window/win32.rs` (`WH_KEYBOARD_LL` on the hook thread), `rust/crates/app/src/shell/mod.rs` (`poll_keyboard`) |
| Following the mouse onto the price check, closing it when the mouse leaves, Ctrl+wheel stash tabs | `rust/crates/platform/src/overlay.rs` (`WidgetAreaTracker` port), `rust/crates/app/src/shell/mod.rs` (`poll_mouse`, `track_area`), `shortcuts.rs` (`on_wheel`) |
| Hotkeys registered only while the game is focused | `rust/crates/app/src/shell/mod.rs` (`sync_registration`) |
| Key synthesis allowed by default there, opt-in elsewhere | `rust/crates/app/src/shell/keys.rs` |
| Focus hand-back (`SetForegroundWindow` + the foreground-lock workaround) | `rust/crates/platform/src/window/win32.rs` (`focus_window`) |
| The price check filling the overlay's height, listings scrolling inside | `rust/crates/app/src/poe1/price_check/view.rs`, `.../trade/mod.rs` (and PoE2's) |

## Before the first run

1. Start Path of Exile (or Path of Exile 2) **in windowed or borderless-windowed mode** once,
   so the game has written its config; fullscreen is the last step, not the first.
2. Get the build from GitHub; there is no need to build Wraex yourself. Open the latest
   **Release** run on `development` under the repository's **Actions** tab (or the one linked
   to you), download the `wraex-windows` artifact at the bottom of the run's page, and unzip it.
   Inside is `wraex-<version>-x86_64-pc-windows-msvc.zip`: unzip that anywhere too. Keep
   `wraex.exe` next to its `assets` folder. That stays the main test path: it is the same
   files however they are started, and it needs no install. The same artifact also holds
   `wraex-<version>-x86_64-pc-windows-msvc-setup.exe`, the installer, for testing the
   installed copy instead or as well. On Windows, for a setup run:

   - it should install into `%LOCALAPPDATA%\Programs\Wraex` (or the folder you pick), with
     no administrator prompt, a Start menu entry and, if ticked, a desktop shortcut;
   - **Settings > Apps** should list "Wraex" with the version, the icon and an uninstall
     entry; uninstalling it should ask you to quit Wraex while it is still running, and
     leave `%APPDATA%\Wraex` (settings, log) alone;
   - running the setup again with `/S` (what the future self-updater will do) should
     install over the same folder without any window, and keep working after a restart.
3. Run it: double-click `wraex.exe` (or use the Start menu entry, if you ran the setup
   instead), or from PowerShell in that folder `.\wraex.exe --game poe1` (or `--game poe2`).

   The log is written to `%APPDATA%\Wraex\Path of Exile\wraex.log` (PoE2:
   `%APPDATA%\Wraex\Path of Exile 2\wraex.log`), each line with the seconds since start;
   panics land there too. Each start keeps the previous run's file as `wraex.old.log`, so copy
   the file somewhere before starting a third time. To watch it live, in PowerShell:

   ```powershell
   Get-Content "$env:APPDATA\Wraex\Path of Exile\wraex.log" -Wait
   ```

   The first line names the version and the file: `wraex 0.1.0: logging to …`. If the file
   cannot be created, that is in the console instead and the log goes to the console only.

4. What the log says on a healthy start (a few lines each):

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
2. Press the price-check hotkey (default `Ctrl + D`; recorded in the settings window) **and
   keep Ctrl held**.
3. **Look for**: the panel appears with the item's name and prices, starting at the cursor
   (12 px below and right) and inside the game's window. No title bar, no taskbar button, no
   Alt+Tab entry for it. **The game keeps playing**: no alt-tab flash, the character still
   moves, W/A/S/D keep working while Ctrl is held.
4. **Look for** in the log: `wraex: focus game=true overlay=false hotkey=false`. There is no
   `focus game=false` yet — the overlay has not taken the mouse.

If pressing the hotkey does nothing: the log must still say `N hotkeys registered` from when
the game came to the front. If it does not, the game is not focused from the hook's point of
view — click into the game once. The window title is case- and space-exact (`Path of Exile`,
not `Path of exile`).

### 2. The panel becomes clickable when the mouse reaches it, and only then

1. With Ctrl still held, move the mouse onto the panel. **Look for**: it takes the mouse
   exactly when the cursor enters the card (hover states on the filters and listings work,
   clicks inside it do not reach the game) and the log says
   `wraex: focus game=false overlay=true hotkey=false`.
2. Scroll inside the listings. **Look for**: the panel scrolls, the game's camera/skills do
   not react.
3. Click a filter row. **Look for**: the row answers, the game does nothing.
4. Release Ctrl and move the mouse away from the card (without clicking). **Look for**: the
   panel closes and the game is in front again — `wraex: focus game=true overlay=false …`.
   The same happens if the card was opened without holding Ctrl: moving more than 2.5 rem
   from the item closes it (`WidgetAreaTracker`'s `closeThreshold`).
5. Press the hotkey again and move onto the card while still within that distance (or with
   Ctrl held), then click inside it. **Look for**: the click takes the overlay, and the card
   stays until Escape / the overlay key / a click on the game.

### 3. The focus goes back to the game, and the panel closes

1. With the panel open and hovered, press `Escape` (or `Ctrl + W`).
2. **Look for**: the panel closes (upstream's `hide-on-blur` widget rule; it does not stay
   drawn) and the game is in front again — moving the character works immediately, without
   clicking.
3. **Look for** in the log: `wraex: focus game=true overlay=false hotkey=false`, then
   `wraex: N hotkeys registered: …` (the hotkeys come back with the game's focus).
4. Click into the game instead of pressing Escape. **Look for**: the same: the game gets the
   focus, the panel closes, and hotkeys re-register.

The overlay key (`Shift + Space` by default) toggles the overlay from inside it and is marked
`hotkey=true` in that log line; the game keeps it registered, so pressing it in the game turns
the overlay interactive even without the mouse.

### 4. Borderless windowed, resizing and moving

1. Play in borderless-windowed (the normal way to play with an overlay) and repeat step 1.
2. While the panel is open, drag the game's window to another monitor and back. **Look for**:
   the panel follows the game's client area (it is placed over the game, so it moves with it),
   and the next widget appears at the cursor's new position.
3. Resize the game window (odd sizes included: 1280×720, 1920×1080, 2560×1440). **Look for**:
   the panel stays inside the game and is not cut off; the widget is clamped to the window,
   and the price check is the overlay's full height with its listings scrolling inside it —
   no black stripes or rows drawn outside the card's background, at any game height. (The
   port's own renderer can be checked without Windows: `--snap out.png --scene
   currency-divine-orb --size 460x700` and `--size 460x1100`.)
4. Switch the game to exclusive fullscreen and back. **Look for**: the panel still lands on
   the game (the client area is the whole screen) and the hotkey keeps copying items.
5. **Look for** on a monitor whose Windows scale is not 100 % (e.g. 150 %): the panel is the
   same physical size and lands at the cursor, not offset by the scale.

### 5. Leaving the game

1. Alt+Tab to another application. **Look for**: the panel disappears with the game (a
   `focus-change` with the overlay out of front closes it, as upstream's `hide-on-blur`
   widgets do), and the price-check hotkey does **nothing** there (press it in Notepad: no
   panel, and no `Ctrl + D` typed into Notepad either). This is upstream's rule: the hotkeys
   exist only while the game has the focus.
2. Alt+Tab back. **Look for**: the hotkey works again and opens a fresh panel; the old one
   does not come back (it was closed when the game lost the focus).

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

### 8. The Alt peek

`OverlayVisibility`: a lone Alt held down hides the widgets so the game underneath can be
looked at; letting go brings them back. The keyboard hook only observes, so no key may be
lost, repeated or blocked while this runs.

1. Open a price check and leave the cursor away from its card. Hold **Alt alone** and keep it
   down. **Look for**: the card disappears about a quarter-second later (275 ms) and the game
   is visible underneath; the character still turns and walks with Alt held.
2. Release Alt. **Look for**: the card is back in the same place, still click-through if the
   cursor is away from it (step 2's behaviour is unchanged).
3. Tap Alt and release it within ~275 ms. **Look for**: no flicker — the card never disappears.
4. Hold Alt and press another key (e.g. Alt+A, or Alt+Tab). **Look for**: the card does **not**
   stay hidden — a lone Alt is the peek, any other key cancels it (upstream's keydown rule).
5. Move the mouse onto the card so it becomes interactable, then hold Alt. **Look for**: it
   hides after 85 ms instead (the interactable delay), and the release brings it back with the
   card still clickable.
6. Tap Alt and move the mouse within the 275 ms window. **Look for**: the card never
   disappears — a mouse move cancels a pending peek, as upstream's `mousemove` handler does.
7. Press the price-check hotkey while the game's "show advanced item descriptions" key is Alt
   (PoE2 default) so Wraex synthesizes Ctrl+Alt+C. **Look for**: the card does not hide — the
   injected Alt is ignored by the hook.
8. Open the game's chat and type a sentence while the hook runs. **Look for**: every character
   appears exactly once and in order; the log (with `logKeys` on) shows one `Keydown`/`Keyup`
   per press and no synthesized keys.

## When something fails

Collect, in the issue or report:

- the whole `wraex.log` from start to the failing step;
- the game's mode (borderless / fullscreen), resolution and Windows display scale;
- the `windowTitle` from the config, and whether Wraex runs as administrator;
- the class name the log printed when it attached;
- whether the log has `the global mouse hook could not be installed` or `the global keyboard
  hook could not be installed` (another application may hold the hook, or the app is sandboxed).

## What the first Windows run found, and what changed

| Symptom | Cause in the port | Fix |
|---|---|---|
| "It alt-tabs out to the overlay" | every hotkey set `activate_on_show`, and `sync_window` called Rune's `open()`, which activates the window | hotkey-opened widgets call `show()` (`showInactive()`), stay click-through, and the game keeps the keyboard. Only the settings window and a window with no game to sit over take the focus |
| "Can click through into the game a lot" | nothing activated the overlay once the game had the focus back: no global mouse input | a `WH_MOUSE_LL` hook feeds the ported `WidgetAreaTracker`; with the card's area armed, the mouse entering it turns click-through off and focuses the overlay |
| "Stuck on screen, could not close it" | Escape went to the game and the tracker had no input, so nothing ever closed the card | moving away from the press position closes the price check (`hide-exclusive-widget`); Escape/Ctrl+W and the overlay key close it on the `focus-change`; the game taking the focus back closes every `hide-on-blur` widget |
| black stripes under the card | the price-check card was a fixed 900 pt inside an overlay of the game's height, and the listing table grew past the card's background | `#price-window` fills the overlay's height like upstream's `h-full`, the listings are the scrolling part, and the card clips to its own rectangle |

What still needs a real Windows run: the hook's coordinate/DPI conversion at non-100 % scale,
whether the click that activates a click-through overlay reaches the game once (as upstream's
does), the wheel hook's interplay with the game's own stash scrolling, and the Alt peek's
timing/modifier tracking from the keyboard hook (step 8 above).

## Known gaps in this spike

These are deliberate, and each is a follow-up rather than a bug:

- **The input hooks are Win32 only.** macOS, X11 and Wayland have no window, mouse or keyboard
  hook (`wraex_platform::window::Watcher::start` reports `Unsupported` there). Rune's
  interactive overlays are unsupported on Wayland, which PLAN.md notes as a known loss.
- **No screenshot/OCR** (`OverlayController.screenshot()`, the heist gem finder).
- **A widget opened from the tray is centred**, not at the cursor: only a hotkey knows where
  the cursor was. The tray and menu paths also still take the focus for the widget they open,
  where a hotkey-opened widget stays inactive.
