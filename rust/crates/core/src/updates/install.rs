//! Installing a downloaded setup on quit, with rollback.
//!
//! A Windows release ships as an NSIS setup (`tools/package/installer.nsi`). Running it
//! with `/S` reinstalls silently over the directory named by its own `InstallLocation`
//! registry value, which is the per-user install directory the app is running from:
//! `wraex.exe`, `assets/`, `licences/`, `wraex.ico` and `Uninstall.exe`. No `/D=` is
//! passed, so a user who chose a different directory keeps it.
//!
//! `wraex.exe` cannot replace itself while it runs: Windows refuses to rename or delete a
//! directory whose executable is open, and the setup answers a locked `wraex.exe` with
//! exit code 2 under `/S`. Installation is therefore a helper process:
//!
//! 1. On quit the app copies its own executable to the updates directory as
//!    `wraex-updater.exe` and starts that copy with the arguments of [`helper_args`]
//!    (`--install-update <setup> --install-dir <dir> --version <v> --sha256 <hex>
//!    [--relaunch]`).
//! 2. The helper ([`apply`]) waits for the old app to exit by retrying a directory
//!    rename, moves the old install aside to `<dir>.previous`, re-checks the setup's
//!    SHA-256, runs the setup, and smoke-tests the new executable with `--self-test`.
//! 3. On success it deletes the backup and the setup, and relaunches the app when the
//!    original run asked for it. On any failure it puts `<dir>.previous` back, deletes
//!    the bad setup and relaunches the restored executable.
//!
//! Known limit: the setup writes the Add/Remove Programs `DisplayVersion` registry value
//! before the smoke test runs. A rollback therefore leaves the newer version number in
//! Settings > Apps until the next successful install (which fixes it again). Restoring
//! that value would mean writing the registry from the helper, which this module
//! deliberately does not do.
//!
//! Upstream (`ref/apt/main/src/AppUpdater.ts`) calls `autoUpdater.quitAndInstall(false)`:
//! electron-updater downloads the setup, runs it on quit, and reports no result to the
//! app afterwards. There is no backup, no smoke test and no rollback; a setup that fails
//! silently leaves the old version installed but the user is not told. Wraex keeps the
//! same "download, then install on quit" shape (the download is
//! [`super::download`]) and adds the helper's backup, smoke test and rollback.
//!
//! The logic is pure: [`System`] is the only OS surface, so every step is testable on
//! every platform. `wraex_platform::updater::OsSystem` is the real implementation.

use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The installed executable's name inside the install directory.
pub const EXE: &str = "wraex.exe";
/// The flag the helper passes to a freshly installed executable to smoke-test it.
pub const SELF_TEST_FLAG: &str = "--self-test";

/// How often the helper tries to move the old install aside, 500 ms apart. Windows keeps
/// the directory locked until the old app's process has exited.
const RENAME_ATTEMPTS: u32 = 60;
/// The pause between rename attempts.
const RENAME_DELAY: Duration = Duration::from_millis(500);
/// How long the silent setup may run before it is killed and treated as a failure.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(5 * 60);
/// How long the new executable's `--self-test` may take.
const SELF_TEST_TIMEOUT: Duration = Duration::from_secs(60);

/// What the helper needs from the OS; `OsSystem` in `wraex_platform::updater` is the real
/// one, the tests use a fake.
pub trait System {
    fn exists(&self, path: &Path) -> bool;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// The file's SHA-256 as lower-case hex, for the setup the download verified.
    fn sha256(&self, path: &Path) -> io::Result<String>;
    /// Runs `program` to completion; `Ok(None)` when it ran past `timeout` and was killed.
    fn run(&self, program: &Path, args: &[&str], timeout: Duration) -> io::Result<Option<i32>>;
    /// Starts `program` detached and does not wait (the relaunch).
    fn spawn(&self, program: &Path, args: &[&str]) -> io::Result<()>;
    fn sleep(&self, duration: Duration);
    fn log(&self, line: &str);
}

