//! Pinned upstream fixtures, fetched explicitly or on demand and verified on every use.
//!
//! A test that needs an upstream artifact pins it by URL and SHA-256 and asks
//! [`pinned_file`] for it: the bytes land in the shared cache returned by
//! [`pinned_fixture_cache_root`] under `<sha256>/<name>`, keyed by the pin's
//! SHA-256 digest rather than by worktree path. A cached copy is re-verified
//! against the hash before it is trusted, and a mismatch is an error rather
//! than a silently different fixture. Nothing is vendored -- the file stays
//! upstream's, and the hash is what makes the pin a pin.
//!
//! `neomacs-test-fonts` applies the same convention to its fonts -- including
//! its archives, via [`pinned_file_with_limit`]; unpacking an archive stays
//! with the caller.
//!
//! Fixtures prepared before a test run use [`cached_pinned_file`], which
//! only reads and verifies the cache and never attempts a download.

use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::path::{Component, Path, PathBuf};
use std::time::Duration;

use fs4::FileExt;
use thiserror::Error;

/// Environment variable overriding the shared pinned-fixture cache directory.
pub const PINNED_FIXTURE_CACHE_OVERRIDE: &str = "NEOMACS_PINNED_FIXTURE_CACHE";

/// Shared directory for SHA-256-keyed pinned fixtures across worktrees.
///
/// Resolution order matches `neomacs-test-fonts`:
/// 1. Nonempty `NEOMACS_PINNED_FIXTURE_CACHE`
/// 2. `$XDG_CACHE_HOME/neomacs/pinned-fixtures`
/// 3. `$HOME/.cache/neomacs/pinned-fixtures`
/// 4. `<workspace>/tmp/pinned-fixtures` as a last resort when neither XDG nor
///    HOME is set.
#[must_use]
pub fn pinned_fixture_cache_root() -> PathBuf {
    resolve_pinned_fixture_cache_root(
        std::env::var_os(PINNED_FIXTURE_CACHE_OVERRIDE),
        std::env::var_os("XDG_CACHE_HOME"),
        std::env::var_os("HOME"),
        crate::workspace_root(),
    )
}

pub(crate) fn resolve_pinned_fixture_cache_root(
    explicit: Option<OsString>,
    xdg_cache_home: Option<OsString>,
    home: Option<OsString>,
    workspace: PathBuf,
) -> PathBuf {
    fn nonempty(value: Option<OsString>) -> Option<PathBuf> {
        value.filter(|item| !item.is_empty()).map(PathBuf::from)
    }
    if let Some(path) = nonempty(explicit) {
        return path;
    }
    if let Some(xdg) = nonempty(xdg_cache_home) {
        return xdg.join("neomacs").join("pinned-fixtures");
    }
    if let Some(home) = nonempty(home) {
        return home.join(".cache").join("neomacs").join("pinned-fixtures");
    }
    workspace.join("tmp").join("pinned-fixtures")
}

fn cache_root() -> PathBuf {
    pinned_fixture_cache_root()
}

pub(crate) fn entry_dir(root: &Path, sha256: &str) -> PathBuf {
    root.join(sha256)
}

pub(crate) fn entry_path(root: &Path, name: &str, sha256: &str) -> PathBuf {
    entry_dir(root, sha256).join(name)
}

fn validate_pin(name: &str, sha256: &str) -> Result<(), PinnedError> {
    let mut components = Path::new(name).components();
    let is_single_normal = matches!(components.next(), Some(Component::Normal(_)))
        && components.next().is_none()
        && !name.contains('/')
        && !name.contains('\\');
    if !is_single_normal {
        return Err(PinnedError::InvalidPin {
            reason: format!("fixture name `{name}` must be a single cache filename"),
        });
    }
    if sha256.len() != 64
        || !sha256
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(PinnedError::InvalidPin {
            reason: format!("fixture SHA-256 `{sha256}` must be 64 lowercase hex digits"),
        });
    }
    Ok(())
}

/// The largest fixture [`pinned_file`] will fetch: a test asset, not a
/// distribution. Callers with larger pins name their own cap through
/// [`pinned_file_with_limit`].
const MAX_PINNED_FIXTURE_BYTES: u64 = 32 * 1024 * 1024;

/// What pinned-fixture access can fail with.
///
/// Typed rather than a string so a caller can keep the cause chain and name
/// the fixture it asked for: this type knows URLs, cache paths and hashes, but
/// not which test wanted the bytes.
#[derive(Debug, Error)]
pub enum PinnedError {
    #[error("invalid pinned fixture identity: {reason}")]
    InvalidPin { reason: String },
    #[error(
        "pinned fixture missing from cache at {path}; prefetch the fixture before running tests"
    )]
    CacheMiss { path: PathBuf },
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

/// Read an already-prefetched fixture and verify its SHA-256.
///
/// This cache-only path never creates directories, locks, or network requests.
/// A missing entry is [`PinnedError::CacheMiss`]; corrupt bytes remain a
/// [`PinnedError::CachedHash`] error. The caller names its prefetch command.
pub fn cached_pinned_file(name: &str, sha256: &str) -> Result<PathBuf, PinnedError> {
    cached_pinned_file_in(&cache_root(), name, sha256)
}

