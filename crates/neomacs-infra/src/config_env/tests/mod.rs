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

#[cfg(unix)]
#[test]
fn package_identity_records_the_elpa_layout() {
    let owner = tempfile::tempdir().expect("fixture root");
    let root = owner.path();
    let elpa = root.join("package-state/elpa/31.1/develop");
    fs::create_dir_all(&elpa).expect("elpa dir");
    for (name, file) in [
        ("ace-link-20241101.1344", "ace-link.el"),
        ("dash-20260728.103800", "dash.el"),
        ("compat-30.0.0.1", "compat.el"),
    ] {
        let dir = elpa.join(name);
        fs::create_dir(&dir).expect("package dir");
        fs::write(dir.join(file), format!(";; {name}\n")).expect("package file");
    }

    let identity = super::package_state::PackageStateIdentity::from_fixture(root)
        .expect("read package identity");
    let super::package_state::PackageStateIdentity::Elpa {
        emacs_version,
        packages,
    } = &identity
    else {
        panic!("elpa layout must read as the Elpa variant");
    };
    assert_eq!(emacs_version, "31.1");
    assert_eq!(
        packages
            .iter()
            .map(|p| (p.name.as_str(), p.version.as_str()))
            .collect::<Vec<_>>(),
        vec![
            ("ace-link", "20241101.1344"),
            ("compat", "30.0.0.1"),
            ("dash", "20260728.103800"),
        ],
        "packages must be sorted by name"
    );

    // The record round-trips, and adding one package changes its digest --
    // that is the whole identity property the pin ceremony relies on.
    let record = identity.to_record();
    let parsed =
        super::package_state::PackageStateIdentity::parse_record(&record).expect("parse record");
    assert_eq!(parsed.to_record(), record);
    let original_digest = super::package_state::digest(&record);

    fs::create_dir(elpa.join("newly-installed-20260924.1000")).expect("add package");
    let changed = super::package_state::PackageStateIdentity::from_fixture(root)
        .expect("read changed identity");
    assert_ne!(
        original_digest,
        super::package_state::digest(&changed.to_record()),
        "an added package must change the identity digest"
    );
}

#[cfg(unix)]
#[test]
fn package_identity_records_the_straight_layout() {
    let owner = tempfile::tempdir().expect("fixture root");
    let root = owner.path();
    let repos_dir = root.join("tree/.local/straight/repos");
    fs::create_dir_all(repos_dir.join("compat")).expect("repo dir");
    fs::create_dir_all(repos_dir.join("dash.el")).expect("repo dir");
    fs::create_dir(root.join("tree/.local/straight/build-31.1")).expect("build dir");
    fs::write(
        root.join("tree/.local/straight/build-31.1-cache.el"),
        ";; straight build cache\n",
    )
    .expect("build cache");

    let identity = super::package_state::PackageStateIdentity::from_fixture(root)
        .expect("read package identity");
    let super::package_state::PackageStateIdentity::Straight {
        build_cache_sha256,
        repos,
    } = &identity
    else {
        panic!("straight layout must read as the Straight variant");
    };
    assert_eq!(
        repos,
        &vec!["compat".to_owned(), "dash.el".to_owned()],
        "repos must be sorted by name"
    );
    assert_eq!(
        build_cache_sha256,
        &super::package_state::digest(";; straight build cache\n"),
        "the build cache digest anchors the straight identity"
    );

    // Adding a repo changes the identity.
    fs::create_dir(repos_dir.join("melpa")).expect("add repo");
    let changed = super::package_state::PackageStateIdentity::from_fixture(root)
        .expect("read changed identity");
    assert_ne!(
        identity.to_record(),
        changed.to_record(),
        "an added repo must change the identity record"
    );
}
