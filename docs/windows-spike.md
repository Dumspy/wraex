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
| What the overlay draws while it is active: every widget whose `wmWants` is `show`, each at its own `anchor` (`OverlayWindow.vue`'s `visibilityState`), the exclusive widgets alone, and the background click back to the game | `rust/crates/app/src/shell/mod.rs` (`App::cards_for`, `App::overlay_cards`, `overlay_root`, `App::on_overlay_background`), `rust/crates/app/src/lib.rs` (`view`, `card_at`) |
| Global mouse moves, buttons and wheel (`uiohook`'s half) | `rust/crates/platform/src/window/win32.rs` (`WH_MOUSE_LL` on the hook thread) |
| Global key down/up (`uiohook`'s other half): the Alt peek and the key log. Keys Wraex synthesizes are flagged injected and ignored | `rust/crates/platform/src/window/win32.rs` (`WH_KEYBOARD_LL` on the hook thread), `rust/crates/app/src/shell/mod.rs` (`poll_keyboard`) |
| Following the mouse onto the price check, closing it when the mouse leaves, Ctrl+wheel stash tabs | `rust/crates/platform/src/overlay.rs` (`WidgetAreaTracker` port), `rust/crates/app/src/shell/mod.rs` (`poll_mouse`, `track_area`), `shortcuts.rs` (`on_wheel`) |
| Moving a widget (`Widget.vue`'s move handles), raising the pressed card (`wmZorder`) and saving both to the config | `rust/crates/app/src/widgets/mod.rs` (`move_handles`, `Overlay::begin_move`/`drag_to`/`end_move`, `bring_to_top`), `rust/crates/app/src/shell/mod.rs` (`Live::save_widget_config`, `sync_window`'s overlay size) |
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

   The first line names the version and the file: `wraex 0.1.0: logging to …`. The release
   build is a windowed program, as upstream's is: it opens no console (so no second taskbar
   button), and the log file is the only place its lines go. Wraex runs in the tray; quit it
   from there.

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
   stays until Escape / the overlay key / a click on the game's background (the overlay's
   `overlayBackgroundClose`, which gives the game the focus back).

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

1. In the game, press the overlay key with nothing else open. **Look for**: the widget menu at
   the top left (its `anchor: { pos: 'tl', x: 5, y: 5 }`, as upstream draws it), the game still
   visible behind it, and `wraex: focus game=false overlay=true hotkey=true`. The log line
   `wraex: overlay draws menu` names what the overlay paints. This was the first Windows run's
   overlay-key bug: `shown` said `None` and the window hid at once. From the menu, the cog opens
   the settings and the other buttons their widgets.
2. Click **Map rolling** in the menu. **Look for**: the menu *stays* where it is (cog included)
   and the stash search's card appears at its own anchor (35 %/46 % of the game) beside it, in the
   same overlay window — `wraex: overlay draws menu, stash-search`. Clicking the menu's other
   entries adds their cards the same way; each hides again with its own button (a shown widget is
   no longer listed), and a card that overlaps another one comes to the front when clicked.
3. Click the cog. **Look for**: the menu and the stash search go, and the settings window fills
   the overlay (centred, 50 × 38 rem, at the top — `SettingsWindow.vue` is `inset: 0` with the
   window inside it); `wraex: overlay draws settings`. Click a page, type in a field, click an
   empty part of the window (below the pages, the title bar's background): **nothing closes** —
   only Save, Cancel, the title bar's close, or the game taking the focus does.
4. Press Save. **Look for**: the settings window goes and the menu is back at 5 %/5 %, *not* in
   the middle of the screen where the settings window was, with the stash search still up if it
   was. Cancel does the same.
5. Click the background (anywhere outside every card). **Look for**: the game gets the focus, the
   menu goes (`invisible-on-blur`) and the hotkeys re-register — log
   `wraex: focus game=true overlay=false …`. Press Escape or the overlay key instead for the same.
6. Press the overlay key again while the menu is up. **Look for**: the same: the overlay key's off
   state gives the game the focus, and the menu closes with it.
7. Run a stopwatch and let the overlay lose the focus (click into the game). **Look for**: the
   running stopwatch stays on screen (its `invisible-on-blur` flag is cleared while it runs) while
   the menu goes; activating the overlay brings the menu back over it.

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

1. Tray → Settings. **Look for**: the settings window opens (over the game, centred
   horizontally and at the top — `SettingsWindow.vue` is `position: absolute; inset: 0` with the
   50 × 38 rem window inside it — with no title bar, in the same overlay window) and is usable:
   pages switch, a hotkey can be recorded, Save closes it. Opening it activates the overlay
   (`wraex: overlay takes the mouse and keyboard`), as upstream's settings widget is
   `invisible-on-blur`; the tray menu closing must not be what closes it. Save leaves the overlay
   active, so the widget menu is there afterwards — the background click or the overlay key goes
   back to the game.
2. Record a different price-check hotkey, save, and repeat step 1 with it. **Look for**: the
   new chord works and the old one does not.
3. `Alt + F4` on the settings window closes the window and leaves Wraex in the tray; Tray →
   Quit exits the app and leaves the game running.
4. Quit the game, then Tray → Settings again. **Look for**: the settings window still opens and
   is usable — it is the only way in while no hotkey is registered (hotkeys need the game's
   focus). With no game to sit over it is the window itself: still the overlay style (no title
   bar, on top), sized to the settings.
5. With the settings window open, click into the game. **Look for**: the game takes the focus
   and the settings window closes with the overlay (upstream's `invisible-on-blur` on the
   settings widget), rather than staying on top of the game.

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

### 9. Moving widgets and the paint order

Upstream's `Widget.vue`: the hover panel's **move** button draws the widget's handles, dragging
one puts that corner under the pointer and writes the new `anchor` (x/y percentages of the
overlay) into the config, and a press anywhere on a card brings it to the front
(`wm.bringToTop` → `wmZorder`). Both are saved, so a restart keeps them.

1. Open the overlay (overlay key) so the menu and, say, "Map rolling" are up. **Look for**: the
   cards' positions are where the config says (the menu at 5 %/5 %).
2. Click the card that is *behind* the other one. **Look for**: it comes to the front at once
   (the log line `wraex: overlay draws …` keeps naming both), and a click on its blank area or
   a label is enough for that — no title bar is needed.
3. Hover a card and click **move** in its actions panel. **Look for**: the card dims to 75 %,
   its handles appear (a stopwatch has the one in its middle; a corner-handled widget has the
   four; the image strip and the XP tracker the top/bottom ones), and the panel's `move` entry
   stays highlighted.
4. Drag a handle, slowly and to all four edges. **Look for**: the card follows the pointer
   every frame — the corner you grabbed stays under the cursor — and it is never dragged
   outside the game: at the edges it stops at the game's edge, and the saved percentage is
   0 % or 100 %.
5. Release, then hover the card again and click **move** to put the handles away. **Look for**:
   the card is not dimmed any more and stays where you dropped it.
6. Drag the *corner* versus the *middle* handle: the card jumps so that the handle you grabbed
   is at the pointer. That is `Widget.vue`'s `startMove`: `anchor.pos` becomes the handle's
   position.
7. Quit Wraex from the tray (or let the game take the focus and reopen the overlay), then start
   it again. **Look for**: every card is where you left it, in the same front-to-back order.
   The log's config path names the file the positions were written to
   (`%APPDATA%\Wraex\Path of Exile\config.json`); its `widgets` array holds the new `anchor`
   and `wmZorder`.
8. While a card is being moved, Alt-tab away or press the overlay key's off state. **Look for**:
   nothing breaks: a drag whose pointer Rune takes away is cancelled and the card returns to
   where the gesture started, and a gesture still in progress when the app quits leaves the
   config as it was (the file is written on release only).

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
| the overlay key showed nothing | `App::shown` named one window (or `None`), so activating the overlay with nothing open hid it in the same frame | `shown` follows upstream's `visibilityState`: while the overlay is active the widget menu is drawn (it is `wmWants: 'show'`), at its own `anchor` (5 %/5 %), and it goes with the overlay (`invisible-on-blur`). Settings and the checks are exclusive and still win |
| "Could not get back to the game from the menu" | the overlay covers the game and takes the mouse, and nothing handled a click on its background | an `overlay-background` layer runs upstream's `handleBackgroundClick`: with `overlayBackgroundClose` (on by default) it gives the game its focus back, which closes the menu and every `hide-on-blur` widget |
| "Can click through into the game a lot" | nothing activated the overlay once the game had the focus back: no global mouse input | a `WH_MOUSE_LL` hook feeds the ported `WidgetAreaTracker`; with the card's area armed, the mouse entering it turns click-through off and focuses the overlay |
| "Stuck on screen, could not close it" | Escape went to the game and the tracker had no input, so nothing ever closed the card | moving away from the press position closes the price check (`hide-exclusive-widget`); Escape/Ctrl+W and the overlay key close it on the `focus-change`; the game taking the focus back closes every `hide-on-blur` widget |
| black stripes under the card | the price-check card was a fixed 900 pt inside an overlay of the game's height, and the listing table grew past the card's background | `#price-window` fills the overlay's height like upstream's `h-full`, the listings are the scrolling part, and the card clips to its own rectangle |
| the menu came back in the middle of the screen after Save | one card at a time: the shell stored a single placement, and opening the settings set it to centred, so the menu inherited it | a `Widget.vue` card is placed at its config's `anchor` (`App::cards_for`), and only the hotkey checks and the settings window use a placement |
| "Map rolling" hid the menu | the one-card model drew the newly shown widget *instead of* the menu | `OverlayWindow.vue`'s `visibilityState`: every widget whose `wmWants` is `show` is drawn at once, each at its own anchor, and an exclusive widget is drawn alone |
| clicking inside the settings window closed it | the overlay's background click layer was four strips worked out around one card, and a click on a card's own blank area (no handler) fell through to it | the background is one layer under all the cards (`overlay_root`), and each card's box takes its own presses, so only the empty overlay gives the game the focus back |
| the next widget's offset was stale after one closed | the offset was cached per shown card | every card's offset is worked out from its anchor each frame |

The second bug of that run was not Windows-specific: after a filter change the price check kept
showing the listings of the old filters, because `CheckedItem.vue`'s deep watch on
`[item, doSearch, itemStats, itemFilters]` was not ported. Every edit of a stat or an item
filter now hides the results and brings the Search button back, while a trade-option change
(league, listing type, currency, ...) restarts the search by itself, as the second watch does.

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
- **A card goes where its config says, not where the cursor is.** `Widget.vue` places every card
  at its `anchor`, so a widget opened from the tray or the menu lands there too; only the price
  check and the item check open at the cursor (`Placement::Cursor`). Several widgets are drawn at
  once (`OverlayWindow.vue`'s `visibilityState`), each at its own anchor; the settings window and
  the two checks are exclusive and hide the rest, as upstream's `wmZorder: 'exclusive'` widgets
  do.
- **The overlay background is not painted.** Upstream dims the game with
  `AppConfig().overlayBackground` while the overlay is active; the port only honours the click
  on that background (`overlayBackgroundClose`), so the game looks unchanged behind the cards.
- **A card's anchor size is an estimate.** `Widget.vue` positions a card with CSS percentages, so
  its own size cancels out of the maths; the port lays the cards out in points and needs a size
  before the card is measured, so it uses the reference's own size (`widgets::scene_size`). A
  ported card a few points narrower only shifts a `tc`/`br`/`cc` anchor by half the difference.
- **The move mode's floating handle is not drawn.** `Widget.vue` hides the in-card handle that
  sits at the current `anchor.pos` and draws a fixed white-bordered `mover.active` at the anchor
  instead; the port draws every declared handle inside the card and has no overlay-level handle.
  Dragging works from every handle either way, and the card's own `anchor` is read and written
  by the overlay now (`Overlay::begin_move`/`drag_to`).
- **Widget positions are saved after the change, not on blur.** Upstream's `saveConfig()` runs
  when the overlay window loses the focus (and when an exclusive widget closes), so a config
  changed and then killed in the same second could be lost; the port writes the file in the
  poll after the drag or press (`Live::save_widget_config`) and skips the write when nothing
  changed.
