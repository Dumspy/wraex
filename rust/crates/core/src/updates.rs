//! The update check: upstream's `AppUpdater.ts` runs electron-updater, which
//! downloads and installs the new version. Wraex is notify-only for now: it asks
//! GitHub for this repository's latest release and reports whether it is newer
//! (self-update is Dumspy/wraex#3).
//!
//! [`UpdateInfo`] mirrors the `UpdateInfo` union of `ipc/types.ts`; every
//! [`check`] is one request through the app's [`Transport`].

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::trade::api::{HttpRequest, Method, Transport};
use crate::trade::rate_limit::Millis;

/// The update API's host. Not part of [`crate::trade::api::ALLOWED_HOSTS`].
pub const UPDATE_HOST: &str = "api.github.com";

/// How often upstream checks (`setInterval`, 16 hours).
pub const CHECK_INTERVAL_MS: u64 = 16 * 60 * 60 * 1000;

/// Why Wraex cannot download an update itself (`noDownloadReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoDownloadReason {
    NotSupported,
    DisabledByFlag,
}

/// `UpdateInfo` (`ipc/types.ts`): the state of the update check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "kebab-case")]
pub enum UpdateInfo {
    Initial,
    CheckingForUpdate,
    #[serde(rename_all = "camelCase")]
    UpdateAvailable {
        version: String,
        no_download_reason: Option<NoDownloadReason>,
    },
    /// Kept for shape parity with upstream; Wraex never downloads.
    #[serde(rename_all = "camelCase")]
    UpdateDownloaded {
        version: String,
    },
    #[serde(rename_all = "camelCase")]
    UpdateNotAvailable {
        checked_at: Millis,
    },
    #[serde(rename_all = "camelCase")]
    Error {
        checked_at: Millis,
    },
}

/// The latest release, `GET /repos/Dumspy/wraex/releases/latest`. The transport
/// adds the User-Agent GitHub requires.
pub fn latest_release_request() -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: format!("https://{UPDATE_HOST}/repos/Dumspy/wraex/releases/latest"),
        headers: vec![("accept", "application/vnd.github+json".to_owned())],
        body: None,
    }
}

/// Why Wraex cannot download: always [`NoDownloadReason::NotSupported`], or
/// [`NoDownloadReason::DisabledByFlag`] after `--no-updates`.
pub fn no_download_reason(args: &[String]) -> Option<NoDownloadReason> {
    Some(if args.iter().any(|arg| arg == "--no-updates") {
        NoDownloadReason::DisabledByFlag
    } else {
        NoDownloadReason::NotSupported
    })
}

/// `MAJOR.MINOR.PATCH`, then an optional `-prerelease`.
struct Version<'a> {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<&'a str>,
}

impl<'a> Version<'a> {
    fn parse(text: &'a str) -> Option<Self> {
        let (core, prerelease) = match text.split_once('-') {
            Some((core, pre)) if !pre.is_empty() => (core, Some(pre)),
            Some(_) => return None,
            None => (text, None),
        };
        let mut parts = core.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self { major, minor, patch, prerelease })
    }
}

/// Whether `latest` is a newer version than `current`. A prerelease sorts below
/// the version it belongs to; prereleases compare as plain strings.
fn is_newer(latest: &str, current: &str) -> Option<bool> {
    let latest = Version::parse(latest)?;
    let current = Version::parse(current)?;
    let order = (latest.major, latest.minor, latest.patch).cmp(&(current.major, current.minor, current.patch));
    Some(match order {
        Ordering::Greater => true,
        Ordering::Less => false,
        Ordering::Equal => match (latest.prerelease, current.prerelease) {
            (None, None) => false,
            (None, Some(_)) => true,
            (Some(_), None) => false,
            (Some(latest), Some(current)) => latest > current,
        },
    })
}

/// Turns a `/releases/latest` response into [`UpdateInfo`]. `current` is the
/// running version; a missing release (404) means there is nothing to offer.
pub fn parse_latest_release(
    status: u16,
    body: &str,
    current: &str,
    no_download_reason: Option<NoDownloadReason>,
    now: Millis,
) -> UpdateInfo {
    #[derive(Deserialize)]
    struct Release {
        tag_name: String,
    }

    match status {
        200 => {
            let Ok(release) = serde_json::from_str::<Release>(body) else {
                return UpdateInfo::Error { checked_at: now };
            };
            let tag = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
            match is_newer(tag, current) {
                Some(true) => UpdateInfo::UpdateAvailable { version: tag.to_owned(), no_download_reason },
                Some(false) => UpdateInfo::UpdateNotAvailable { checked_at: now },
                None => UpdateInfo::Error { checked_at: now },
            }
        }
        404 => UpdateInfo::UpdateNotAvailable { checked_at: now },
        _ => UpdateInfo::Error { checked_at: now },
    }
}

