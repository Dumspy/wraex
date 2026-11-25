# Porting a client-log widget

How a widget that reads the game's `Client.txt` gets its data, how to draw it, and what to
check. The experience tracker (`ref/ee2/renderer/src/web/leveling/WidgetXpTracker.vue` →
`rust/crates/app/src/widgets/xp_tracker.rs`) is the worked example; the pieces below name it
wherever it is the first user of something.

Only PoE2 has the live log. APT (`ref/apt`) reads `Client.txt` for chat and trade whispers
(`renderer/src/web/client-log/client-log.ts`), not for level tracking, and that half is not
ported; do not invent a PoE1 equivalent. EE2's `readClientLog` is opt-in
(`defaultConfig()` sets it false), and the shell follows that.

## Where the events come from

| Layer | File | What it does |
|---|---|---|
| Reference | `ref/ee2/main/src/host-files/GameLogWatcher.ts` | Watches `Client.txt`, sends new lines as `MAIN->CLIENT::game-log`. |
| Reference | `ref/ee2/renderer/src/web/client-log/client-log.ts` | `useClientLog()`: parses a line and keeps `areaLevel`, `zoneName`, `playerLevel`, `lastCharacter`. |
| Rust port | `rust/crates/core/src/game_files.rs` (`LogWatcher`) | Tails the file: new lines only, 450 ms polls, the 64 KiB chunk quirk of the reference. |
| Rust port | `rust/crates/poe2/src/client_log.rs` (`ClientLog`) | The parser and the same refs, without Vue. `handle_line` returns an event or `None` for plain log lines. |
| App | `rust/crates/app/src/client_log.rs` (`Log`, `State`) | The worker: opens the file from the config or the defaults, feeds lines, publishes a `State` snapshot. |

`Log` runs on its own thread and is started once, for PoE2 only, in
`shell::Live::boot`. The shell hands it `readClientLog` and `clientLog` from the config
(`Live::sync_client_log`), again after a settings save (`reload_config`); `Log::update` restarts
the watcher only when that pair changed. With `readClientLog` false the worker idles. A
configured path wins; an empty one uses `game_files::log_paths` and the one readable candidate
(`guess_file_location`), as `GameLogWatcher.restart` does. Reading starts at the *end* of the
file; `Log::reread` is EE2's `re-parse-log` (reset to the start), available for a future
settings button but not wired to one.

Timestamps: the reference's `Date.parse("YYYY/MM/DD HH:mm:ss")` reads the machine's local time;
`ClientLog::new` takes the offset explicitly. The worker samples the current offset once
(`wraex_platform::time::local_utc_offset_seconds`) and documents the DST caveat. No ported
widget reads `ts`, so the XP tracker is unaffected by that approximation.

## How state reaches the app

1. `Log` compares `State::from_log(&log)` after each poll and, when it changed, stores it in an
   `Arc<Mutex<Option<State>>>` and calls the callback it was spawned with. `Live::boot` passes
   `move || handle.wake()`, so Rune runs a poll instead of sleeping.
2. `Live::poll` takes the newest snapshot once (`Log::take`) and calls
   `App::apply_client_log(state)`.
3. `App::apply_client_log` stores it in `App::client_log` and, when the overlay exists,
   copies it into `widgets::Overlay` (`Overlay::set_xp`). `App::ensure_overlay_state` copies a
   snapshot into an overlay built later, so a line read before the first frame is not lost.
4. The widget's `view` reads `Overlay`'s copy. A scene bypasses 1–3: `Overlay::scene` sets the
   fixed state itself (`xp_tracker::scene_state`), because a scene must not read the user's log.

Anything the widget writes back to the snapshot (the XP tracker's level input calls
`Overlay::xp.player_level` directly, like `setPlayerLevel`) lives with the overlay, not with
`App`; the next worker snapshot wins, as a level-up does upstream.

## Registering a widget

Each step is small and local; nothing registers widgets dynamically.

