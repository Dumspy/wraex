//! EE2's `Client.txt` reader, as the app runs it: the live half of
//! [`wraex_poe2::client_log::ClientLog`].
//!
//! EE2's main process watches the game's log (`GameLogWatcher.ts`) and broadcasts new lines
//! (`MAIN->CLIENT::game-log`); its renderer parses them in `useClientLog()` and the widgets read
//! the zone, area level, player level and character from there. The port keeps that split: a
//! worker owns the [`LogWatcher`] and one [`ClientLog`], and publishes a [`State`] snapshot
//! whenever an event changed it. The app applies snapshots on the UI thread and redraws, which
//! is the equivalent of the renderer's reactive refs.
//!
//! Only PoE2 has this: APT's renderer reads `Client.txt` for chat and trade whispers
//! (`ref/apt/renderer/src/web/client-log/client-log.ts`), not for level tracking, so the
//! shell starts the worker for [`GameId::Poe2`] alone (the chat half is not ported).
//!
//! # Which file, and re-reading
//!
//! `clientLog` in the config wins when it is set; an empty value uses
//! [`game_files::log_paths`]'s candidates, kept only when exactly one is readable
//! (`guess_file_location`) — EE2's `GameLogWatcher.restart`. Reading starts at the end of the
//! file, as upstream's `offset: stats.size` does. A config change restarts the worker with the
//! new path and `readClientLog`; an unchanged path keeps the open watcher. `re-parse-log`
//! (EE2's `GameLogWatcher` offset reset, sent only by the commented-out settings button) is
//! [`Log::reread`] for when a caller needs it.
//!
//! # Timestamps
//!
//! The reference's timestamps are `Date.parse("YYYY/MM/DD HH:mm:ss")`, which V8 reads as the
//! machine's local time. The port makes the offset explicit
//! ([`ClientLog::new`]'s `utc_offset_seconds`) and samples the machine's current offset
//! ([`wraex_platform::time::local_utc_offset_seconds`]) when the worker opens the log. A log
//! that spans a daylight-saving change is therefore an hour off for the other half, because
//! the offset is not the one in force at each line's date; the XP tracker never reads `ts`, so
//! nothing the port shows today depends on it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use wraex_core::GameId;
use wraex_core::game_files::{self, LOG_POLL_INTERVAL_MS, LogWatcher};
use wraex_poe2::client_log::ClientLog;

/// `useClientLog()`'s refs, as one snapshot: what a widget reads.
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    /// `areaLevel`: the current zone's monster level.
    pub area_level: f64,
    /// `playerLevel`: the character's level.
    pub player_level: f64,
    /// `zoneName`: the last real zone, lowercased; the Riverbank until a zone loads.
    pub zone: String,
    /// `lastCharacter`: the character of the last level-up, empty before one.
    pub character: String,
}

impl Default for State {
    /// `useClientLog()`'s initial refs: area level 1, Riverbank, player level 1.
    fn default() -> Self {
        Self { area_level: 1.0, player_level: 1.0, zone: "G1_1".to_owned(), character: String::new() }
    }
}

impl State {
    /// The refs of a log, as they stand.
    pub fn from_log(log: &ClientLog) -> Self {
        Self {
            area_level: log.area_level,
            player_level: log.player_level,
            zone: log.zone_name.clone(),
            character: log.last_character().to_owned(),
        }
    }
}

/// What the worker should do: EE2's `restart(logFile, readLog)` arguments.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Wanted {
    /// `readClientLog`: opt-in in EE2 (`defaultConfig` sets it false).
    read: bool,
    /// `clientLog`: the configured path; empty means "guess".
    path: String,
}

/// The live log reader: a worker thread plus the latest snapshot it published.
#[derive(Debug, Clone)]
pub struct Log {
    wanted: Arc<Mutex<Wanted>>,
    /// Bumped by [`Log::reread`]; the worker reopens the file when the value changes.
    rereads: Arc<AtomicU64>,
    latest: Arc<Mutex<Option<State>>>,
}

