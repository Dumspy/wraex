//! The app's log: every line goes to stderr, as before, and, once [`init`] ran, to a file.
//!
//! Upstream's `RemoteLogger.ts` sends its lines to the renderer's debug page and keeps
//! nothing. Wraex runs without a console when started from a shortcut, so a live run also
//! writes `wraex.log` next to its settings: a tester can send the file without redirecting
//! stderr, and a crash's panic message is in it. The previous run's file is kept as
//! `wraex.old.log`.

use std::fmt;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static FILE: OnceLock<Mutex<File>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

/// One log line: `eprintln!`'s arguments.
#[macro_export]
macro_rules! log {
    ($($arg:tt)*) => {
        $crate::log::write(::std::format_args!($($arg)*))
    };
}

/// Start writing the log to `path`, keeping the previous file as `wraex.old.log` beside it, and
/// sending panics there too.
///
/// Its folder is created. Returns where it writes; a file that cannot be created leaves the log
/// on stderr alone.
pub fn init(path: &Path) -> Option<PathBuf> {
    START.get_or_init(Instant::now);
    if let Some(folder) = path.parent() {
        _ = std::fs::create_dir_all(folder);
    }
    if path.exists() {
        _ = std::fs::rename(path, old_path(path));
    }
    let file = match File::create(path) {
        Ok(file) => file,
        Err(error) => {
            _ = writeln!(std::io::stderr(), "wraex: cannot write the log file {}: {error}", path.display());
            return None;
        }
    };
    if FILE.set(Mutex::new(file)).is_err() {
        return None;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        write(format_args!("wraex: panic: {info}"));
        previous(info);
    }));
    Some(path.to_owned())
}

/// `wraex.old.log` beside `path`.
fn old_path(path: &Path) -> PathBuf {
    path.with_file_name("wraex.old.log")
}

/// Write one line to stderr and, after [`init`], to the file with the seconds since start.
pub fn write(args: fmt::Arguments<'_>) {
    // A failed write to stderr has nowhere left to go; the logger itself never panics.
    _ = writeln!(std::io::stderr(), "{args}");
    let Some(file) = FILE.get() else { return };
    let seconds = START.get().map_or(0.0, |start| start.elapsed().as_secs_f64());
    let mut file = file.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    _ = writeln!(file, "[{seconds:9.3}] {args}");
    _ = file.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_previous_log_is_kept_beside_the_new_one() {
        assert_eq!(old_path(Path::new("/a/b/wraex.log")), Path::new("/a/b/wraex.old.log"));
    }
}
