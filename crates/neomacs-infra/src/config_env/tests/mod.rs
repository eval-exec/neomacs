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

#[cfg(unix)]
#[test]
fn spec_pin_binds_the_recorded_package_identity() {
    let owner = tempfile::tempdir().expect("spec dir");
    let spec_dir = owner.path();
    fs::write(
        spec_dir.join("synthetic-spec.toml"),
        "repo = \"https://example.invalid/fixture\"\n\
         revision = \"a1b2c3d4e5f6\"\n",
    )
    .expect("write spec");

    // Unpinned: reading the pin answers None, not an error.
    let spec = super::common::Spec::load_from(spec_dir, "synthetic").expect("load unpinned spec");
    assert!(spec.packages.is_none(), "no pin recorded yet: {spec:?}");

    // The identity under test: whatever a fixture's PACKAGES record says.
    let identity = super::package_state::PackageStateIdentity::parse_record(
        "schema = 1\nlayout = straight\nbuild_cache = dead\nrepos = 0\n",
    )
    .expect("synthetic record");
    let fixture_digest = super::package_state::digest(&identity.to_record());

    // Unpinned: the check passes for any identity -- nothing has committed
    // to a package set yet; `infra status` surfaces the unpinned state.
    assert!(
        super::common::check_package_pin(spec.packages.as_deref(), "synthetic", &identity).is_ok(),
        "an unpinned spec does not refuse any identity"
    );

    // The pin ceremony records the digest and logs the re-baseline.
    super::common::Spec::pin_packages(
        spec_dir,
        "synthetic",
        &fixture_digest,
        "2026-09-24 synthetic fixture materialized; package identity recorded",
    )
    .expect("pin packages");
    let pinned = super::common::Spec::load_from(spec_dir, "synthetic").expect("load pinned");
    assert_eq!(
        pinned.packages.as_deref(),
        Some(fixture_digest.as_str()),
        "{pinned:?}"
    );
    assert!(
        super::common::check_package_pin(pinned.packages.as_deref(), "synthetic", &identity)
            .is_ok()
    );

    // A changed fixture identity must fail the check with the fix named.
    let changed = super::package_state::PackageStateIdentity::parse_record(
        "schema = 1\nlayout = straight\nbuild_cache = beff\nrepos = 1\nmelpa\n",
    )
    .expect("changed synthetic record");
    let error = super::common::check_package_pin(pinned.packages.as_deref(), "synthetic", &changed)
        .expect_err("diverging identity must fail");
    assert!(
        error.contains("pin-packages synthetic"),
        "the failure must name the re-pin ceremony: {error}"
    );

    // Re-pinning updates the value and appends to the log, never silently.
    super::common::Spec::pin_packages(
        spec_dir,
        "synthetic",
        &super::package_state::digest(&changed.to_record()),
        "2026-09-24 re-pin after deliberate re-materialization",
    )
    .expect("re-pin");
    let repinned = super::common::Spec::load_from(spec_dir, "synthetic").expect("re-pinned");
    assert!(
        super::common::check_package_pin(repinned.packages.as_deref(), "synthetic", &changed)
            .is_ok(),
        "the re-pinned spec must accept the new identity"
    );
    assert!(
        super::common::check_package_pin(repinned.packages.as_deref(), "synthetic", &identity)
            .is_err(),
        "the re-pinned spec must refuse the old identity"
    );
    let spec_text =
        fs::read_to_string(spec_dir.join("synthetic-spec.toml")).expect("read spec after re-pin");
    assert!(
        spec_text.contains("2026-09-24 re-pin after deliberate re-materialization"),
        "the re-baseline must be logged in the spec: {spec_text}"
    );
}