/// Checks the latest release once. A failed request is an [`UpdateInfo::Error`]
/// like a bad answer.
pub fn check(transport: &impl Transport, now: Millis, no_download_reason: Option<NoDownloadReason>) -> UpdateInfo {
    match transport.send(&latest_release_request()) {
        Ok(response) => {
            parse_latest_release(response.status, &response.body, env!("CARGO_PKG_VERSION"), no_download_reason, now)
        }
        Err(_) => UpdateInfo::Error { checked_at: now },
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use serde_json::json;

    use super::*;
    use crate::trade::api::HttpResponse;

    fn release(tag: &str) -> String {
        json!({ "tag_name": tag, "name": tag }).to_string()
    }

    fn parsed(status: u16, body: &str, current: &str) -> UpdateInfo {
        parse_latest_release(status, body, current, Some(NoDownloadReason::NotSupported), 42)
    }

    #[test]
    fn newer_tags_are_available() {
        assert_eq!(
            parsed(200, &release("v0.2.0"), "0.1.0"),
            UpdateInfo::UpdateAvailable {
                version: "0.2.0".into(),
                no_download_reason: Some(NoDownloadReason::NotSupported)
            }
        );
        // Compared as numbers, not strings.
        assert_eq!(
            parsed(200, &release("v0.10.0"), "0.9.0"),
            UpdateInfo::UpdateAvailable {
                version: "0.10.0".into(),
                no_download_reason: Some(NoDownloadReason::NotSupported)
            }
        );
        assert!(matches!(parsed(200, &release("1.0.0"), "0.9.9"), UpdateInfo::UpdateAvailable { .. }));
    }

    #[test]
    fn equal_and_older_tags_are_not_available() {
        assert_eq!(parsed(200, &release("v0.1.0"), "0.1.0"), UpdateInfo::UpdateNotAvailable { checked_at: 42 });
        assert_eq!(parsed(200, &release("v0.1.0"), "0.2.0"), UpdateInfo::UpdateNotAvailable { checked_at: 42 });
    }

    #[test]
    fn a_prerelease_sorts_below_its_version() {
        assert_eq!(parsed(200, &release("v0.2.0-rc.1"), "0.2.0"), UpdateInfo::UpdateNotAvailable { checked_at: 42 });
        assert_eq!(
            parsed(200, &release("v0.2.0"), "0.2.0-rc.1"),
            UpdateInfo::UpdateAvailable {
                version: "0.2.0".into(),
                no_download_reason: Some(NoDownloadReason::NotSupported)
            }
        );
        assert!(matches!(parsed(200, &release("v0.2.0-rc.2"), "0.2.0-rc.1"), UpdateInfo::UpdateAvailable { .. }));
    }

    #[test]
    fn missing_releases_and_bad_answers() {
        assert_eq!(parsed(404, "{}", "0.1.0"), UpdateInfo::UpdateNotAvailable { checked_at: 42 });
        assert_eq!(parsed(500, "oops", "0.1.0"), UpdateInfo::Error { checked_at: 42 });
        assert_eq!(parsed(200, "not json", "0.1.0"), UpdateInfo::Error { checked_at: 42 });
        assert_eq!(parsed(200, "{}", "0.1.0"), UpdateInfo::Error { checked_at: 42 });
        // Tags semver cannot read.
        assert_eq!(parsed(200, &release("nightly"), "0.1.0"), UpdateInfo::Error { checked_at: 42 });
        assert_eq!(parsed(200, &release("v0.2"), "0.1.0"), UpdateInfo::Error { checked_at: 42 });
    }

    #[test]
    fn update_infos_serialise_like_the_reference() {
        assert_eq!(serde_json::to_value(UpdateInfo::Initial).unwrap(), json!({ "state": "initial" }));
        assert_eq!(
            serde_json::to_value(UpdateInfo::CheckingForUpdate).unwrap(),
            json!({ "state": "checking-for-update" })
        );
        assert_eq!(
            serde_json::to_value(UpdateInfo::UpdateAvailable {
                version: "0.2.0".into(),
                no_download_reason: Some(NoDownloadReason::DisabledByFlag),
            })
            .unwrap(),
            json!({ "state": "update-available", "version": "0.2.0", "noDownloadReason": "disabled-by-flag" })
        );
        assert_eq!(
            serde_json::to_value(UpdateInfo::UpdateNotAvailable { checked_at: 1_700_000_000_000 }).unwrap(),
            json!({ "state": "update-not-available", "checkedAt": 1_700_000_000_000u64 })
        );
    }

    /// Answers every request with the configured status and body.
    struct FakeGitHub {
        status: u16,
        body: String,
        sent: RefCell<Vec<HttpRequest>>,
    }

    impl Transport for FakeGitHub {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            self.sent.borrow_mut().push(request.clone());
            Ok(HttpResponse { status: self.status, body: self.body.clone(), ..Default::default() })
        }
    }

    fn github(status: u16, body: String) -> FakeGitHub {
        FakeGitHub { status, body, sent: RefCell::new(Vec::new()) }
    }

    #[test]
    fn check_asks_github_for_the_latest_release() {
        let github = github(200, release("v0.2.0"));
        let info = check(&github, 42, Some(NoDownloadReason::NotSupported));
        assert_eq!(
            info,
            UpdateInfo::UpdateAvailable {
                version: "0.2.0".into(),
                no_download_reason: Some(NoDownloadReason::NotSupported)
            }
        );
        let sent = &github.sent.borrow()[0];
        assert_eq!(sent.url, "https://api.github.com/repos/Dumspy/wraex/releases/latest");
        assert_eq!(sent.method, Method::Get);
        assert_eq!(sent.headers, [("accept", "application/vnd.github+json".to_owned())]);
        assert_eq!(sent.body, None);
    }

    #[test]
    fn a_failed_request_is_an_error() {
        struct Offline;
        impl Transport for Offline {
            fn send(&self, _: &HttpRequest) -> Result<HttpResponse, String> {
                Err("offline".into())
            }
        }

        assert_eq!(check(&Offline, 42, None), UpdateInfo::Error { checked_at: 42 });
    }

    #[test]
    fn no_download_reason_follows_the_flag() {
        let args = |args: &[&str]| args.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(no_download_reason(&args(&[])), Some(NoDownloadReason::NotSupported));
        assert_eq!(no_download_reason(&args(&["wraex", "--no-updates"])), Some(NoDownloadReason::DisabledByFlag));
    }
}
