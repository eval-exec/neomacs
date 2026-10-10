//! Pinned upstream fixtures, fetched at test time and verified on every use.
//!
//! A test that needs an upstream artifact pins it by URL and SHA-256 and asks
//! [`pinned_file`] for it: the bytes land in `tmp/pinned-fixtures` under the
//! workspace, a cached copy is re-verified against the hash before it is
//! trusted, and a mismatch is an error rather than a silently different
//! fixture. Nothing is vendored -- the file stays upstream's, and the hash is
//! what makes the pin a pin.
//!
//! This is the same convention `neomacs-test-fonts` applies to its fonts; the
//! simple single-file case lives here so tests that are not about fonts do not
//! grow a second copy of it.

use std::fs::{self, File};
use std::path::PathBuf;
use std::time::Duration;

use fs4::FileExt;

/// The workspace root, or the live one when nextest provides it. See
/// [`crate::workspace_root`].
fn cache_root() -> PathBuf {
    crate::workspace_root().join("tmp/pinned-fixtures")
}

/// The largest fixture this will fetch: a test asset, not a distribution.
const MAX_PINNED_FIXTURE_BYTES: u64 = 32 * 1024 * 1024;

/// Fetch `url` into the shared fixture cache as `name`, verifying `sha256`
/// both before accepting a cached copy and after downloading.
///
/// Fails loudly when the network or the hash disagrees: a test that pins an
/// upstream artifact must not pass on a fixture that is not the pinned bytes,
/// and must not quietly skip either.
pub fn pinned_file(name: &str, url: &str, sha256: &str) -> Result<PathBuf, String> {
    let root = cache_root();
    fs::create_dir_all(&root).map_err(|error| format!("create {}: {error}", root.display()))?;
    let destination = root.join(name);
    // One fetch per fixture across concurrently running test binaries.
    let lock_path = root.join(format!(".{name}.lock"));
    let lock = File::create(&lock_path)
        .map_err(|error| format!("create {}: {error}", lock_path.display()))?;
    lock.lock()
        .map_err(|error| format!("lock {}: {error}", lock_path.display()))?;
    let result = ensure_pinned(&destination, url, sha256);
    let _ = FileExt::unlock(&lock);
    result.map(|()| destination)
}

fn ensure_pinned(destination: &std::path::Path, url: &str, sha256: &str) -> Result<(), String> {
    if destination.exists() && verify(destination, sha256)?.is_ok() {
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
        .map_err(|error| format!("GET {url}: {error}"))?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(MAX_PINNED_FIXTURE_BYTES)
        .read_to_vec()
        .map_err(|error| format!("GET {url}: {error}"))?;

    // Verify before touching the cache, so a bad response cannot replace a
    // good copy.
    let actual = crate::inventory::sha256_hex(&bytes);
    if actual != sha256 {
        return Err(format!(
            "{url} has SHA-256 {actual}, expected {sha256} -- the pin no longer describes this artifact"
        ));
    }
    let partial =
        destination.with_file_name(format!(".{name}.partial", name = name_of(destination)));
    fs::write(&partial, &bytes).map_err(|error| format!("write {}: {error}", partial.display()))?;
    fs::rename(&partial, destination)
        .map_err(|error| format!("rename {}: {error}", destination.display()))?;
    verify(destination, sha256).map(|_| ())
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pinned")
        .to_owned()
}

fn verify(path: &std::path::Path, expected: &str) -> Result<Result<(), String>, String> {
    let bytes = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let actual = crate::inventory::sha256_hex(&bytes);
    if actual == expected {
        Ok(Ok(()))
    } else {
        Ok(Err(format!(
            "cached {} has SHA-256 {actual}, expected {expected}",
            path.display()
        )))
    }
}

#[cfg(test)]
#[path = "pinned/tests/pinned_test.rs"]
mod tests;
