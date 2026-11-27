//! Downloading a newer release's setup and verifying it before anything runs it.
//!
//! A download is only useful to [`super::install`] when its bytes are the ones the release
//! published, so this module refuses to start without a SHA-256, hashes the bytes as they
//! stream to disk (one pass, no second read of a tens-of-MB file) and only renames the
//! `.part` file into place once the size and the digest match. A partly downloaded or
//! corrupt file is deleted, never left where the installer might find it.
//!
//! Pure `std` + `sha2`: the platform's [`super::source::Fetch`] does the network part.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use thiserror::Error;

use super::source::{Asset, Release, ReleaseSource};

/// The Windows target the release workflow builds the setup for
/// (`tools/package/package.py --target`).
pub const WINDOWS_TARGET: &str = "x86_64-pc-windows-msvc";

/// `wraex-<version>-<target>-setup.exe`, the name `tools/package/package.py --installer`
/// gives the setup; the release asset is the same file under the same name.
pub fn setup_name(version: &str, target: &str) -> String {
    format!("wraex-{version}-{target}-setup.exe")
}

/// The release's setup for `target`, by exact name: no other asset is installable, and a
/// name that differs by a character is a different file.
pub fn setup_asset<'a>(release: &'a Release, target: &str) -> Option<&'a Asset> {
    let name = setup_name(&release.version, target);
    release.assets.iter().find(|asset| asset.name == name)
}

/// A verified setup on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    /// The release it belongs to, without the leading `v`.
    pub version: String,
    /// The file, named as the release assets are.
    pub path: PathBuf,
    /// Its SHA-256 as 64 lower-case hex digits, equal to the release's digest.
    pub sha256: String,
}

/// Why a download did not produce a verified file. Every variant but [`Io`](Self::Io) and
/// [`Source`](Self::Source) means the release cannot be installed from here.
#[derive(Debug, Error)]
pub enum DownloadError {
    #[error("release {version} has no {name}")]
    NoAsset { version: String, name: String },
    #[error("{name} has no published SHA-256, so it cannot be verified")]
    NoChecksum { name: String },
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("download failed: {0}")]
    Source(String),
    #[error("downloaded {actual} bytes, not the {expected} the release announced")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("downloaded SHA-256 {actual}, not the published {expected}")]
    ChecksumMismatch { expected: String, actual: String },
}

/// Downloads the release's setup for `target` into `dir` and verifies it. An already
/// downloaded, verified file is reused, so a restart does not fetch tens of MB again.
pub fn download(
    source: &impl ReleaseSource,
    release: &Release,
    target: &str,
    dir: &Path,
) -> Result<Downloaded, DownloadError> {
    let asset = setup_asset(release, target).ok_or_else(|| DownloadError::NoAsset {
        version: release.version.clone(),
        name: setup_name(&release.version, target),
    })?;
    // Never install an unverified file: without a digest there is nothing to check, so the
    // download does not even start.
    let expected = asset.sha256.clone().ok_or_else(|| DownloadError::NoChecksum { name: asset.name.clone() })?;

    std::fs::create_dir_all(dir)?;
    let path = dir.join(&asset.name);

    // A previous run may have fetched it already; a file with another hash is a leftover
    // from an interrupted or different release and is replaced.
    if path.exists() {
        if sha256_file(&path)? == expected {
            remove_stale(dir, &path);
            return Ok(Downloaded { version: release.version.clone(), path, sha256: expected });
        }
        std::fs::remove_file(&path)?;
    }

    let part = dir.join(format!("{}.part", asset.name));
    let mut sink = HashingFile { file: File::create(&part)?, hasher: Sha256::new(), written: 0 };

    if let Err(error) = source.download(asset, &mut sink) {
        drop(sink);
        remove_if_present(&part);
        return Err(DownloadError::Source(error));
    }
    let written = sink.written;
    if asset.size > 0 && written != asset.size {
        drop(sink);
        remove_if_present(&part);
        return Err(DownloadError::SizeMismatch { expected: asset.size, actual: written });
    }
    let digest = hex(&sink.hasher.clone().finalize());
    if digest != expected {
        drop(sink);
        remove_if_present(&part);
        return Err(DownloadError::ChecksumMismatch { expected, actual: digest });
    }

    // Flushed to the device before the rename, so a crash cannot leave the installer a
    // correctly named file with an empty tail.
    let synced = sink.file.flush().and_then(|()| sink.file.sync_all());
    drop(sink);
    if let Err(error) = synced {
        remove_if_present(&part);
        return Err(DownloadError::Io(error));
    }
    if let Err(error) = std::fs::rename(&part, &path) {
        remove_if_present(&part);
        return Err(DownloadError::Io(error));
    }

    remove_stale(dir, &path);
    Ok(Downloaded { version: release.version.clone(), path, sha256: digest })
}