/// One install the helper must perform. The setup is already verified and downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub setup: PathBuf,
    pub install_dir: PathBuf,
    pub version: String,
    /// The setup's SHA-256, as the download verified it. [`apply`] checks it again: the file
    /// sits in a user-writable folder between the download and the quit.
    pub sha256: String,
    /// Whether the app was running when the update was requested, so it should come back.
    pub relaunch: bool,
}

/// What [`apply`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// The new version is installed and passed `--self-test`.
    Installed,
    /// The old version was put back after the install or the smoke test failed.
    RolledBack { reason: String },
    /// Nothing was changed: the setup was missing, or the old app never exited.
    NotStarted { reason: String },
}

/// The directory the old install is moved to: a sibling named `<name>.previous`
/// (`...\Wraex` → `...\Wraex.previous`).
pub fn backup_dir(install_dir: &Path) -> PathBuf {
    let mut name = install_dir.file_name().unwrap_or_default().to_os_string();
    name.push(".previous");
    install_dir.with_file_name(name)
}

/// The helper's command line after the program name:
/// `--install-update <setup> --install-dir <dir> --version <v> --sha256 <hex> [--relaunch]`.
pub fn helper_args(plan: &Plan) -> Vec<String> {
    let mut args = vec![
        "--install-update".to_owned(),
        plan.setup.to_string_lossy().into_owned(),
        "--install-dir".to_owned(),
        plan.install_dir.to_string_lossy().into_owned(),
        "--version".to_owned(),
        plan.version.clone(),
        "--sha256".to_owned(),
        plan.sha256.clone(),
    ];
    if plan.relaunch {
        args.push("--relaunch".to_owned());
    }
    args
}

/// Parses [`helper_args`]' output (without the program name). `None` unless
/// `--install-update` carries a setup, `--install-dir`, `--version` and `--sha256` have
/// values; unknown arguments are ignored.
pub fn parse_helper_args(args: &[String]) -> Option<Plan> {
    let mut args = args.iter();
    let setup = loop {
        match args.next()? {
            arg if arg == "--install-update" => break args.next()?,
            _ => {}
        }
    };
    if setup.is_empty() {
        return None;
    }
    let mut install_dir = None;
    let mut version = None;
    let mut sha256 = None;
    let mut relaunch = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--install-dir" => install_dir = Some(PathBuf::from(args.next()?)),
            "--version" => version = Some(args.next()?.clone()),
            "--sha256" => sha256 = Some(args.next()?.clone()),
            "--relaunch" => relaunch = true,
            _ => {}
        }
    }
    let install_dir = install_dir.filter(|dir| !dir.as_os_str().is_empty())?;
    let version = version.filter(|version| !version.is_empty())?;
    let sha256 = sha256.filter(|sha256| !sha256.is_empty())?;
    Some(Plan { setup: PathBuf::from(setup), install_dir, version, sha256, relaunch })
}

