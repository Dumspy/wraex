//! The self-updater's OS side: running the setup helper, waiting for it, and relaunching.
//!
//! [`install`] holds the install/rollback state machine; this module is the
//! [`install::System`] implementation it runs against, plus the two paths and the helper
//! hand-off the app needs:
//!
//! - [`updates_dir`] is where downloads and the helper live: `%LOCALAPPDATA%\wraex-updater`
//!   on Windows (electron-updater's own place), the system temporary directory elsewhere.
//! - [`installed_dir`] recognizes an NSIS install by its `Uninstall.exe`; the portable zip
//!   has none, so a portable run reports an update only instead of installing it.
//! - [`start_helper`] copies the running executable to the updates directory as
//!   `wraex-updater.exe` and starts that copy detached with the plan's arguments. The app
//!   calls it on quit, after the download is verified.
//! - [`run_helper`] is the helper's entry point: [`install::apply`] with a log file in the
//!   updates directory.
//!
//! Everything compiles on every platform. Only the detached-spawn flags are
//! Windows-specific; on other platforms the same commands simply run as separate processes.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use wraex_core::updates::install::{self, Outcome, Plan, System};

/// The copy of the app that installs an update after the app itself has exited.
pub const HELPER_EXE: &str = "wraex-updater.exe";

/// The log the helper appends to beside its downloads.
const LOG_FILE: &str = "update.log";

/// The real [`install::System`]: std filesystem calls, `Command` for the setup and the
/// smoke test, and a detached process for the relaunch.
#[derive(Debug, Clone, Default)]
pub struct OsSystem {
    /// Where [`System::log`] appends, besides the app log and stderr.
    log_file: Option<PathBuf>,
}

impl OsSystem {
    /// A system whose log also goes to `log_file` (`None` keeps it on stderr and the app log).
    pub fn new(log_file: Option<PathBuf>) -> Self {
        Self { log_file }
    }
}

impl System for OsSystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn sha256(&self, path: &Path) -> io::Result<String> {
        // The same streamed hash the download verified the bytes with.
        wraex_core::updates::download::sha256_file(path)
    }

    fn run(&self, program: &Path, args: &[&str], timeout: Duration) -> io::Result<Option<i32>> {
        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait()? {
                // A process killed by a signal has no code; report it like any other abnormal
                // exit instead of pretending it succeeded.
                return Ok(Some(status.code().unwrap_or(-1)));
            }
            if start.elapsed() >= timeout {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn spawn(&self, program: &Path, args: &[&str]) -> io::Result<()> {
        let mut command = Command::new(program);
        command.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        // The helper and the relaunched app must outlive the process that starts them, so
        // they get no console of their own and are not tied to the caller's process group.
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const DETACHED_PROCESS: u32 = 0x0000_0008;
            const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
            command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
        }
        command.spawn().map(|_| ())
    }

    fn sleep(&self, duration: Duration) {
        thread::sleep(duration);
    }

    fn log(&self, line: &str) {
        crate::log!("wraex updater: {line}");
        let Some(path) = &self.log_file else { return };
        if let Some(folder) = path.parent() {
            let _ = fs::create_dir_all(folder);
        }
        if let Ok(mut file) = fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// Where downloads, the helper copy and its log live: `%LOCALAPPDATA%\wraex-updater` on
/// Windows, the system temporary directory elsewhere.
pub fn updates_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("wraex-updater"))
    }
    #[cfg(not(windows))]
    {
        Some(std::env::temp_dir().join("wraex-updater"))
    }
}

/// The NSIS install directory `exe` belongs to: its parent when that holds
/// `Uninstall.exe`. The portable zip has no uninstaller, so it reports `None`.
pub fn installed_dir(exe: &Path) -> Option<PathBuf> {
    let dir = exe.parent()?;
    dir.join("Uninstall.exe").is_file().then(|| dir.to_owned())
}

/// Copies the running executable to the updates directory as [`HELPER_EXE`] and starts it
/// detached with `plan`'s arguments. The app exits right after, so the copy can replace the
/// old executable once Windows lets go of it.
pub fn start_helper(plan: &Plan) -> io::Result<()> {
    let dir = updates_dir().ok_or_else(|| io::Error::other("no updates directory"))?;
    fs::create_dir_all(&dir)?;
    let helper = dir.join(HELPER_EXE);
    fs::copy(std::env::current_exe()?, &helper)?;
    let args = install::helper_args(plan);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    OsSystem::default().spawn(&helper, &args)
}

/// The helper process's entry point: install `plan`, log beside the downloads, and report
/// what happened.
pub fn run_helper(plan: &Plan) -> Outcome {
    let log_file = updates_dir().map(|dir| dir.join(LOG_FILE));
    install::apply(&OsSystem::new(log_file), plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory under the system temporary directory, removed and recreated so a
    /// previous run cannot interfere.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wraex-updater-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a temporary directory");
        dir
    }

    #[test]
    fn an_install_is_recognised_by_its_uninstaller() {
        let dir = temp_dir("installed");
        let exe = dir.join("wraex.exe");
        fs::write(&exe, b"").expect("the executable");
        assert_eq!(installed_dir(&exe), None, "no Uninstall.exe: a portable copy");

        fs::write(dir.join("Uninstall.exe"), b"").expect("the uninstaller");
        assert_eq!(installed_dir(&exe), Some(dir.clone()));

        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    fn slow() -> (PathBuf, Vec<&'static str>) {
        (PathBuf::from("sh"), vec!["-c", "sleep 5"])
    }

    #[cfg(windows)]
    fn slow() -> (PathBuf, Vec<&'static str>) {
        (PathBuf::from("cmd"), vec!["/C", "ping -n 6 127.0.0.1 > NUL"])
    }

    #[cfg(unix)]
    fn quick(code: i32) -> (PathBuf, Vec<String>) {
        (PathBuf::from("sh"), vec!["-c".to_owned(), format!("exit {code}")])
    }

    #[cfg(windows)]
    fn quick(code: i32) -> (PathBuf, Vec<String>) {
        (PathBuf::from("cmd"), vec!["/C".to_owned(), format!("exit {code}")])
    }

    #[test]
    fn a_long_running_program_is_killed_at_the_timeout() {
        let (program, args) = slow();
        let system = OsSystem::default();
        let started = Instant::now();
        assert_eq!(system.run(&program, &args, Duration::from_millis(100)).expect("spawned"), None);
        assert!(started.elapsed() < Duration::from_secs(5), "the timeout ended it");
    }

    #[test]
    fn a_quick_program_reports_its_exit_code() {
        let (program, args) = quick(3);
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let system = OsSystem::default();
        assert_eq!(system.run(&program, &args, Duration::from_secs(30)).expect("spawned"), Some(3));
    }
}
