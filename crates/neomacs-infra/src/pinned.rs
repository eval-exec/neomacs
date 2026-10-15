//! Pinned upstream fixtures, fetched at test time and verified on every use.
//!
//! A test that needs an upstream artifact pins it by URL and SHA-256 and asks
//! [`pinned_file`] for it: the bytes land in `tmp/pinned-fixtures` under the
//! workspace, a cached copy is re-verified against the hash before it is
//! trusted, and a mismatch is an error rather than a silently different
//! fixture. Nothing is vendored -- the file stays upstream's, and the hash is
//! what makes the pin a pin.
//!
//! `neomacs-test-fonts` applies the same convention to its fonts -- including
//! its archives, via [`pinned_file_with_limit`]; unpacking an archive stays
//! with the caller.

use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::Duration;

use fs4::FileExt;
use thiserror::Error;

/// The workspace root, or the live one when nextest provides it. See
/// [`crate::workspace_root`].
fn cache_root() -> PathBuf {
    crate::workspace_root().join("tmp/pinned-fixtures")
}

/// The largest fixture [`pinned_file`] will fetch: a test asset, not a
/// distribution. Callers with larger pins name their own cap through
/// [`pinned_file_with_limit`].
const MAX_PINNED_FIXTURE_BYTES: u64 = 32 * 1024 * 1024;

/// What a pinned-fixture fetch can fail with.
///
/// Typed rather than a string so a caller can keep the cause chain and name
/// the fixture it asked for: this type knows URLs, cache paths and hashes, but
/// not which test wanted the bytes.
#[derive(Debug, Error)]
pub enum PinnedError {
    #[error("create the fixture cache at {path}: {source}")]
    CacheRoot {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("lock {path}: {source}")]
    Lock {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("GET {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: Box<ureq::Error>,
    },
    #[error("{url} is {actual} bytes, over the pinned fixture's {max_bytes}-byte limit")]
    TooLarge {
        url: String,
        max_bytes: u64,
        actual: u64,
    },
    #[error(
        "{url} has SHA-256 {actual}, expected {expected} -- the pin no longer describes this artifact"
    )]
    DownloadedHash {
        url: String,
        expected: String,
        actual: String,
    },
    #[error("{path} has SHA-256 {actual}, expected {expected}")]
    CachedHash {
        path: PathBuf,
        expected: String,
        actual: String,
    },
    #[error("read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("rename into {path}: {source}")]
    Rename {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Fetch `url` into the shared fixture cache as `name`, verifying `sha256`
/// both before accepting a cached copy and after downloading.
///
/// Fails loudly when the network or the hash disagrees: a test that pins an
/// upstream artifact must not pass on a fixture that is not the pinned bytes,
/// and must not quietly skip either.
pub fn pinned_file(name: &str, url: &str, sha256: &str) -> Result<PathBuf, PinnedError> {
    pinned_file_with_limit(name, url, sha256, MAX_PINNED_FIXTURE_BYTES)
}

/// [`pinned_file`] with the fetch's size cap named by the caller: a font
/// archive is a pinned fixture too, just not a small one.
pub fn pinned_file_with_limit(
    name: &str,
    url: &str,
    sha256: &str,
    max_bytes: u64,
) -> Result<PathBuf, PinnedError> {
    let root = cache_root();
    fs::create_dir_all(&root).map_err(|source| PinnedError::CacheRoot {
        path: root.clone(),
        source,
    })?;
    let destination = root.join(name);
    // One fetch per fixture across concurrently running test binaries.
    let lock_path = root.join(format!(".{name}.lock"));
    let lock = File::create(&lock_path).map_err(|source| PinnedError::Lock {
        path: lock_path.clone(),
        source,
    })?;
    lock.lock().map_err(|source| PinnedError::Lock {
        path: lock_path,
        source,
    })?;
    let result = ensure_pinned(&destination, url, sha256, max_bytes);
    let _ = FileExt::unlock(&lock);
    result.map(|()| destination)
}

fn ensure_pinned(
    destination: &Path,
    url: &str,
    sha256: &str,
    max_bytes: u64,
) -> Result<(), PinnedError> {
    if destination.exists() && verify(destination, sha256).is_ok() {
        return Ok(());
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(120)))
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_body(Some(Duration::from_secs(60)))
        .build();
    let mut response = ureq::Agent::new_with_config(agent)
        .get(url)
        .call()
        .map_err(|source| PinnedError::Http {
            url: url.to_owned(),
            source: Box::new(source),
        })?;
    // One byte over the cap, then an explicit check: ureq's limit reader
    // raises `BodyExceedsLimit` on the read after its allowance reaches zero,
    // so a body exactly at `max_bytes` must not be what trips it.
    let bytes = response
        .body_mut()
        .with_config()
        .limit(max_bytes.saturating_add(1))
        .read_to_vec()
        .map_err(|source| PinnedError::Http {
            url: url.to_owned(),
            source: Box::new(source),
        })?;
    if bytes.len() as u64 > max_bytes {
        return Err(PinnedError::TooLarge {
            url: url.to_owned(),
            max_bytes,
            actual: bytes.len() as u64,
        });
    }

    // Verify before touching the cache, so a bad response cannot replace a
    // good copy.
    let actual = crate::inventory::sha256_hex(&bytes);
    if actual != sha256 {
        return Err(PinnedError::DownloadedHash {
            url: url.to_owned(),
            expected: sha256.to_owned(),
            actual,
        });
    }
    let partial =
        destination.with_file_name(format!(".{name}.partial", name = name_of(destination)));
    fs::write(&partial, &bytes).map_err(|source| PinnedError::Write {
        path: partial.clone(),
        source,
    })?;
    fs::rename(&partial, destination).map_err(|source| PinnedError::Rename {
        path: destination.to_path_buf(),
        source,
    })?;
    verify(destination, sha256)
}

fn name_of(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pinned")
        .to_owned()
}

/// The cached bytes are what the pin describes.
fn verify(path: &Path, expected: &str) -> Result<(), PinnedError> {
    let bytes = fs::read(path).map_err(|source| PinnedError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let actual = crate::inventory::sha256_hex(&bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(PinnedError::CachedHash {
            path: path.to_path_buf(),
            expected: expected.to_owned(),
            actual,
        })
    }
}

#[cfg(test)]
#[path = "pinned/tests/pinned_test.rs"]
mod tests;