impl Log {
    /// Start the worker. `on_change` runs on the worker thread after a new snapshot lands; the
    /// shell's is `AppHandle::wake`, so the next poll applies it.
    ///
    /// The worker lives for the process: `readClientLog` off/on only switches the watcher.
    #[expect(
        clippy::expect_used,
        reason = "a worker thread that cannot start leaves no log reader at all, and the process cannot run without one"
    )]
    pub fn spawn(on_change: impl Fn() + Send + 'static) -> Self {
        let log = Self {
            wanted: Arc::new(Mutex::new(Wanted::default())),
            rereads: Arc::new(AtomicU64::new(0)),
            latest: Arc::new(Mutex::new(None)),
        };
        let wanted = Arc::clone(&log.wanted);
        let rereads = Arc::clone(&log.rereads);
        let latest = Arc::clone(&log.latest);
        std::thread::Builder::new()
            .name("wraex-client-log".to_owned())
            .spawn(move || run(&wanted, &rereads, &latest, on_change))
            .expect("start the client-log worker");
        log
    }

    /// `GameLogWatcher.restart(logFile, readLog)`: a changed path or switch restarts the file.
    pub fn update(&self, read: bool, path: &str) {
        if let Ok(mut wanted) = self.wanted.lock() {
            *wanted = Wanted { read, path: path.to_owned() };
        }
    }

    /// `MAIN->CLIENT::re-parse-log`: read the current file from its start on the next poll.
    pub fn reread(&self) {
        self.rereads.fetch_add(1, Ordering::SeqCst);
    }

    /// The newest snapshot the app has not applied yet.
    pub fn take(&self) -> Option<State> {
        self.latest.lock().ok().and_then(|mut latest| latest.take())
    }
}