/// The file's SHA-256 as 64 lower-case hex digits, streamed in 64 KiB chunks: a setup is
/// tens of MB and does not belong in memory.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// A [`Write`] that hashes what it writes and counts the bytes, so the digest and the size
/// check come from the same pass as the file contents.
struct HashingFile {
    file: File,
    hasher: Sha256,
    written: u64,
}

impl Write for HashingFile {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.file.write(buffer)?;
        self.hasher.update(&buffer[..written]);
        self.written += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// Removes earlier downloads from `dir`: another version's `*-setup.exe` and any leftover
/// `.part`, but never `keep` (the file just written) and never another kind of file.
fn remove_stale(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.as_path() == keep {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else { continue };
        if name.ends_with("-setup.exe") || name.ends_with(".part") {
            remove_if_present(&path);
        }
    }
}

/// Deletes `path`, ignoring a file that is already gone: the caller is already reporting
/// the error that made the cleanup necessary.
fn remove_if_present(path: &Path) {
    let _ = std::fs::remove_file(path);
}

/// The digest as lower-case hex, which is how GitHub publishes it.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::io::Write;

    use super::*;

    /// Serves the file from memory and counts the downloads, so a test can tell a fetch
    /// from a reuse; `fail` makes the source fail as a dropped connection would.
    struct FakeSource {
        bytes: Vec<u8>,
        fail: Option<&'static str>,
        downloads: Cell<usize>,
    }

    impl FakeSource {
        fn serving(bytes: &[u8]) -> Self {
            Self { bytes: bytes.to_vec(), fail: None, downloads: Cell::new(0) }
        }

        fn failing(error: &'static str) -> Self {
            Self { bytes: Vec::new(), fail: Some(error), downloads: Cell::new(0) }
        }

        fn downloads(&self) -> usize {
            self.downloads.get()
        }
    }

    impl ReleaseSource for FakeSource {
        fn latest(&self) -> Result<Option<Release>, String> {
            unreachable!("the download tests never check for a release")
        }

        fn download(&self, _: &Asset, sink: &mut dyn Write) -> Result<u64, String> {
            self.downloads.set(self.downloads.get() + 1);
            if let Some(error) = self.fail {
                return Err(error.to_owned());
            }
            sink.write_all(&self.bytes).map_err(|error| error.to_string())?;
            Ok(self.bytes.len() as u64)
        }
    }

    /// A directory of this test's own under the temp dir; a rerun starts empty.
    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wraex-download-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    const BYTES: &[u8] = b"the setup's bytes";

