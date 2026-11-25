use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The shared cache root follows the same precedence as `neomacs-test-fonts`:
/// explicit env override, then XDG, then HOME, then the workspace fallback.
#[test]
fn pinned_fixture_cache_root_prefers_explicit_then_xdg_then_home_then_workspace() {
    let workspace = PathBuf::from("/worktree-a");
    assert_eq!(
        resolve_pinned_fixture_cache_root(
            Some("/shared/pinned".into()),
            Some("/xdg".into()),
            Some("/home/tester".into()),
            workspace.clone(),
        ),
        PathBuf::from("/shared/pinned")
    );
    assert_eq!(
        resolve_pinned_fixture_cache_root(
            Some("".into()),
            Some("/xdg".into()),
            Some("/home/tester".into()),
            workspace.clone(),
        ),
        PathBuf::from("/xdg/neomacs/pinned-fixtures")
    );
    assert_eq!(
        resolve_pinned_fixture_cache_root(
            None,
            Some("".into()),
            Some("/home/tester".into()),
            workspace.clone(),
        ),
        PathBuf::from("/home/tester/.cache/neomacs/pinned-fixtures")
    );
    assert_eq!(
        resolve_pinned_fixture_cache_root(None, Some("".into()), Some("".into()), workspace),
        PathBuf::from("/worktree-a/tmp/pinned-fixtures")
    );
}

/// The bytes the cache holds are what the hash describes: a cached fixture
/// with the pinned hash is accepted, and one with any other hash is refused
/// with the typed mismatch a caller can match on.
#[test]
fn verification_accepts_only_the_pinned_bytes() {
    let temporary = tempfile::tempdir().expect("temporary fixture directory");
    let path = temporary.path().join("verify-probe.bin");
    std::fs::write(&path, b"pinned bytes").expect("write probe");

    let good = crate::inventory::sha256_hex(b"pinned bytes");
    assert!(verify(&path, &good).is_ok());
    match verify(&path, &"0".repeat(64)) {
        Err(error @ PinnedError::CachedHash { .. }) => {
            assert!(error.to_string().contains("SHA-256"), "{error}");
        }
        other => panic!("a wrong hash must be reported as a hash error: {other:?}"),
    }
}

/// A cache read that cannot happen is its own typed error, not a hash
/// mismatch: the two call for different reactions (re-fetch vs give up).
#[test]
fn an_unreadable_cache_entry_is_a_read_error() {
    let temporary = tempfile::tempdir().expect("temporary fixture directory");
    let missing = temporary.path().join("never-written-probe.bin");
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
    let payload = format!("limit-probe-{}", std::process::id()).into_bytes();
    let sha256 = crate::inventory::sha256_hex(&payload);
    let name = format!("limit-probe-{}.bin", std::process::id());
    let path = publish_verified_bytes(&root, &name, &sha256, &payload).expect("publish probe");

    let resolved = pinned_file_with_limit(
        &name,
        "https://127.0.0.1:1/never",
        &sha256,
        192 * 1024 * 1024,
    )
    .expect("a verified cache entry resolves offline");
    assert_eq!(resolved, path);
    std::fs::remove_dir_all(entry_dir(&root, &sha256)).ok();
}

/// A cached fixture is trusted without touching the network: the URL below
/// cannot resolve, and the call still succeeds because the bytes on disk
/// verify. This is also what keeps a suite that already fetched its fixtures
/// runnable offline.
#[test]
fn a_verified_cache_entry_needs_no_network() {
    let root = cache_root();
    let payload = format!("cache-probe-{}", std::process::id()).into_bytes();
    let sha256 = crate::inventory::sha256_hex(&payload);
    let name = format!("cache-probe-{}.bin", std::process::id());
    let path = publish_verified_bytes(&root, &name, &sha256, &payload).expect("publish probe");

    let resolved = pinned_file(&name, "https://127.0.0.1:1/never", &sha256)
        .expect("a verified cache entry resolves offline");
    assert_eq!(resolved, path);
    std::fs::remove_dir_all(entry_dir(&root, &sha256)).ok();
}