/// The worker loop: apply config changes, read new lines, publish changed snapshots.
#[expect(clippy::infinite_loop, reason = "the worker lives for the process, as EE2's `GameLogWatcher` does")]
fn run(wanted: &Mutex<Wanted>, rereads: &AtomicU64, latest: &Mutex<Option<State>>, on_change: impl Fn()) {
    let mut applied = Wanted::default();
    let mut opened: Option<(LogWatcher, ClientLog)> = None;
    let mut attempted = false;
    let mut last: Option<State> = None;
    let mut reread_seen = rereads.load(Ordering::SeqCst);
    let mut from_start = false;
    let utc_offset = wraex_platform::time::local_utc_offset_seconds();
    loop {
        let next = wanted.lock().map(|wanted| wanted.clone()).unwrap_or_default();
        if next != applied {
            applied = next;
            opened = None;
            attempted = false;
            last = None;
            from_start = false;
        }
        let reread = rereads.load(Ordering::SeqCst);
        if reread != reread_seen {
            reread_seen = reread;
            opened = None;
            attempted = false;
            last = None;
            from_start = true;
        }
        if applied.read && opened.is_none() && !attempted {
            // `restart` tries once per config change: a game installed later needs the settings
            // saved again, as upstream does.
            attempted = true;
            match open(&applied.path, utc_offset) {
                Ok((mut watcher, log)) => {
                    if from_start {
                        // `re-parse-log`: read the whole file again on the next poll.
                        watcher.reread_from_start();
                        from_start = false;
                    }
                    let state = State::from_log(&log);
                    last = Some(state.clone());
                    publish(latest, state, &on_change);
                    opened = Some((watcher, log));
                }
                Err(message) => wraex_platform::log!("wraex: {message}"),
            }
        }
        if let Some((watcher, log)) = &mut opened {
            match watcher.poll() {
                Ok(batches) => {
                    for batch in batches {
                        for line in batch {
                            // The events themselves are not forwarded: the widgets read the
                            // snapshot, and EE2's main-process consumers are not ported.
                            let _ = log.handle_line(&line);
                        }
                    }
                    let state = State::from_log(log);
                    if last.as_ref() != Some(&state) {
                        last = Some(state.clone());
                        publish(latest, state, &on_change);
                    }
                }
                Err(error) => {
                    wraex_platform::log!("wraex: reading {} failed: {error}", watcher.path().display());
                    opened = None;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(LOG_POLL_INTERVAL_MS));
    }
}

fn publish(latest: &Mutex<Option<State>>, state: State, on_change: &impl Fn()) {
    if let Ok(mut latest) = latest.lock() {
        *latest = Some(state);
    }
    on_change();
}

/// `GameLogWatcher.restart`'s file half: the configured path, or the one readable default.
fn open(configured: &str, utc_offset: i32) -> Result<(LogWatcher, ClientLog), String> {
    let path: PathBuf = if configured.is_empty() {
        let candidates =
            game_files::log_paths(GameId::Poe2, crate::shell::current_os(), std::env::home_dir().as_deref());
        game_files::guess_file_location(&candidates).ok_or_else(|| {
            format!(
                "no readable Client.txt found; enter one in the settings ({})",
                candidates.iter().map(|path| path.display().to_string()).collect::<Vec<_>>().join(", ")
            )
        })?
    } else {
        PathBuf::from(configured)
    };
    let watcher = LogWatcher::open(&path).map_err(|error| format!("cannot watch {}: {error}", path.display()))?;
    // Only the client strings are used (`LOG_LEVEL_UP`, `LOG_ZONE_GEN`); the trade data is not
    // needed, so this never asks the trade site.
    let data = wraex_poe2::data::Data::load(&wraex_poe2::data::source(), "en", None)
        .map_err(|error| format!("cannot load the client-log strings: {error}"))?;
    Ok((watcher, ClientLog::new(&data, utc_offset)))
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// A log file whose watcher starts watching, then lines appended by the test.
    fn temp_log(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wraex-client-log-{}-{name}", std::process::id()));
        drop(std::fs::remove_dir_all(&dir));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("Client.txt")
    }

    fn append(path: &PathBuf, text: &str) {
        let mut file = std::fs::OpenOptions::new().append(true).create(true).open(path).unwrap();
        file.write_all(text.as_bytes()).unwrap();
        file.flush().unwrap();
    }

    /// The worker publishes a snapshot when it opens the log and again after a zone line and a
    /// level-up. The timeout is the watcher's poll interval plus data loading.
    #[test]
    fn the_worker_follows_the_log() {
        let path = temp_log("follows");
        append(&path, "2026/04/26 09:00:00 1 a1b2c3d4 [INFO Client 1] : Old line before watching\n");

        let changes = Arc::new(AtomicUsize::new(0));
        let woke = Arc::clone(&changes);
        let log = Log::spawn(move || {
            woke.fetch_add(1, Ordering::SeqCst);
        });
        log.update(true, path.to_str().unwrap());

        // The first publish is the initial state (area 1, Riverbank, level 1).
        let first = wait_for(&log).expect("the initial snapshot");
        assert_eq!(first, State::default());

        append(
            &path,
            concat!(
                "2026/04/26 09:28:58 1785282093 a1b2c3d4 [INFO Client 443804] ",
                "Generating level 80 area \"G1_9\" with seed 1\n",
                "2026/04/26 09:29:16 168144002 a1b2c3d4 [INFO Client 443804] ",
                ": SomeChar (Mercenary) is now level 72\n",
            ),
        );

        let deadline = std::time::Instant::now().checked_add(Duration::from_secs(10)).unwrap();
        let mut state = None;
        while std::time::Instant::now() < deadline {
            if let Some(next) = log.take() {
                state = Some(next);
                // The level is a parsed integer, so the exact comparison is what is meant.
                if state.as_ref().is_some_and(|state| (state.player_level - 72.0).abs() <= f64::EPSILON) {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let state = state.expect("a snapshot after the new lines");
        assert_eq!(
            state,
            State { area_level: 80.0, player_level: 72.0, zone: "g1_9".to_owned(), character: "SomeChar".to_owned() }
        );
        assert!(changes.load(Ordering::SeqCst) >= 2);
        drop(log);
    }

    fn wait_for(log: &Log) -> Option<State> {
        let deadline = std::time::Instant::now().checked_add(Duration::from_secs(10)).unwrap();
        while std::time::Instant::now() < deadline {
            if let Some(state) = log.take() {
                return Some(state);
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        None
    }
}