1. **Settings/config.** If the widget needs a setting, add its default to the game's
   `default_config` (`rust/crates/poe2/src/config.rs`) and a control to `settings/pages/*.rs`.
   The XP tracker's `showExp` is on `settings/pages/leveling.rs`; `readClientLog` and
   `clientLog` appear on both the General and Leveling pages, as they do in EE2. A new settings
   page needs `Page`'s `ALL`/`MENU`, `slug`, `title_key`,
   `seed_edits` and `pages::view` (see `Page::Leveling`).
2. **Widget module.** One Rust module per Vue component (`widgets/xp_tracker.rs`), with
   `WM_TYPE`, the card's `view`/`card`, and the pure maths in functions with unit tests. Port
   numbers exactly (`Math.floor`, `Math.max`, `toFixed`): `xp_tracker`'s tests carry hand
   computed cases from the Vue code.
3. **`widgets/mod.rs`.**
   - a `Kind` variant, in `Kind::ALL`, and its scene name (`xp-tracker`) and `wm_type`
     (`experience-tracker`);
   - `chrome_for`: the component's `Widget` props (`removable`, `inline-edit`,
     `move-handles`); `xp_tracker` is `removable="false"`, `inline-edit="false"`,
     `move-handles="top-bottom"`;
   - `card`: send the `wm_type` to the module's view;
   - `scene_size`: measure the reference's card (the screenshot footer logs it; the XP tracker
     is 192 × 40);
   - running state, if any, on `Overlay` (`Overlay::xp`, `Overlay::set_xp`) and its initial
     value in `Overlay::new`. `Overlay::scene` can set a fixed one;
   - `initialised` when `wm.create` may make an instance (the XP tracker is
     `instances: 'single'`, so only a config that lost its widget gets here);
   - `widgets::scene` when one game only has it (PoE1 must not open `widget-xp-tracker`).
4. **Scene.** Add `SCENES` in `lib.rs` (`poe2-widget-xp-tracker`). The menu lists any hidden
   widget automatically (`WidgetMenu.vue`'s one-button-per-hidden-widget loop), so no menu
   change is needed unless it should be `menu::skip`.
5. **Docs.** Update the module lists in `widgets/mod.rs` and `lib.rs` if the widget count or
   names change.

## Scene and test checklist

- `cargo run -p wraex-app -- --snap /tmp/xp.png --scene poe2-widget-xp-tracker` and look at the
  PNG; `--tree --scene …` prints the element keys.
- Compare with the reference: `cd tools/golden && npm run screens -- --game poe2 --overlay
  xp-tracker` (writes `target/screens/poe2/xp-tracker.png` and `.txt`). The harness fakes the
  main process; for a client-log widget it sends `MAIN->CLIENT::game-log` with fixed lines
  (`screens/widgets.mjs`), the same state the Rune scene uses.
- A settings page has a reference screenshot too: `npm run screens -- --game poe2 --settings
  leveling` (`target/screens/poe2/settings-leveling.png`).
- Rust tests:
  - the pure maths (`xp_tracker::tests`) against values computed from the Vue component;
  - the scene's texts and element keys (`widgets/tests.rs`, e.g.
    `the_xp_tracker_shows_the_logs_levels_and_penalty`);
  - the worker against a temporary `Client.txt` (`client_log::tests`), including "the initial
    state" and "a zone and a level-up change it".
- `devenv tasks run check:all`, and the Windows cross-check:
  `cd rust && cargo xwin clippy -p wraex-app --target x86_64-pc-windows-msvc -- -D warnings`.
  A new platform API (`wraex_platform::time`) needs the Windows path compiled by that check.

## What is not ported yet

- EE2 forwards every non-`log` event to its main process (`CLIENT->MAIN::write-data`), which
  the minimap/stat widgets use. The port keeps only the state snapshot; add the event stream
  when a widget needs one.
- `re-parse-log` has no button (EE2's is commented out too).
- PoE1's chat/trade log parsing (APT's `client-log.ts`) is a separate, non-leveling job.