    fn digest(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn asset(sha256: Option<String>, size: u64) -> Asset {
        Asset {
            name: setup_name("0.2.0", WINDOWS_TARGET),
            url: "https://github.com/Dumspy/wraex/releases/download/v0.2.0/setup.exe".into(),
            size,
            sha256,
        }
    }

    fn release(asset: Asset) -> Release {
        Release { version: "0.2.0".into(), assets: vec![asset] }
    }

    fn files(dir: &Path) -> Vec<String> {
        let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
        let mut names: Vec<String> =
            entries.flatten().map(|entry| entry.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        names
    }

    #[test]
    fn a_verified_download_lands_under_the_release_name() {
        let dir = temp_dir("happy");
        let source = FakeSource::serving(BYTES);
        let expected = digest(BYTES);
        let release = release(asset(Some(expected.clone()), BYTES.len() as u64));

        let downloaded = download(&source, &release, WINDOWS_TARGET, &dir).expect("the setup");

        assert_eq!(
            downloaded,
            Downloaded { version: "0.2.0".into(), path: dir.join(&release.assets[0].name), sha256: expected.clone() }
        );
        assert_eq!(std::fs::read(&downloaded.path).unwrap(), BYTES);
        assert_eq!(sha256_file(&downloaded.path).unwrap(), expected);
        assert_eq!(files(&dir), [release.assets[0].name.clone()], "the .part file is gone");
        assert_eq!(source.downloads(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_checksum_mismatch_leaves_nothing_behind() {
        let dir = temp_dir("checksum");
        let source = FakeSource::serving(BYTES);
        let release = release(asset(Some("00".repeat(32)), BYTES.len() as u64));

        let error = download(&source, &release, WINDOWS_TARGET, &dir).unwrap_err();

        assert!(matches!(error, DownloadError::ChecksumMismatch { .. }), "{error:?}");
        assert!(files(&dir).is_empty(), "no setup and no .part: {:?}", files(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_size_mismatch_is_refused() {
        let dir = temp_dir("size");
        let source = FakeSource::serving(BYTES);
        let release = release(asset(Some(digest(BYTES)), BYTES.len() as u64 + 1));

        let error = download(&source, &release, WINDOWS_TARGET, &dir).unwrap_err();

        assert!(
            matches!(error, DownloadError::SizeMismatch { expected, actual } if expected == BYTES.len() as u64 + 1 && actual == BYTES.len() as u64),
            "{error:?}"
        );
        assert!(files(&dir).is_empty(), "the truncated file is deleted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_release_without_a_checksum_is_not_downloaded() {
        let dir = temp_dir("no-checksum");
        let source = FakeSource::serving(BYTES);
        let release = release(asset(None, BYTES.len() as u64));

        let error = download(&source, &release, WINDOWS_TARGET, &dir).unwrap_err();

        assert!(matches!(error, DownloadError::NoChecksum { .. }), "{error:?}");
        assert_eq!(source.downloads(), 0, "nothing is fetched without a digest to check it");
        assert!(!dir.exists(), "not even the directory is made");
    }

    #[test]
    fn a_release_without_the_setup_is_refused() {
        let dir = temp_dir("no-asset");
        let source = FakeSource::serving(BYTES);
        let other = Asset { name: "wraex-0.2.0-x86_64-pc-windows-msvc.zip".into(), ..asset(Some(digest(BYTES)), 0) };
        let release = release(other);

        let error = download(&source, &release, WINDOWS_TARGET, &dir).unwrap_err();

        assert!(matches!(error, DownloadError::NoAsset { .. }), "{error:?}");
        assert_eq!(source.downloads(), 0);
        assert!(!dir.exists());
    }

    #[test]
    fn an_already_downloaded_setup_is_reused() {
        let dir = temp_dir("reuse");
        let expected = digest(BYTES);
        std::fs::create_dir_all(&dir).unwrap();
        let name = setup_name("0.2.0", WINDOWS_TARGET);
        std::fs::write(dir.join(&name), BYTES).unwrap();
        let source = FakeSource::serving(b"a different file");
        let release = release(asset(Some(expected.clone()), BYTES.len() as u64));

        let downloaded = download(&source, &release, WINDOWS_TARGET, &dir).expect("the setup");

        assert_eq!(downloaded.sha256, expected);
        assert_eq!(source.downloads(), 0, "a verified file is not fetched again");
        assert_eq!(std::fs::read(&downloaded.path).unwrap(), BYTES);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_leftover_is_replaced() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        let name = setup_name("0.2.0", WINDOWS_TARGET);
        std::fs::write(dir.join(&name), b"half a setup").unwrap();
        let source = FakeSource::serving(BYTES);
        let release = release(asset(Some(digest(BYTES)), BYTES.len() as u64));

        download(&source, &release, WINDOWS_TARGET, &dir).expect("the setup");

        assert_eq!(source.downloads(), 1);
        assert_eq!(std::fs::read(dir.join(&name)).unwrap(), BYTES);
        assert_eq!(files(&dir), [name]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn older_downloads_are_removed_and_other_files_kept() {
        let dir = temp_dir("stale");
        std::fs::create_dir_all(&dir).unwrap();
        let old = dir.join(setup_name("0.1.0", WINDOWS_TARGET));
        std::fs::write(&old, b"old").unwrap();
        let leftover = dir.join("wraex-0.3.0-x86_64-pc-windows-msvc-setup.exe.part");
        std::fs::write(&leftover, b"partial").unwrap();
        let notes = dir.join("notes.txt");
        std::fs::write(&notes, b"keep me").unwrap();
        let source = FakeSource::serving(BYTES);
        let release = release(asset(Some(digest(BYTES)), BYTES.len() as u64));

        let downloaded = download(&source, &release, WINDOWS_TARGET, &dir).expect("the setup");

        assert!(!old.exists(), "the previous version's setup is gone");
        assert!(!leftover.exists(), "a stray .part is gone");
        assert!(notes.exists(), "another kind of file is left alone");
        assert!(downloaded.path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_download_cleans_up_its_part_file() {
        let dir = temp_dir("source-error");
        let source = FakeSource::failing("connection reset");
        let release = release(asset(Some(digest(BYTES)), BYTES.len() as u64));

        let error = download(&source, &release, WINDOWS_TARGET, &dir).unwrap_err();

        assert!(matches!(&error, DownloadError::Source(message) if message == "connection reset"), "{error:?}");
        assert!(files(&dir).is_empty(), "the .part file is deleted: {:?}", files(&dir));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_setup_name_matches_the_packager() {
        assert_eq!(setup_name("0.2.0", WINDOWS_TARGET), "wraex-0.2.0-x86_64-pc-windows-msvc-setup.exe");
        assert_eq!(WINDOWS_TARGET, "x86_64-pc-windows-msvc");
    }
}