/// Installs `plan`'s setup, rolling back to the previous installation on any failure.
pub fn apply(system: &impl System, plan: &Plan) -> Outcome {
    let backup = backup_dir(&plan.install_dir);
    system.log(&format!("update: installing {} {}", plan.version, plan.setup.display()));

    // A setup that is gone cannot install anything, and a stale backup belongs to an
    // earlier attempt whose rollback did not finish (or whose cleanup was interrupted).
    if !system.exists(&plan.setup) {
        let reason = format!("the setup {} is missing", plan.setup.display());
        system.log(&format!("update: not started: {reason}"));
        return Outcome::NotStarted { reason };
    }
    // The download verified the setup, but it has sat in a user-writable folder since then:
    // re-hash it now, before anything is moved, or a file swapped in between would run with
    // the user's rights. A file that no longer matches is bad and does not survive.
    let digest = system.sha256(&plan.setup);
    if !matches!(&digest, Ok(actual) if *actual == plan.sha256) {
        let reason = match &digest {
            Ok(actual) => {
                format!("the setup {} has SHA-256 {actual}, not the verified {}", plan.setup.display(), plan.sha256)
            }
            Err(error) => format!("the setup {} could not be hashed: {error}", plan.setup.display()),
        };
        system.log(&format!("update: not started: {reason}"));
        if let Err(error) = system.remove_file(&plan.setup) {
            system.log(&format!("update: could not remove the bad setup {}: {error}", plan.setup.display()));
        }
        return Outcome::NotStarted { reason };
    }
    // A backup without an install directory is an interrupted rollback: it is the only copy
    // of the app, so it goes back into place instead of being deleted.
    if system.exists(&backup) && !system.exists(&plan.install_dir) {
        system.log(&format!("update: restoring {} from an interrupted update", plan.install_dir.display()));
        if let Err(error) = system.rename(&backup, &plan.install_dir) {
            let reason = format!("the backup {} could not be restored: {error}", backup.display());
            system.log(&format!("update: not started: {reason}"));
            return Outcome::NotStarted { reason };
        }
    }
    if system.exists(&backup) {
        system.log(&format!("update: removing the stale backup {}", backup.display()));
        if let Err(error) = system.remove_dir_all(&backup) {
            let reason = format!("the stale backup {} could not be removed: {error}", backup.display());
            system.log(&format!("update: not started: {reason}"));
            return Outcome::NotStarted { reason };
        }
    }

    // The old app locks its own directory until its process has exited. The rename is the
    // wait: it succeeds only once nothing holds the executable open. When it never does, the
    // old app is still running, so there is nothing to relaunch (that would start a second
    // one); the setup stays for the next quit.
    if !rename_with_retry(system, &plan.install_dir, &backup) {
        let reason = format!("{} is still running", plan.install_dir.join(EXE).display());
        system.log(&format!("update: not started: {reason}"));
        return Outcome::NotStarted { reason };
    }
    system.log(&format!("update: moved {} aside to {}", plan.install_dir.display(), backup.display()));

    let failure = match system.run(&plan.setup, &["/S"], INSTALL_TIMEOUT) {
        Ok(Some(0)) => None,
        Ok(Some(code)) => Some(format!("the setup exited with code {code}")),
        Ok(None) => Some("the setup timed out".to_owned()),
        Err(error) => Some(format!("the setup could not run: {error}")),
    };
    let failure = failure.or_else(|| {
        let exe = plan.install_dir.join(EXE);
        if !system.exists(&exe) {
            return Some(format!("{} is missing after the install", exe.display()));
        }
        match system.run(&exe, &[SELF_TEST_FLAG], SELF_TEST_TIMEOUT) {
            Ok(Some(0)) => None,
            Ok(Some(code)) => Some(format!("{SELF_TEST_FLAG} exited with code {code}")),
            Ok(None) => Some(format!("{SELF_TEST_FLAG} timed out")),
            Err(error) => Some(format!("{SELF_TEST_FLAG} could not run: {error}")),
        }
    });

    if let Some(reason) = failure {
        return rollback(system, plan, &backup, reason);
    }

    system.log(&format!("update: installed {} {}", plan.version, plan.install_dir.display()));
    // The install is good; leftover cleanup failures are worth a line but must not turn a
    // working update into a failure.
    if let Err(error) = system.remove_dir_all(&backup) {
        system.log(&format!("update: could not remove the backup {}: {error}", backup.display()));
    }
    if let Err(error) = system.remove_file(&plan.setup) {
        system.log(&format!("update: could not remove the setup {}: {error}", plan.setup.display()));
    }
    if plan.relaunch {
        relaunch(system, &plan.install_dir);
    }
    Outcome::Installed
}

/// Puts the previous installation back and deletes the setup that failed.
fn rollback(system: &impl System, plan: &Plan, backup: &Path, reason: String) -> Outcome {
    system.log(&format!("update: rolling back: {reason}"));
    if let Err(error) = system.remove_dir_all(&plan.install_dir) {
        system.log(&format!("update: could not clear {}: {error}", plan.install_dir.display()));
    }
    if rename_with_retry(system, backup, &plan.install_dir) {
        system.log(&format!("update: restored {}", plan.install_dir.display()));
        if plan.relaunch {
            relaunch(system, &plan.install_dir);
        }
    } else {
        system.log(&format!("update: could not restore {} from {}", plan.install_dir.display(), backup.display()));
    }
    // A setup that failed once should not be retried silently: the next check downloads it
    // again.
    if let Err(error) = system.remove_file(&plan.setup) {
        system.log(&format!("update: could not remove the setup {}: {error}", plan.setup.display()));
    }
    Outcome::RolledBack { reason }
}

