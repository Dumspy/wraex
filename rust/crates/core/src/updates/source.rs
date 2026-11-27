//! Where releases come from. electron-updater reads a provider (`publish:` in upstream's
//! `electron-builder.yml`, GitHub for both references); Wraex puts the same question behind
//! [`ReleaseSource`], so the check, the download and the tests do not care whether the
//! releases live on GitHub, on another repository or on a static host.
//!
//! [`GitHubReleases`] is the real source: it reuses the update check's `/releases/latest`
//! request ([`crate::updates::latest_release_request`]), so one [`Transport`] serves both
//! the check and this trait, and it downloads through a [`Fetch`].

use std::io::Write;

use serde::Deserialize;

use crate::trade::api::Transport;

/// One published release: its version (no leading `v`) and its files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub assets: Vec<Asset>,
}

/// One file of a [`Release`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    /// The file name, e.g. `wraex-0.2.0-x86_64-pc-windows-msvc-setup.exe`.
    pub name: String,
    /// Where [`ReleaseSource::download`] fetches it from.
    pub url: String,
    /// The size in bytes the source announced.
    pub size: u64,
    /// The SHA-256 of the file as 64 lower-case hex digits, when the source publishes one.
    /// A setup without one is never installed.
    pub sha256: Option<String>,
}

/// A place that publishes releases.
pub trait ReleaseSource {
    /// The newest published release. `Ok(None)` means there is none to see (GitHub answers
    /// 404 for a repository without releases, and for a private one without a token).
    fn latest(&self) -> Result<Option<Release>, String>;

    /// Writes `asset`'s bytes to `sink` and returns how many were written.
    fn download(&self, asset: &Asset, sink: &mut dyn Write) -> Result<u64, String>;
}

/// A binary `GET` streamed to a sink, which is what a download needs on top of the text
/// [`crate::trade::api::Transport`]: the app's is `wraex_platform::http::HttpTransport`.
pub trait Fetch {
    /// Writes the 2xx body of `url` to `sink` and returns the byte count. Any other status is
    /// an error.
    fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u64, String>;
}

/// This repository's releases, read through the app's [`Transport`] and downloaded through
/// its [`Fetch`]. Kept generic so the tests (and a replay run) can answer without a network.
pub struct GitHubReleases<'a, T> {
    transport: &'a T,
}

/// Debug without `T: Debug`: the transport may hold a cookie jar, and a source has
/// nothing of its own to print.
impl<T> std::fmt::Debug for GitHubReleases<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GitHubReleases").finish_non_exhaustive()
    }
}

impl<'a, T> GitHubReleases<'a, T> {
    /// Reads releases and files through `transport`.
    pub fn new(transport: &'a T) -> Self {
        Self { transport }
    }
}

/// The tag as the version Wraex reports it: GitHub tags releases `v0.2.0`, and every
/// update string upstream shows has no `v`. One place, so the check and the source agree.
pub(crate) fn version_from_tag(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

/// The parts of `/releases/latest` a [`Release`] needs. GitHub sends more fields; serde
/// ignores them.
#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
    /// Absent for a release with no files, which is still a release (and still newer).
    #[serde(default)]
    assets: Vec<LatestAsset>,
}

/// One element of `assets`.
#[derive(Deserialize)]
struct LatestAsset {
    name: String,
    browser_download_url: String,
    /// GitHub always sends a size; `0` means "not announced", which the download accepts.
    #[serde(default)]
    size: u64,
    /// `"sha256:<hex>"` as GitHub sends since 2025, or `null` for an older asset.
    #[serde(default)]
    digest: Option<String>,
}

/// The hex half of a `sha256:` digest, lower-cased; `None` for a missing or other kind.
fn sha256_digest(digest: Option<&str>) -> Option<String> {
    let hex = digest?.strip_prefix("sha256:")?;
    Some(hex.to_ascii_lowercase())
}

impl<T: Transport + Fetch> GitHubReleases<'_, T> {
    /// The latest release, or `Ok(None)` when GitHub says there is none.
    fn latest_release(&self) -> Result<Option<Release>, String> {
        let response = self.transport.send(&super::latest_release_request())?;
        match response.status {
            200 => {
                let parsed: LatestRelease =
                    serde_json::from_str(&response.body).map_err(|error| format!("bad release JSON: {error}"))?;
                let assets = parsed
                    .assets
                    .into_iter()
                    .map(|asset| Asset {
                        name: asset.name,
                        url: asset.browser_download_url,
                        size: asset.size,
                        sha256: sha256_digest(asset.digest.as_deref()),
                    })
                    .collect();
                Ok(Some(Release { version: version_from_tag(&parsed.tag_name).to_owned(), assets }))
            }
            404 => Ok(None),
            status => Err(format!("{status} from the release API")),
        }
    }
}

