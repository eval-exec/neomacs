use super::*;

/// The bytes the cache holds are what the hash describes: a cached fixture
/// with the pinned hash is accepted, and one with any other hash is refused
/// with the typed mismatch a caller can match on.
#[test]
fn verification_accepts_only_the_pinned_bytes() {
    let root = cache_root();
    std::fs::create_dir_all(&root).expect("cache root");
    let path = root.join(format!("verify-probe-{}.bin", std::process::id()));
    std::fs::write(&path, b"pinned bytes").expect("write probe");

    let good = crate::inventory::sha256_hex(b"pinned bytes");
    assert!(verify(&path, &good).is_ok());
    match verify(&path, &"0".repeat(64)) {
        Err(error @ PinnedError::CachedHash { .. }) => {
            assert!(error.to_string().contains("SHA-256"), "{error}");
        }
        other => panic!("a wrong hash must be reported as a hash error: {other:?}"),
    }
    std::fs::remove_file(&path).ok();
}

/// A cache read that cannot happen is its own typed error, not a hash
/// mismatch: the two call for different reactions (re-fetch vs give up).
#[test]
fn an_unreadable_cache_entry_is_a_read_error() {
    let missing = cache_root().join("never-written-probe.bin");
    match verify(&missing, &"0".repeat(64)) {
        Err(PinnedError::Read { .. }) => {}
        other => panic!("a missing cache entry must be a read error: {other:?}"),
    }
}

/// The size-limited variant shares the cache and its verification: a verified
/// entry resolves offline for a caller that names its own cap (issue #556).
#[test]
fn a_verified_cache_entry_resolves_for_a_caller_named_limit() {
    let root = cache_root();
    std::fs::create_dir_all(&root).expect("cache root");
    let name = format!("limit-probe-{}.bin", std::process::id());
    let path = root.join(&name);
    std::fs::write(&path, b"cached fixture").expect("write probe");
    let sha256 = crate::inventory::sha256_hex(b"cached fixture");

    let resolved = pinned_file_with_limit(
        &name,
        "https://127.0.0.1:1/never",
        &sha256,
        192 * 1024 * 1024,
    )
    .expect("a verified cache entry resolves offline");
    assert_eq!(resolved, path);
    std::fs::remove_file(&path).ok();
}

/// A cached fixture is trusted without touching the network: the URL below
/// cannot resolve, and the call still succeeds because the bytes on disk
/// verify. This is also what keeps a suite that already fetched its fixtures
/// runnable offline.
#[test]
fn a_verified_cache_entry_needs_no_network() {
    let root = cache_root();
    std::fs::create_dir_all(&root).expect("cache root");
    let name = format!("cache-probe-{}.bin", std::process::id());
    let path = root.join(&name);
    std::fs::write(&path, b"cached fixture").expect("write probe");
    let sha256 = crate::inventory::sha256_hex(b"cached fixture");

    let resolved = pinned_file(&name, "https://127.0.0.1:1/never", &sha256)
        .expect("a verified cache entry resolves offline");
    assert_eq!(resolved, path);
    std::fs::remove_file(&path).ok();
}

/// A test-runtime miss stays offline and does not materialize the cache.
#[test]
fn cache_only_missing_fixture_is_a_typed_miss_without_writes() {
    let temporary = tempfile::tempdir().expect("temporary fixture directory");
    let root = temporary.path().join("not-created");
    let expected = crate::inventory::sha256_hex(b"pinned bytes");
    match cached_pinned_file_in(&root, "fixture.xpm", &expected) {
        Err(PinnedError::CacheMiss { path }) => assert_eq!(path, root.join("fixture.xpm")),
        other => panic!("expected a cache-only miss, got {other:?}"),
    }
    assert!(
        !root.exists(),
        "reading a missing fixture must not create the cache"
    );
}

/// Bad cached bytes fail integrity checking and are never repaired by a read.
#[test]
fn cache_only_corrupt_fixture_is_rejected_without_replacement() {
    let temporary = tempfile::tempdir().expect("temporary fixture directory");
    let path = temporary.path().join("fixture.xpm");
    std::fs::write(&path, b"corrupt bytes").unwrap();
    let expected = crate::inventory::sha256_hex(b"pinned bytes");
    match cached_pinned_file_in(temporary.path(), "fixture.xpm", &expected) {
        Err(PinnedError::CachedHash {
            path: actual_path,
            expected: pin,
            actual,
        }) => {
            assert_eq!(actual_path, path);
            assert_eq!(pin, expected);
            assert_eq!(actual, crate::inventory::sha256_hex(b"corrupt bytes"));
        }
        other => panic!("expected a corrupt cache error, got {other:?}"),
    }
    assert_eq!(std::fs::read(&path).unwrap(), b"corrupt bytes");
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 1);
}

/// A valid fixture resolves offline, and every later read re-verifies the pin.
#[test]
fn cache_only_valid_fixture_is_reverified_on_every_read() {
    let temporary = tempfile::tempdir().expect("temporary fixture directory");
    let path = temporary.path().join("fixture.xpm");
    std::fs::write(&path, b"pinned bytes").unwrap();
    let expected = crate::inventory::sha256_hex(b"pinned bytes");
    assert_eq!(
        cached_pinned_file_in(temporary.path(), "fixture.xpm", &expected).unwrap(),
        path
    );
    std::fs::write(&path, b"changed after verification").unwrap();
    assert!(matches!(
        cached_pinned_file_in(temporary.path(), "fixture.xpm", &expected),
        Err(PinnedError::CachedHash { .. })
    ));
    assert_eq!(std::fs::read_dir(temporary.path()).unwrap().count(), 1);
}