/// A test-runtime miss stays offline and does not materialize the cache.
#[test]
fn cache_only_missing_fixture_is_a_typed_miss_without_writes() {
    let temporary = tempfile::tempdir().expect("temporary fixture directory");
    let root = temporary.path().join("not-created");
    let expected = crate::inventory::sha256_hex(b"pinned bytes");
    match cached_pinned_file_in(&root, "fixture.xpm", &expected) {
        Err(PinnedError::CacheMiss { path }) => {
            assert_eq!(path, entry_path(&root, "fixture.xpm", &expected));
        }
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
    let expected = crate::inventory::sha256_hex(b"pinned bytes");
    let path = entry_path(temporary.path(), "fixture.xpm", &expected);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"corrupt bytes").unwrap();
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
    let expected = crate::inventory::sha256_hex(b"pinned bytes");
    let path = publish_verified_bytes(temporary.path(), "fixture.xpm", &expected, b"pinned bytes")
        .unwrap();
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

/// Two worktrees pinning different revisions of the same filename coexist in
/// the shared cache without clobbering each other.
#[test]
fn distinct_revisions_for_the_same_filename_do_not_collide() {
    let shared = tempfile::tempdir().expect("shared cache");
    let rev_a = b"revision A bytes";
    let rev_b = b"revision B bytes";
    let sha_a = crate::inventory::sha256_hex(rev_a);
    let sha_b = crate::inventory::sha256_hex(rev_b);

    let path_a =
        publish_verified_bytes(shared.path(), "nyan-mode-nyan.xpm", &sha_a, rev_a).unwrap();
    let path_b =
        publish_verified_bytes(shared.path(), "nyan-mode-nyan.xpm", &sha_b, rev_b).unwrap();

    assert_ne!(path_a, path_b);
    assert_eq!(
        cached_pinned_file_in(shared.path(), "nyan-mode-nyan.xpm", &sha_a).unwrap(),
        path_a
    );
    assert_eq!(
        cached_pinned_file_in(shared.path(), "nyan-mode-nyan.xpm", &sha_b).unwrap(),
        path_b
    );
    assert_eq!(std::fs::read(&path_a).unwrap(), rev_a);
    assert_eq!(std::fs::read(&path_b).unwrap(), rev_b);
}

/// Atomic publish writes through a sibling staging directory and leaves no
/// staging leftovers on success or on hash mismatch.
#[test]
fn atomic_publish_stages_cleanly_and_rejects_bad_downloads() {
    let shared = tempfile::tempdir().expect("shared cache");
    let good = b"good pinned payload";
    let sha = crate::inventory::sha256_hex(good);

    let err = pinned_file_in_with_fetch(
        shared.path(),
        "fixture.xpm",
        "https://fixtures.invalid/fixture.xpm",
        &sha,
        1024,
        || Ok(b"wrong payload".to_vec()),
    )
    .expect_err("wrong download digest must fail");
    assert!(matches!(err, PinnedError::DownloadedHash { .. }));
    assert!(
        !entry_dir(shared.path(), &sha).exists(),
        "bad download must not create the hash directory"
    );

    let published = pinned_file_in_with_fetch(
        shared.path(),
        "fixture.xpm",
        "https://fixtures.invalid/fixture.xpm",
        &sha,
        1024,
        || Ok(good.to_vec()),
    )
    .expect("verified download publishes atomically");
    assert_eq!(published, entry_path(shared.path(), "fixture.xpm", &sha));
    assert_eq!(std::fs::read(&published).unwrap(), good);

    let leftover_stages: Vec<_> = std::fs::read_dir(shared.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".stage-"))
        .collect();
    assert!(
        leftover_stages.is_empty(),
        "unexpected staging leftovers: {leftover_stages:?}"
    );
}

/// A corrupt cached entry is refused by the cache-only reader and atomically
/// repaired by the writer without leaving staging leftovers behind.
#[test]
fn corrupt_entry_is_repaired_atomically_by_writer() {
    let shared = tempfile::tempdir().expect("shared cache");
    let good = b"repaired pinned bytes";
    let sha = crate::inventory::sha256_hex(good);
    let path = entry_path(shared.path(), "fixture.xpm", &sha);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"corrupt bytes").unwrap();

    assert!(matches!(
        cached_pinned_file_in(shared.path(), "fixture.xpm", &sha),
        Err(PinnedError::CachedHash { .. })
    ));

    let repaired = pinned_file_in_with_fetch(
        shared.path(),
        "fixture.xpm",
        "https://fixtures.invalid/fixture.xpm",
        &sha,
        1024,
        || Ok(good.to_vec()),
    )
    .expect("writer repairs corrupt entry");
    assert_eq!(repaired, path);
    assert_eq!(std::fs::read(&repaired).unwrap(), good);
    assert_eq!(
        cached_pinned_file_in(shared.path(), "fixture.xpm", &sha).unwrap(),
        path
    );
    let leftover_stages: Vec<_> = std::fs::read_dir(shared.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(".stage-"))
        .collect();
    assert!(leftover_stages.is_empty(), "{leftover_stages:?}");
}

