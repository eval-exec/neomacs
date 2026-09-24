//! Synthetic-fixture tests for the seal/inventory contract: the
//! materialization tail exercised on a hand-built root, no bootstrap
//! needed.
//!
//! The point these tests pin: a sealed fixture must carry its own content
//! inventory, and verification must compare the tree *against that stored
//! record* — not against an inventory rebuilt from the tree itself, which
//! is clean by construction and detects nothing.

use super::common;
use std::fs;
use std::path::Path;

/// Give one file its write bits back, the way a misbehaving tool would
/// before mutating a sealed fixture.
#[cfg(unix)]
fn unseal_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let meta = fs::symlink_metadata(path).expect("stat tamper target");
    let mut permissions = meta.permissions();
    permissions.set_mode(meta.permissions().mode() | 0o600);
    fs::set_permissions(path, permissions).expect("unseal tamper target");
}

#[cfg(unix)]
#[test]
fn sealed_inventory_detects_post_seal_mutation() {
    let owner = tempfile::tempdir().expect("fixture root");
    let root = owner.path();
    fs::create_dir(root.join("home")).expect("home dir");
    fs::write(root.join("home/init.el"), ";; synthetic fixture\n").expect("write fixture file");
    fs::write(root.join("tree.txt"), "alpha\n").expect("write fixture file");

    common::manifest_and_seal(root, "synthetic", "test source").expect("seal synthetic fixture");

    // The seal must have recorded the content inventory the fixture is
    // verified against; a fixture without one is a fixture nobody can
    // prove byte-identity for.
    let clean = common::verify_sealed_fixture(root).expect("verify freshly sealed fixture");
    assert!(clean.is_clean(), "fresh seal must verify clean: {clean:?}");

    // The inventory itself is recorded and parseable.
    let inventory = common::load_sealed_inventory(root).expect("load stored INVENTORY");
    assert!(
        inventory
            .entries
            .iter()
            .any(|entry| entry.path == "home/init.el"),
        "the inventory must cover the fixture's files"
    );

    // The class of accident the inventory exists to catch.
    let target = root.join("tree.txt");
    unseal_file(&target);
    fs::write(&target, "tampered\n").expect("tamper with a sealed file");

    let drift = common::verify_sealed_fixture(root).expect("verify mutated fixture");
    assert_eq!(
        drift.modified,
        vec!["tree.txt".to_owned()],
        "post-seal mutation must be reported, {drift:?}"
    );
}