fn cached_pinned_file_in(root: &Path, name: &str, sha256: &str) -> Result<PathBuf, PinnedError> {
    validate_pin(name, sha256)?;
    let destination = entry_path(root, name, sha256);
    match verify(&destination, sha256) {
        Err(PinnedError::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            Err(PinnedError::CacheMiss { path: destination })
        }
        result => result.map(|()| destination),
    }
}

/// [`pinned_file`] with the fetch's size cap named by the caller: a font
/// archive is a pinned fixture too, just not a small one.
pub fn pinned_file_with_limit(
    name: &str,
    url: &str,
    sha256: &str,
    max_bytes: u64,
) -> Result<PathBuf, PinnedError> {
    pinned_file_in_with_fetch(&cache_root(), name, url, sha256, max_bytes, || {
        download_pinned(url, max_bytes)
    })
}

fn pinned_file_in_with_fetch<F>(
    root: &Path,
    name: &str,
    url: &str,
    sha256: &str,
    max_bytes: u64,
    fetch: F,
) -> Result<PathBuf, PinnedError>
where
    F: FnOnce() -> Result<Vec<u8>, PinnedError>,
{
    validate_pin(name, sha256)?;
    let destination = entry_path(root, name, sha256);
    if destination.exists() && verify(&destination, sha256).is_ok() {
        return Ok(destination);
    }
    fs::create_dir_all(root).map_err(|source| PinnedError::CacheRoot {
        path: root.to_path_buf(),
        source,
    })?;
    // One fetch per pinned digest across concurrently running test binaries and worktrees.
    let lock_path = root.join(format!(".{sha256}.lock"));
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .map_err(|source| PinnedError::Lock {
            path: lock_path.clone(),
            source,
        })?;
    lock.lock().map_err(|source| PinnedError::Lock {
        path: lock_path,
        source,
    })?;
    let result = ensure_pinned_in(root, name, url, sha256, max_bytes, fetch);
    let _ = FileExt::unlock(&lock);
    result
}

fn download_pinned(url: &str, max_bytes: u64) -> Result<Vec<u8>, PinnedError> {
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
    response
        .body_mut()
        .with_config()
        .limit(max_bytes.saturating_add(1))
        .read_to_vec()
        .map_err(|source| PinnedError::Http {
            url: url.to_owned(),
            source: Box::new(source),
        })
}

fn ensure_pinned_in<F>(
    root: &Path,
    name: &str,
    url: &str,
    sha256: &str,
    max_bytes: u64,
    fetch: F,
) -> Result<PathBuf, PinnedError>
where
    F: FnOnce() -> Result<Vec<u8>, PinnedError>,
{
    let destination = entry_path(root, name, sha256);
    if destination.exists() && verify(&destination, sha256).is_ok() {
        return Ok(destination);
    }
    let bytes = fetch()?;
    if bytes.len() as u64 > max_bytes {
        return Err(PinnedError::TooLarge {
            url: url.to_owned(),
            max_bytes,
            actual: bytes.len() as u64,
        });
    }

    // Verify before touching the cache, so a bad response cannot replace a
    // good copy or leave an entry directory behind.
    let actual = crate::inventory::sha256_hex(&bytes);
    if actual != sha256 {
        return Err(PinnedError::DownloadedHash {
            url: url.to_owned(),
            expected: sha256.to_owned(),
            actual,
        });
    }
    publish_verified_bytes(root, name, sha256, &bytes)
}

/// Stage verified bytes in a sibling temporary directory inside `root` and
/// publish them atomically via rename.
fn publish_verified_bytes(
    root: &Path,
    name: &str,
    sha256: &str,
    bytes: &[u8],
) -> Result<PathBuf, PinnedError> {
    validate_pin(name, sha256)?;
    let target_dir = entry_dir(root, sha256);
    let destination = target_dir.join(name);
    let actual = crate::inventory::sha256_hex(bytes);
    if actual != sha256 {
        return Err(PinnedError::CachedHash {
            path: destination,
            expected: sha256.to_owned(),
            actual,
        });
    }
    fs::create_dir_all(root).map_err(|source| PinnedError::CacheRoot {
        path: root.to_path_buf(),
        source,
    })?;
    let stage = tempfile::Builder::new()
        .prefix(&format!(".stage-{}-", &sha256[..12]))
        .tempdir_in(root)
        .map_err(|source| PinnedError::Write {
            path: root.to_path_buf(),
            source,
        })?;
    let staged_file = stage.path().join(name);
    fs::write(&staged_file, bytes).map_err(|source| PinnedError::Write {
        path: staged_file.clone(),
        source,
    })?;

    if !target_dir.exists() {
        match fs::rename(stage.path(), &target_dir) {
            Ok(()) => {
                // The staging directory is now the live entry directory.
                let _ = stage.keep();
            }
            Err(_) if target_dir.is_dir() => {
                // Another writer created the hash directory concurrently;
                // publish the staged file atomically into it.
                fs::rename(&staged_file, &destination).map_err(|source| PinnedError::Rename {
                    path: destination.clone(),
                    source,
                })?;
            }
            Err(source) => {
                return Err(PinnedError::Rename {
                    path: destination,
                    source,
                });
            }
        }
    } else {
        fs::rename(&staged_file, &destination).map_err(|source| PinnedError::Rename {
            path: destination.clone(),
            source,
        })?;
    }
    verify(&destination, sha256).map(|()| destination)
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
