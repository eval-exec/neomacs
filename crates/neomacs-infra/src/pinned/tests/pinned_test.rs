use super::*;

/// The bytes the cache holds are what the hash describes: a cached fixture
/// with the pinned hash is accepted, and one with any other hash is not.
#[test]
fn verification_accepts_only_the_pinned_bytes() {
    let root = cache_root();
    std::fs::create_dir_all(&root).expect("cache root");
    let path = root.join(format!("verify-probe-{}.bin", std::process::id()));
    std::fs::write(&path, b"pinned bytes").expect("write probe");

    let good = crate::inventory::sha256_hex(b"pinned bytes");
    assert!(matches!(verify(&path, &good), Ok(Ok(()))));
    match verify(&path, &"0".repeat(64)) {
        Ok(Err(message)) => assert!(message.contains("SHA-256"), "{message}"),
        other => panic!("a wrong hash must be reported: {other:?}"),
    }
    std::fs::remove_file(&path).ok();
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

    let resolved = pinned_file_with_limit(&name, "https://127.0.0.1:1/never", &sha256, 192 * 1024 * 1024)
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