impl<T: Transport + Fetch> ReleaseSource for GitHubReleases<'_, T> {
    fn latest(&self) -> Result<Option<Release>, String> {
        self.latest_release()
    }

    fn download(&self, asset: &Asset, sink: &mut dyn Write) -> Result<u64, String> {
        self.transport.fetch(&asset.url, sink)
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use serde_json::json;

    use super::*;
    use crate::trade::api::{HttpRequest, HttpResponse, Method};

    /// Answers the check with a fixed status and body, and serves fixed bytes to a
    /// download; records both so the tests can see which URL was asked for.
    struct FakeGitHub {
        status: u16,
        body: String,
        file: Vec<u8>,
        sent: RefCell<Vec<HttpRequest>>,
        fetched: RefCell<Vec<String>>,
    }

    impl FakeGitHub {
        fn new(status: u16, body: String) -> Self {
            Self { status, body, file: Vec::new(), sent: RefCell::new(Vec::new()), fetched: RefCell::new(Vec::new()) }
        }
    }

    /// No network at all.
    struct Offline;

    impl Transport for Offline {
        fn send(&self, _: &HttpRequest) -> Result<HttpResponse, String> {
            Err("offline".into())
        }
    }

    impl Fetch for Offline {
        fn fetch(&self, _: &str, _: &mut dyn Write) -> Result<u64, String> {
            Err("offline".into())
        }
    }

    impl Transport for FakeGitHub {
        fn send(&self, request: &HttpRequest) -> Result<HttpResponse, String> {
            self.sent.borrow_mut().push(request.clone());
            Ok(HttpResponse { status: self.status, body: self.body.clone(), ..Default::default() })
        }
    }

    impl Fetch for FakeGitHub {
        fn fetch(&self, url: &str, sink: &mut dyn Write) -> Result<u64, String> {
            self.fetched.borrow_mut().push(url.to_owned());
            sink.write_all(&self.file).map_err(|error| error.to_string())?;
            Ok(self.file.len() as u64)
        }
    }

    #[test]
    fn the_latest_release_lists_its_assets() {
        let body = json!({
            "tag_name": "v0.2.0",
            "assets": [
                {
                    "name": "wraex-0.2.0-x86_64-pc-windows-msvc-setup.exe",
                    "browser_download_url": "https://github.com/Dumspy/wraex/releases/download/v0.2.0/setup.exe",
                    "size": 42,
                    "digest": format!("sha256:{}", "AB".repeat(32)),
                },
                {
                    "name": "wraex-0.2.0-x86_64-pc-windows-msvc.zip",
                    "browser_download_url": "https://github.com/Dumspy/wraex/releases/download/v0.2.0/zip",
                    "size": 7,
                    "digest": null,
                },
                {
                    "name": "other.txt",
                    "browser_download_url": "https://github.com/Dumspy/wraex/releases/download/v0.2.0/other.txt",
                    "size": 1,
                    "digest": "md5:abc",
                },
            ],
        })
        .to_string();
        let github = FakeGitHub::new(200, body);
        let release = GitHubReleases::new(&github).latest().expect("an answer").expect("a release");

        assert_eq!(release.version, "0.2.0", "the leading v is stripped");
        assert_eq!(release.assets.len(), 3);
        assert_eq!(release.assets[0].name, "wraex-0.2.0-x86_64-pc-windows-msvc-setup.exe");
        assert_eq!(release.assets[0].url, "https://github.com/Dumspy/wraex/releases/download/v0.2.0/setup.exe");
        assert_eq!(release.assets[0].size, 42);
        assert_eq!(release.assets[0].sha256, Some("ab".repeat(32)), "upper-case hex is lower-cased");
        assert_eq!(release.assets[1].sha256, None, "a null digest is no checksum");
        assert_eq!(release.assets[2].sha256, None, "only a sha256 digest is used");

        let sent = &github.sent.borrow()[0];
        assert_eq!(sent.url, "https://api.github.com/repos/Dumspy/wraex/releases/latest");
        assert_eq!(sent.method, Method::Get);
    }

    #[test]
    fn a_release_without_assets_has_none() {
        let github = FakeGitHub::new(200, json!({ "tag_name": "0.2.0" }).to_string());
        let release = GitHubReleases::new(&github).latest().expect("an answer").expect("a release");
        assert_eq!(release.version, "0.2.0");
        assert!(release.assets.is_empty());
    }

    #[test]
    fn a_missing_release_is_none_and_bad_answers_are_errors() {
        let github = FakeGitHub::new(404, "{}".into());
        assert_eq!(GitHubReleases::new(&github).latest(), Ok(None));

        let github = FakeGitHub::new(500, "oops".into());
        assert!(GitHubReleases::new(&github).latest().is_err());

        let github = FakeGitHub::new(200, "not json".into());
        assert!(GitHubReleases::new(&github).latest().is_err());

        let github = FakeGitHub::new(200, "{}".into());
        assert!(GitHubReleases::new(&github).latest().is_err(), "a body without a tag is not a release");

        assert!(GitHubReleases::new(&Offline).latest().is_err(), "a failed request is an error");
    }

    #[test]
    fn download_fetches_the_asset_url() {
        let mut github = FakeGitHub::new(200, String::new());
        github.file = b"setup bytes".to_vec();
        let asset = Asset {
            name: "setup.exe".into(),
            url: "https://github.com/Dumspy/wraex/releases/download/v0.2.0/setup.exe".into(),
            size: 11,
            sha256: None,
        };
        let mut sink = Vec::new();
        let written = GitHubReleases::new(&github).download(&asset, &mut sink).expect("the file");

        assert_eq!(written, 11);
        assert_eq!(sink, b"setup bytes");
        assert_eq!(github.fetched.borrow().as_slice(), [asset.url.as_str()]);
    }
}
