//! The startup update check and its 16-hour repeat, off the UI thread.
//!
//! Upstream's `AppUpdater.ts` asks electron-updater at launch and every
//! [`CHECK_INTERVAL_MS`]. Wraex is notify-only (`wraex_core::updates`), so the whole job is
//! one HTTPS request per interval: a worker does it and hands each answer to the app, which
//! puts it on the settings window's About page (`Host.updateInfo`).
//!
//! The worker sleeps between checks, so nothing polls and an idle app wakes only when an
//! answer lands. The process ending is what stops it.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use wraex_core::trade::api::Transport;
use wraex_core::updates::{self, NoDownloadReason, UpdateInfo};

/// How often upstream checks (`setInterval`, 16 hours).
pub use wraex_core::updates::CHECK_INTERVAL_MS;

/// The latest answer, shared with the app: the worker writes it, a settings window that opens
/// later reads it.
#[derive(Debug, Clone, Default)]
pub struct Updates {
    latest: Arc<Mutex<Option<UpdateInfo>>>,
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

    /// Start the worker: one check now, then one per [`CHECK_INTERVAL_MS`]. `on_landed` runs
    /// on the worker thread; the app's is a `AppHandle::update`, so its state changes on the
    /// UI thread.
    pub fn spawn(
        on_landed: impl Fn(UpdateInfo) + Send + Sync + 'static,
        no_download: Option<NoDownloadReason>,
    ) -> Self {
        let updates = Self::default();
        let worker = updates.clone();
        std::thread::Builder::new()
            .name("wraex-updates".to_owned())
            .spawn(move || {
                loop {
                    let info = check(&wraex_platform::http::HttpTransport::default(), no_download);
                    if let Ok(mut latest) = worker.latest.lock() {
                        *latest = Some(info.clone());
                    }
                    on_landed(info);
                    std::thread::sleep(std::time::Duration::from_millis(CHECK_INTERVAL_MS));
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

/// Whether a check is due: the first one always is, then one per interval.
pub fn due(last: Option<u64>, now: u64) -> bool {
    last.is_none_or(|last| now.saturating_sub(last) >= CHECK_INTERVAL_MS)
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|elapsed| elapsed.as_millis() as u64).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wraex_core::trade::api::{HttpRequest, HttpResponse};

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
}