/// Renames `from` to `to`, retrying while Windows holds the directory open.
fn rename_with_retry(system: &impl System, from: &Path, to: &Path) -> bool {
    for attempt in 1..=RENAME_ATTEMPTS {
        match system.rename(from, to) {
            Ok(()) => return true,
            Err(error) => {
                system.log(&format!("update: rename {} ({attempt}/{RENAME_ATTEMPTS}): {error}", from.display()));
                if attempt < RENAME_ATTEMPTS {
                    system.sleep(RENAME_DELAY);
                }
            }
        }
    }
    false
}

/// Starts the app in `install_dir` without waiting for it.
fn relaunch(system: &impl System, install_dir: &Path) {
    let exe = install_dir.join(EXE);
    system.log(&format!("update: relaunching {}", exe.display()));
    if let Err(error) = system.spawn(&exe, &[]) {
        system.log(&format!("update: could not relaunch {}: {error}", exe.display()));
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::HashSet;

    use super::*;

    /// Records every call and models the filesystem as a set of paths. Each test scripts
    /// the outcomes it needs (rename failures, setup/self-test exit codes, and so on).
    struct FakeSystem {
        setup: PathBuf,
        install_dir: PathBuf,
        calls: RefCell<Vec<String>>,
        logs: RefCell<Vec<String>>,
        existing: RefCell<HashSet<PathBuf>>,
        /// Renames that fail before one succeeds (a running app locks the directory).
        rename_failures: Cell<u32>,
        /// What [`System::sha256`] answers; `None` makes the hash fail.
        sha256: RefCell<Option<String>>,
        /// The setup's exit code; `None` is a timeout (`Ok(None)`).
        setup_code: Cell<Option<i32>>,
        setup_error: Cell<bool>,
        /// Whether a successful setup writes `wraex.exe` into the install directory.
        setup_writes_exe: Cell<bool>,
        /// The self-test's exit code; `None` is a timeout.
        self_test_code: Cell<Option<i32>>,
        self_test_error: Cell<bool>,
        sleeps: Cell<u32>,
        spawned: RefCell<Vec<PathBuf>>,
    }

    impl FakeSystem {
        fn new(plan: &Plan) -> Self {
            Self {
                setup: plan.setup.clone(),
                install_dir: plan.install_dir.clone(),
                calls: RefCell::default(),
                logs: RefCell::default(),
                existing: RefCell::default(),
                rename_failures: Cell::new(0),
                sha256: RefCell::new(Some(plan.sha256.clone())),
                setup_code: Cell::new(Some(0)),
                setup_error: Cell::new(false),
                setup_writes_exe: Cell::new(true),
                self_test_code: Cell::new(Some(0)),
                self_test_error: Cell::new(false),
                sleeps: Cell::new(0),
                spawned: RefCell::default(),
            }
        }

        fn add(&self, path: &Path) {
            self.existing.borrow_mut().insert(path.to_owned());
        }

        /// An existing NSIS install: the directory, the executable and the uninstaller.
        fn add_install(&self) {
            self.add(&self.install_dir);
            self.add(&self.install_dir.join(EXE));
            self.add(&self.install_dir.join("Uninstall.exe"));
        }

        fn has(&self, path: &Path) -> bool {
            self.existing.borrow().contains(path)
        }

        fn calls(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }

        fn sleeps(&self) -> u32 {
            self.sleeps.get()
        }

        fn spawned(&self) -> Vec<PathBuf> {
            self.spawned.borrow().clone()
        }
    }

    impl System for FakeSystem {
        fn exists(&self, path: &Path) -> bool {
            self.calls.borrow_mut().push(format!("exists {}", path.display()));
            self.existing.borrow().contains(path)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            self.calls.borrow_mut().push(format!("rename {} -> {}", from.display(), to.display()));
            if self.rename_failures.get() > 0 {
                self.rename_failures.set(self.rename_failures.get() - 1);
                return Err(io::Error::other("in use"));
            }
            let mut existing = self.existing.borrow_mut();
            if !existing.contains(from) {
                return Err(io::Error::new(io::ErrorKind::NotFound, "missing"));
            }
            let moved: Vec<PathBuf> = existing.iter().filter(|path| path.starts_with(from)).cloned().collect();
            existing.remove(from);
            for path in moved {
                existing.remove(&path);
                if let Ok(relative) = path.strip_prefix(from) {
                    existing.insert(to.join(relative));
                }
            }
            existing.insert(to.to_owned());
            Ok(())
        }

        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            self.calls.borrow_mut().push(format!("remove-dir {}", path.display()));
            let mut existing = self.existing.borrow_mut();
            if !existing.iter().any(|entry| entry.starts_with(path)) {
                return Err(io::Error::new(io::ErrorKind::NotFound, "missing"));
            }
            existing.retain(|entry| !entry.starts_with(path));
            Ok(())
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            self.calls.borrow_mut().push(format!("remove-file {}", path.display()));
            if self.existing.borrow_mut().remove(path) {
                Ok(())
            } else {
                Err(io::Error::new(io::ErrorKind::NotFound, "missing"))
            }
        }

        fn sha256(&self, path: &Path) -> io::Result<String> {
            self.calls.borrow_mut().push(format!("sha256 {}", path.display()));
            match &*self.sha256.borrow() {
                Some(digest) => Ok(digest.clone()),
                None => Err(io::Error::other("unreadable")),
            }
        }

        fn run(&self, program: &Path, args: &[&str], _timeout: Duration) -> io::Result<Option<i32>> {
            self.calls.borrow_mut().push(format!("run {} {args:?}", program.display()));
            if program == self.setup {
                if self.setup_error.get() {
                    return Err(io::Error::other("cannot run"));
                }
                let code = self.setup_code.get();
                if code == Some(0) && self.setup_writes_exe.get() {
                    self.add_install();
                }
                return Ok(code);
            }
            if program == self.install_dir.join(EXE) && args == [SELF_TEST_FLAG] {
                if self.self_test_error.get() {
                    return Err(io::Error::other("cannot run"));
                }
                return Ok(self.self_test_code.get());
            }
            Err(io::Error::new(io::ErrorKind::NotFound, "unscripted program"))
        }

        fn spawn(&self, program: &Path, args: &[&str]) -> io::Result<()> {
            self.calls.borrow_mut().push(format!("spawn {} {args:?}", program.display()));
            self.spawned.borrow_mut().push(program.to_owned());
            Ok(())
        }

        fn sleep(&self, _duration: Duration) {
            self.calls.borrow_mut().push("sleep".to_owned());
            self.sleeps.set(self.sleeps.get() + 1);
        }

        fn log(&self, line: &str) {
            self.logs.borrow_mut().push(line.to_owned());
        }
    }

    /// The digest every test plan carries, as the download would have computed it.
    const SHA256: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";

    fn plan(relaunch: bool) -> Plan {
        Plan {
            setup: PathBuf::from("/downloads/wraex 0.2.0/wraex-0.2.0-x86_64-pc-windows-msvc-setup.exe"),
            install_dir: PathBuf::from("/home/user/AppData/Local/Programs/Wraex"),
            version: "0.2.0".to_owned(),
            sha256: SHA256.to_owned(),
            relaunch,
        }
    }

    fn index_of(calls: &[String], needle: &str) -> usize {
        calls.iter().position(|call| call.contains(needle)).unwrap_or_else(|| panic!("no `{needle}` in {calls:?}"))
    }

    #[test]
    fn a_successful_install_replaces_the_directory_and_relaunches() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        let exe = plan.install_dir.join(EXE);
        let backup = backup_dir(&plan.install_dir);

        assert_eq!(apply(&fake, &plan), Outcome::Installed);
        assert_eq!(
            fake.calls(),
            [
                format!("exists {}", plan.setup.display()),
                format!("sha256 {}", plan.setup.display()),
                // Once for an interrupted rollback, once for a stale backup.
                format!("exists {}", backup.display()),
                format!("exists {}", backup.display()),
                format!("rename {} -> {}", plan.install_dir.display(), backup.display()),
                format!("run {} [\"/S\"]", plan.setup.display()),
                format!("exists {}", exe.display()),
                format!("run {} [\"--self-test\"]", exe.display()),
                format!("remove-dir {}", backup.display()),
                format!("remove-file {}", plan.setup.display()),
                format!("spawn {} []", exe.display()),
            ]
        );
        assert!(!fake.has(&backup), "the backup is cleaned up");
        assert!(!fake.has(&plan.setup), "the setup is cleaned up");
        assert!(fake.has(&exe), "the new executable stays");
        assert_eq!(fake.spawned(), [exe]);
    }

    #[test]
    fn an_install_without_relaunch_leaves_the_app_closed() {
        let plan = plan(false);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();

        assert_eq!(apply(&fake, &plan), Outcome::Installed);
        assert!(fake.spawned().is_empty());
    }

    #[test]
    fn the_old_app_gets_a_moment_to_exit() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        fake.rename_failures.set(3);

        assert_eq!(apply(&fake, &plan), Outcome::Installed);
        assert_eq!(fake.sleeps(), 3, "three failed renames, three pauses");
        let renames = fake.calls().iter().filter(|call| call.starts_with("rename ")).count();
        assert_eq!(renames, 4, "the fourth attempt succeeds");
    }

    #[test]
    fn a_running_old_app_leaves_everything_alone() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        fake.rename_failures.set(RENAME_ATTEMPTS);
        let exe = plan.install_dir.join(EXE);

        let outcome = apply(&fake, &plan);
        assert!(matches!(outcome, Outcome::NotStarted { .. }), "{outcome:?}");
        assert_eq!(fake.sleeps(), RENAME_ATTEMPTS - 1, "no pause after the last attempt");
        assert!(fake.has(&plan.install_dir), "the install directory is untouched");
        assert!(fake.has(&exe));
        assert!(fake.has(&plan.setup), "the setup is kept for the next try");
        assert!(!fake.has(&backup_dir(&plan.install_dir)));
        assert!(fake.calls().iter().all(|call| !call.starts_with("run ")), "the setup never ran");
        assert!(fake.spawned().is_empty(), "the old app is still running: no second copy");
    }

    #[test]
    fn a_failed_setup_rolls_back() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        // The installer's `/S` exit code when wraex.exe is still running.
        fake.setup_code.set(Some(2));
        let exe = plan.install_dir.join(EXE);

        let outcome = apply(&fake, &plan);
        match &outcome {
            Outcome::RolledBack { reason } => assert!(reason.contains("code 2"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(fake.has(&plan.install_dir), "the old directory is restored");
        assert!(fake.has(&exe));
        assert!(!fake.has(&backup_dir(&plan.install_dir)));
        assert!(!fake.has(&plan.setup), "a bad setup is deleted");
        assert!(fake.calls().iter().all(|call| !call.contains(SELF_TEST_FLAG)), "no self-test");
        assert_eq!(fake.spawned(), [exe], "the restored app comes back");
    }

    #[test]
    fn a_timing_out_setup_rolls_back() {
        let plan = plan(false);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        fake.setup_code.set(None);

        let outcome = apply(&fake, &plan);
        match &outcome {
            Outcome::RolledBack { reason } => assert!(reason.contains("timed out"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(fake.has(&plan.install_dir.join(EXE)));
        assert!(fake.spawned().is_empty());
    }

    #[test]
    fn a_failed_self_test_rolls_back() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        fake.self_test_code.set(Some(1));
        let exe = plan.install_dir.join(EXE);
        let backup = backup_dir(&plan.install_dir);

        let outcome = apply(&fake, &plan);
        match &outcome {
            Outcome::RolledBack { reason } => assert!(reason.contains(SELF_TEST_FLAG), "{reason}"),
            other => panic!("{other:?}"),
        }
        // The setup replaced the directory, so the restore has to move the backup back.
        let calls = fake.calls();
        let restore = format!("rename {} -> {}", backup.display(), plan.install_dir.display());
        assert!(index_of(&calls, &format!("remove-dir {}", plan.install_dir.display())) < index_of(&calls, &restore));
        assert!(calls.contains(&restore));
        assert!(fake.has(&exe));
        assert!(!fake.has(&backup));
        assert_eq!(fake.spawned(), [exe]);
    }

    #[test]
    fn a_missing_new_executable_rolls_back() {
        let plan = plan(false);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        fake.setup_writes_exe.set(false);

        let outcome = apply(&fake, &plan);
        match &outcome {
            Outcome::RolledBack { reason } => assert!(reason.contains("missing"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(fake.has(&plan.install_dir.join(EXE)), "the old executable is back");
    }

    #[test]
    fn a_stale_backup_is_removed_before_the_rename() {
        let plan = plan(false);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        let backup = backup_dir(&plan.install_dir);
        fake.add(&backup);
        fake.add(&backup.join(EXE));
        fake.add(&backup.join("leftover.txt"));

        assert_eq!(apply(&fake, &plan), Outcome::Installed);
        let calls = fake.calls();
        assert!(index_of(&calls, &format!("remove-dir {}", backup.display())) < index_of(&calls, "rename "));
        assert!(!fake.has(&backup.join("leftover.txt")), "the stale contents are gone");
    }

    /// A rollback cut short (power loss, a killed helper) leaves only `<dir>.previous`: it is
    /// put back, never deleted, and the update then runs as usual.
    #[test]
    fn an_interrupted_rollback_is_restored_first() {
        let plan = plan(false);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        let backup = backup_dir(&plan.install_dir);
        fake.add(&backup);
        fake.add(&backup.join(EXE));

        assert_eq!(apply(&fake, &plan), Outcome::Installed);
        let calls = fake.calls();
        let restore = format!("rename {} -> {}", backup.display(), plan.install_dir.display());
        assert!(index_of(&calls, &restore) < index_of(&calls, "run "));
        let removed = calls.iter().position(|call| *call == format!("remove-dir {}", backup.display()));
        assert!(removed.is_none_or(|at| at > index_of(&calls, "run ")), "the only copy is never deleted first");
        assert!(fake.has(&plan.install_dir.join(EXE)));
    }

    #[test]
    fn a_missing_setup_changes_nothing() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add_install();

        let outcome = apply(&fake, &plan);
        match &outcome {
            Outcome::NotStarted { reason } => assert!(reason.contains("missing"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert_eq!(fake.calls(), [format!("exists {}", plan.setup.display())]);
        assert!(fake.has(&plan.install_dir.join(EXE)));
        assert!(fake.spawned().is_empty());
    }

    /// The setup waited in a user-writable folder between the download and the quit: a file
    /// that no longer has the verified digest must not run, and must not stay there either.
    #[test]
    fn a_swapped_setup_is_refused_and_deleted() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        *fake.sha256.borrow_mut() = Some("00".repeat(32));

        let outcome = apply(&fake, &plan);
        match &outcome {
            Outcome::NotStarted { reason } => assert!(reason.contains("SHA-256"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(!fake.has(&plan.setup), "the bad setup is deleted");
        assert!(fake.has(&plan.install_dir.join(EXE)), "the install is untouched");
        assert!(fake.calls().iter().all(|call| !call.starts_with("rename ") && !call.starts_with("run ")));
        assert!(fake.spawned().is_empty());
    }

    /// A setup that cannot be read at all is as bad as a swapped one.
    #[test]
    fn an_unreadable_setup_is_refused_and_deleted() {
        let plan = plan(true);
        let fake = FakeSystem::new(&plan);
        fake.add(&plan.setup);
        fake.add_install();
        *fake.sha256.borrow_mut() = None;

        let outcome = apply(&fake, &plan);
        match &outcome {
            Outcome::NotStarted { reason } => assert!(reason.contains("could not be hashed"), "{reason}"),
            other => panic!("{other:?}"),
        }
        assert!(!fake.has(&plan.setup));
        assert!(fake.has(&plan.install_dir.join(EXE)));
        assert!(fake.spawned().is_empty());
    }

    #[test]
    fn the_backup_is_a_sibling_with_a_previous_suffix() {
        assert_eq!(
            backup_dir(Path::new(r"C:\Users\me\AppData\Local\Programs\Wraex")),
            PathBuf::from(r"C:\Users\me\AppData\Local\Programs\Wraex.previous")
        );
        assert_eq!(backup_dir(Path::new("/opt/Wraex")), PathBuf::from("/opt/Wraex.previous"));
        assert_eq!(backup_dir(Path::new("Wraex")), PathBuf::from("Wraex.previous"));
    }

    #[test]
    fn helper_args_round_trip_including_spaces() {
        let plan = plan(true);
        let args = helper_args(&plan);
        assert_eq!(
            args,
            [
                "--install-update",
                "/downloads/wraex 0.2.0/wraex-0.2.0-x86_64-pc-windows-msvc-setup.exe",
                "--install-dir",
                "/home/user/AppData/Local/Programs/Wraex",
                "--version",
                "0.2.0",
                "--sha256",
                SHA256,
                "--relaunch",
            ]
        );
        assert_eq!(parse_helper_args(&args), Some(plan));
    }

    #[test]
    fn helper_args_omit_relaunch_when_not_asked() {
        let plan = plan(false);
        let args = helper_args(&plan);
        assert!(!args.iter().any(|arg| arg == "--relaunch"));
        assert_eq!(parse_helper_args(&args), Some(plan));
    }

    #[test]
    fn parse_helper_args_needs_every_value() {
        let args = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        // No `--install-update` at all.
        assert_eq!(parse_helper_args(&args(&["--version", "0.2.0"])), None);
        // No setup after the flag.
        assert_eq!(parse_helper_args(&args(&["--install-update"])), None);
        // No `--install-dir`.
        assert_eq!(parse_helper_args(&args(&["--install-update", "setup.exe", "--version", "0.2.0"])), None);
        // No version.
        assert_eq!(
            parse_helper_args(&args(&["--install-update", "setup.exe", "--install-dir", "dir", "--sha256", SHA256])),
            None
        );
        // No digest: a plan the helper cannot verify is no plan.
        assert_eq!(
            parse_helper_args(&args(&["--install-update", "setup.exe", "--install-dir", "dir", "--version", "1"])),
            None
        );
        // A flag whose value is missing.
        assert_eq!(
            parse_helper_args(&args(&[
                "--install-update",
                "setup.exe",
                "--install-dir",
                "dir",
                "--version",
                "--sha256"
            ])),
            None
        );
        // Empty values count as missing.
        assert_eq!(parse_helper_args(&args(&["--install-update", "", "--install-dir", "dir", "--version", "1"])), None);
        assert_eq!(
            parse_helper_args(&args(&["--install-update", "setup.exe", "--install-dir", "", "--version", "1"])),
            None
        );
        assert_eq!(
            parse_helper_args(&args(&["--install-update", "setup.exe", "--install-dir", "dir", "--version", ""])),
            None
        );
        assert_eq!(
            parse_helper_args(&args(&[
                "--install-update",
                "setup.exe",
                "--install-dir",
                "dir",
                "--version",
                "1",
                "--sha256",
                ""
            ])),
            None
        );
    }

    #[test]
    fn parse_helper_args_ignores_unknown_arguments() {
        let args = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        let plan = parse_helper_args(&args(&[
            "ignored",
            "--install-update",
            "setup.exe",
            "--unknown",
            "value",
            "--install-dir",
            "dir",
            "--version",
            "1.2.3",
            "--sha256",
            SHA256,
        ]))
        .expect("a plan");
        assert_eq!(plan.setup, PathBuf::from("setup.exe"));
        assert_eq!(plan.install_dir, PathBuf::from("dir"));
        assert_eq!(plan.version, "1.2.3");
        assert_eq!(plan.sha256, SHA256);
        assert!(!plan.relaunch);
    }
}
