//! The update worker: the check, the download, and the answers for the app.
//!
//! Upstream's `AppUpdater.ts` asks electron-updater at launch and every
//! [`CHECK_INTERVAL_MS`]; electron-updater also downloads and installs. Wraex keeps the same
//! shape with three pieces ([`wraex_core::updates`]): [`updates::check_source`](wraex_core::updates::check_source)
//! asks whether a newer release exists, [`download::download`] fetches its setup and verifies
//! the release's SHA-256, and the quit hands the verified file to
//! [`wraex_core::updates::install`] through `wraex_platform::updater`.
//!
//! The worker runs one [`cycle`], then waits on a channel: [`CHECK_INTERVAL_MS`] passes, or
//! [`Updates::check_now`] wakes it for the About page's button. Nothing polls, and an idle app
//! wakes only when the interval ends or a check is asked for. The process ending stops it.
//!
//! Once something is downloaded the worker keeps its schedule: a newer release replaces the
//! download, and [`download::download`] deletes the older setup as stale.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use wraex_core::trade::api::Transport;
use wraex_core::updates::download::{self, Downloaded, WINDOWS_TARGET};
use wraex_core::updates::source::{GitHubReleases, ReleaseSource};
use wraex_core::updates::{self, NoDownloadReason, UpdateInfo};

/// How often upstream checks (`setInterval`, 16 hours).
pub use wraex_core::updates::CHECK_INTERVAL_MS;

/// The latest answer and the verified download, shared with the app: the worker writes them, a
/// settings window that opens later reads them, and the quit takes the download.
#[derive(Debug, Clone, Default)]
pub struct Updates {
    latest: Arc<Mutex<Option<UpdateInfo>>>,
    ready: Arc<Mutex<Option<Downloaded>>>,
    /// Wakes the worker for [`Updates::check_now`]; `None` on a shell that never spawned one.
    wake: Option<Arc<mpsc::Sender<()>>>,
}

impl Updates {
    /// The newest answer, if the worker has one the app has not applied yet.
    pub fn take(&self) -> Option<UpdateInfo> {
        self.latest.lock().ok().and_then(|mut latest| latest.take())
    }

    /// The answer as it stands, without consuming it.
    pub fn latest(&self) -> Option<UpdateInfo> {
        self.latest.lock().ok().and_then(|latest| latest.clone())
    }

    /// The verified setup waiting for the next quit, without consuming it.
    pub fn ready(&self) -> Option<Downloaded> {
        self.ready.lock().ok().and_then(|ready| ready.clone())
    }

    /// Takes the verified setup: the install hands it to the helper exactly once.
    pub fn take_ready(&self) -> Option<Downloaded> {
        self.ready.lock().ok().and_then(|mut ready| ready.take())
    }

    /// The About page's "Check now": wake the sleeping worker, which checks straight away.
    pub fn check_now(&self) {
        if let Some(wake) = &self.wake {
            let _ = wake.send(());
        }
    }