/// Malformed pin names or digests are rejected before any path join.
#[test]
fn invalid_pin_name_or_hash_is_rejected() {
    let shared = tempfile::tempdir().expect("shared cache");
    let good_sha = crate::inventory::sha256_hex(b"ok");
    for bad_name in ["", ".", "..", "../escape.xpm", "sub/dir.xpm", "a\\b.xpm"] {
        assert!(
            matches!(
                cached_pinned_file_in(shared.path(), bad_name, &good_sha),
                Err(PinnedError::InvalidPin { .. })
            ),
            "expected InvalidPin for name {bad_name:?}"
        );
    }
    for bad_sha in [
        "",
        "short",
        &"G".repeat(64),
        &good_sha.to_uppercase(),
        &format!("../{}", "a".repeat(61)),
    ] {
        assert!(
            matches!(
                cached_pinned_file_in(shared.path(), "fixture.xpm", bad_sha),
                Err(PinnedError::InvalidPin { .. })
            ),
            "expected InvalidPin for sha {bad_sha:?}"
        );
    }
}

/// Two distinct worktrees sharing XDG/HOME resolve the exact same hash-keyed
/// cache entry: worktree A publishes once, and worktree B validates and reuses
/// that entry without re-fetching or rebuilding, while a different revision pin
/// still misses.
#[test]
fn two_worktrees_share_one_hash_keyed_entry_without_refetch() {
    let sandbox = tempfile::tempdir().expect("sandbox");
    let wt_a = sandbox.path().join("worktree-a");
    let wt_b = sandbox.path().join("worktree-b");
    let xdg = sandbox.path().join("shared-xdg");
    let home = sandbox.path().join("home");
    std::fs::create_dir_all(&wt_a).unwrap();
    std::fs::create_dir_all(&wt_b).unwrap();

    let root_a = resolve_pinned_fixture_cache_root(
        None,
        Some(xdg.clone().into_os_string()),
        Some(home.clone().into_os_string()),
        wt_a.clone(),
    );
    let root_b = resolve_pinned_fixture_cache_root(
        None,
        Some(xdg.into_os_string()),
        Some(home.into_os_string()),
        wt_b.clone(),
    );
    assert_eq!(root_a, root_b);
    assert!(!root_a.starts_with(&wt_a));
    assert!(!root_b.starts_with(&wt_b));

    let bytes = b"shared pinned fixture payload";
    let sha = crate::inventory::sha256_hex(bytes);
    let other_sha = crate::inventory::sha256_hex(b"different pinned revision");
    let fetches = AtomicUsize::new(0);

    let path_a = pinned_file_in_with_fetch(
        &root_a,
        "nyan-mode-nyan.xpm",
        "https://fixtures.invalid/nyan.xpm",
        &sha,
        1024,
        || {
            fetches.fetch_add(1, Ordering::SeqCst);
            Ok(bytes.to_vec())
        },
    )
    .expect("worktree A populates shared cache");
    assert_eq!(fetches.load(Ordering::SeqCst), 1);

    let cached_b = cached_pinned_file_in(&root_b, "nyan-mode-nyan.xpm", &sha)
        .expect("worktree B reads shared cache offline");
    assert_eq!(cached_b, path_a);

    let path_b = pinned_file_in_with_fetch(
        &root_b,
        "nyan-mode-nyan.xpm",
        "https://127.0.0.1:1/never",
        &sha,
        1024,
        || panic!("worktree B must hit the shared entry without fetching"),
    )
    .expect("worktree B reuses shared entry without fetch");
    assert_eq!(path_b, path_a);
    assert_eq!(fetches.load(Ordering::SeqCst), 1);

    assert!(
        matches!(
            cached_pinned_file_in(&root_b, "nyan-mode-nyan.xpm", &other_sha),
            Err(PinnedError::CacheMiss { .. })
        ),
        "worktree B must not accept a different pinned revision"
    );
}