    /// Start the worker: one cycle now, then one per [`CHECK_INTERVAL_MS`] or whenever
    /// [`Updates::check_now`] asks. `on_landed` runs on the worker thread with every state the
    /// page shows; the app's is an `AppHandle::update`, so its copies change on the UI thread.
    /// `dir` is where a download goes (`wraex_platform::updater::updates_dir`); without one
    /// the worker still checks but never downloads.
    pub fn spawn(
        on_landed: impl Fn(UpdateInfo) + Send + Sync + 'static,
        no_download: Option<NoDownloadReason>,
        dir: Option<PathBuf>,
    ) -> Self {
        let (wake, requests) = mpsc::channel();
        let updates = Self { wake: Some(Arc::new(wake)), ..Self::default() };
        let worker = updates.clone();
        std::thread::Builder::new()
            .name("wraex-updates".to_owned())
            .spawn(move || {
                let transport = wraex_platform::http::HttpTransport::default();
                let source = GitHubReleases::new(&transport);
                loop {
                    let downloaded =
                        cycle(&source, env!("CARGO_PKG_VERSION"), no_download, dir.as_deref(), now(), &mut |info| {
                            if let Ok(mut latest) = worker.latest.lock() {
                                *latest = Some(info.clone());
                            }
                            on_landed(info);
                        });
                    if let Some(downloaded) = downloaded
                        && let Ok(mut ready) = worker.ready.lock()
                    {
                        *ready = Some(downloaded);
                    }
                    match requests.recv_timeout(Duration::from_millis(CHECK_INTERVAL_MS)) {
                        // The interval passed or the About page asked: check again.
                        Ok(()) | Err(RecvTimeoutError::Timeout) => {}
                        // The `Updates` is gone: nothing can read another answer.
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .expect("start the update worker");
        updates
    }
}

/// One check, with the wall clock.
pub fn check(transport: &impl Transport, no_download: Option<NoDownloadReason>) -> UpdateInfo {
    updates::check(transport, now(), no_download)
}

/// One worker cycle: check, publish the answer, and download a newer release when Wraex may
/// install it itself. Every state the About page shows goes through `publish`, in order; the
/// verified download is returned for [`Updates::ready`] (and the next quit).
///
/// A failed download publishes [`UpdateInfo::Error`]: upstream's `error` event covers the
/// download too, so the page says the check did not work rather than claiming a download that
/// will never arrive. An older download that is already `ready` stays as it is.
pub fn cycle(
    source: &impl ReleaseSource,
    current: &str,
    no_download: Option<NoDownloadReason>,
    dir: Option<&Path>,
    now: u64,
    publish: &mut dyn FnMut(UpdateInfo),
) -> Option<Downloaded> {
    let (info, release) = updates::check_source(source, current, no_download, now);
    let downloadable = matches!(&info, UpdateInfo::UpdateAvailable { no_download_reason: None, .. });
    publish(info);
    let (Some(release), Some(dir)) = (release, dir) else { return None };
    if !downloadable {
        return None;
    }
    match download::download(source, &release, WINDOWS_TARGET, dir) {
        Ok(downloaded) => {
            publish(UpdateInfo::UpdateDownloaded { version: downloaded.version.clone() });
            Some(downloaded)
        }
        Err(error) => {
            wraex_platform::log!("wraex: the update download failed: {error}");
            publish(UpdateInfo::Error { checked_at: now });
            None
        }
    }
}

/// Whether a check is due: the first one always is, then one per interval.
pub fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|last| now.saturating_sub(last) >= CHECK_INTERVAL_MS)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io::Write;

    use sha2::{Digest, Sha256};

    use super::*;
    use wraex_core::trade::api::{HttpRequest, HttpResponse};
    use wraex_core::updates::source::{Asset, Release};

    struct Fixed(HttpResponse);

    impl Transport for Fixed {
        fn send(&self, _: &HttpRequest) -> Result<HttpResponse, String> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn the_first_check_is_due_then_one_per_interval() {
        assert!(due(None, 0));
        assert!(!due(Some(0), CHECK_INTERVAL_MS - 1));
        assert!(due(Some(0), CHECK_INTERVAL_MS));
        assert!(!due(Some(CHECK_INTERVAL_MS), CHECK_INTERVAL_MS + 1));
    }

    /// A landing answer is kept for the About page and handed out once.
    #[test]
    fn a_landed_answer_is_shared_once() {
        let status = 200;
        let body = r#"{"tag_name":"v0.0.1"}"#;
        let info =
            updates::check(&Fixed(HttpResponse { status, body: body.to_owned(), ..Default::default() }), 7, None);
        // `0.0.1` is older than this crate's version, so nothing is offered.
        assert!(matches!(info, UpdateInfo::UpdateNotAvailable { checked_at: 7 }));

        let updates = Updates::default();
        *updates.latest.lock().unwrap() = Some(info.clone());
        assert_eq!(updates.latest(), Some(info.clone()));
        assert_eq!(updates.take(), Some(info));
        assert_eq!(updates.take(), None);
    }

    /// The downloaded setup waits for the next quit: reading it does not consume it, and an
    /// install takes it exactly once.
    #[test]
    fn a_ready_download_is_kept_until_it_is_taken() {
        let downloaded = Downloaded {
            version: "0.2.0".to_owned(),
            path: PathBuf::from("/updates/wraex-0.2.0-x86_64-pc-windows-msvc-setup.exe"),
            sha256: "ab".repeat(32),
        };
        let updates = Updates { ready: Arc::new(Mutex::new(Some(downloaded.clone()))), ..Updates::default() };
        assert_eq!(updates.ready(), Some(downloaded.clone()));
        assert_eq!(updates.ready(), Some(downloaded.clone()), "reading it leaves it in place");
        assert_eq!(updates.take_ready(), Some(downloaded));
        assert_eq!(updates.take_ready(), None);
        assert_eq!(updates.ready(), None);
    }

    /// "Check now" wakes the worker through its channel instead of waiting out the interval.
    #[test]
    fn check_now_wakes_the_worker() {
        let (wake, requests) = mpsc::channel();
        let updates = Updates { wake: Some(Arc::new(wake)), ..Updates::default() };
        updates.check_now();
        assert_eq!(requests.try_recv(), Ok(()));
    }

    /// The setup's bytes the fake release serves.
    const BYTES: &[u8] = b"the setup's bytes";

    fn digest(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    /// A release source answering from memory: the newest release, and the setup's bytes. No
    /// network, and the digest the assets carry is a real SHA-256 of `bytes`.
    struct FakeSource {
        latest: Result<Option<Release>, String>,
        bytes: Vec<u8>,
        downloads: Cell<usize>,
    }

    impl FakeSource {
        fn offering(release: Release, bytes: &[u8]) -> Self {
            Self { latest: Ok(Some(release)), bytes: bytes.to_vec(), downloads: Cell::new(0) }
        }

        fn failing(error: &'static str) -> Self {
            Self { latest: Err(error.to_owned()), bytes: Vec::new(), downloads: Cell::new(0) }
        }

        fn downloads(&self) -> usize {
            self.downloads.get()
        }
    }

    impl ReleaseSource for FakeSource {
        fn latest(&self) -> Result<Option<Release>, String> {
            self.latest.clone()
        }

        fn download(&self, _: &Asset, sink: &mut dyn Write) -> Result<u64, String> {
            self.downloads.set(self.downloads.get() + 1);
            sink.write_all(&self.bytes).map_err(|error| error.to_string())?;
            Ok(self.bytes.len() as u64)
        }
    }

    /// The `0.2.0` release whose setup asset carries `sha256`.
    fn release(sha256: Option<String>) -> Release {
        let version = "0.2.0";
        Release {
            version: version.to_owned(),
            assets: vec![Asset {
                name: download::setup_name(version, WINDOWS_TARGET),
                url: "https://github.com/Dumspy/wraex/releases/download/v0.2.0/setup.exe".into(),
                size: BYTES.len() as u64,
                sha256,
            }],
        }
    }

    /// A directory of this test's own under the temp dir; a rerun starts empty.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wraex-updates-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// Runs one cycle and collects what it published.
    fn run(
        source: &FakeSource,
        no_download: Option<NoDownloadReason>,
        dir: Option<&Path>,
    ) -> (Vec<UpdateInfo>, Option<Downloaded>) {
        let mut published = Vec::new();
        let downloaded = cycle(source, "0.1.0", no_download, dir, 42, &mut |info| published.push(info));
        (published, downloaded)
    }

    #[test]
    fn a_newer_release_is_downloaded_in_the_background() {
        let dir = temp_dir("downloaded");
        let source = FakeSource::offering(release(Some(digest(BYTES))), BYTES);

        let (published, downloaded) = run(&source, None, Some(&dir));

        let downloaded = downloaded.expect("the setup");
        assert_eq!(downloaded.version, "0.2.0");
        assert_eq!(downloaded.sha256, digest(BYTES));
        assert_eq!(std::fs::read(&downloaded.path).unwrap(), BYTES, "the verified setup is on disk");
        assert_eq!(
            published,
            [
                UpdateInfo::UpdateAvailable { version: "0.2.0".into(), no_download_reason: None },
                UpdateInfo::UpdateDownloaded { version: "0.2.0".into() },
            ]
        );
        assert_eq!(source.downloads(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_not_supported_platform_only_reports_the_update() {
        let dir = temp_dir("not-supported");
        let source = FakeSource::offering(release(Some(digest(BYTES))), BYTES);

        let (published, downloaded) = run(&source, Some(NoDownloadReason::NotSupported), Some(&dir));

        assert_eq!(downloaded, None);
        assert_eq!(
            published,
            [UpdateInfo::UpdateAvailable {
                version: "0.2.0".into(),
                no_download_reason: Some(NoDownloadReason::NotSupported),
            }]
        );
        assert_eq!(source.downloads(), 0, "nothing is fetched");
        assert!(!dir.exists(), "not even the downloads directory is made");
    }

    #[test]
    fn a_bad_checksum_is_an_error_and_leaves_no_download() {
        let dir = temp_dir("checksum");
        let source = FakeSource::offering(release(Some("00".repeat(32))), BYTES);

        let (published, downloaded) = run(&source, None, Some(&dir));

        assert_eq!(downloaded, None);
        assert_eq!(
            published,
            [
                UpdateInfo::UpdateAvailable { version: "0.2.0".into(), no_download_reason: None },
                UpdateInfo::Error { checked_at: 42 },
            ],
            "a failed download is the error state, as upstream's error event"
        );
        assert_eq!(source.downloads(), 1, "the download was tried");
        let files: Vec<_> =
            std::fs::read_dir(&dir).map(|entries| entries.flatten().map(|e| e.path()).collect()).unwrap_or_default();
        assert!(files.is_empty(), "the corrupt bytes are deleted: {files:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_release_that_is_not_newer_is_not_downloaded() {
        let dir = temp_dir("not-newer");
        let mut older = release(Some(digest(BYTES)));
        older.version = "0.1.0".into();
        let name = download::setup_name("0.1.0", WINDOWS_TARGET);
        older.assets[0].name = name;
        let source = FakeSource::offering(older, BYTES);

        let (published, downloaded) = run(&source, None, Some(&dir));

        assert_eq!(downloaded, None);
        assert_eq!(published, [UpdateInfo::UpdateNotAvailable { checked_at: 42 }]);
        assert_eq!(source.downloads(), 0);
        assert!(!dir.exists());
    }

    #[test]
    fn a_failed_check_is_an_error_and_downloads_nothing() {
        let dir = temp_dir("source-error");
        let source = FakeSource::failing("offline");

        let (published, downloaded) = run(&source, None, Some(&dir));

        assert_eq!(downloaded, None);
        assert_eq!(published, [UpdateInfo::Error { checked_at: 42 }]);
        assert_eq!(source.downloads(), 0);
    }

    /// A download without a directory to put it in still reports the release; the next quit
    /// simply has nothing to install.
    #[test]
    fn without_an_updates_directory_the_release_is_only_reported() {
        let source = FakeSource::offering(release(Some(digest(BYTES))), BYTES);

        let (published, downloaded) = run(&source, None, None);

        assert_eq!(downloaded, None);
        assert_eq!(published.len(), 1);
        assert!(matches!(&published[0], UpdateInfo::UpdateAvailable { .. }));
        assert_eq!(source.downloads(), 0);
    }
}
