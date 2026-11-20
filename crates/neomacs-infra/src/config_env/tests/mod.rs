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

/// Initialize a synthetic straight repo checkout with one commit, the way
/// the bootstrap leaves recipe repos: a real git tree at a pinned commit.
#[cfg(unix)]
fn git_repo(path: &Path, file: &'static str) {
    use std::process::Command;
    fs::create_dir_all(path).expect("repo dir");
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .env("GIT_AUTHOR_NAME", "infra test")
            .env("GIT_AUTHOR_EMAIL", "test@neomacs.invalid")
            .env("GIT_COMMITTER_NAME", "infra test")
            .env("GIT_COMMITTER_EMAIL", "test@neomacs.invalid")
            .env("GIT_AUTHOR_DATE", "2005-04-02T22:17:00Z")
            .env("GIT_COMMITTER_DATE", "2005-04-02T22:17:00Z")
            .output()
            .expect("spawn git");
        assert!(
            output.status.success(),
            "git {args:?} in {}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    };
    git(&["init", "--quiet"]);
    fs::write(path.join(file), ";; synthetic package\n").expect("write repo file");
    git(&["add", file]);
    git(&["commit", "--quiet", "-m", "synthetic package"]);
}

#[cfg(unix)]
#[test]
fn package_identity_records_the_straight_layout() {
    let owner = tempfile::tempdir().expect("fixture root");
    let root = owner.path();
    let straight = root.join("tree/.local/straight");
    let repos_dir = straight.join("repos");
    git_repo(&repos_dir.join("compat"), "compat.el");
    git_repo(&repos_dir.join("dash.el"), "dash.el");
    // The build cache exists in real fixtures but is host-local output:
    // it must not anchor the identity.
    fs::write(
        straight.join("build-31.1-cache.el"),
        ";; straight build cache\n",
    )
    .expect("build cache");

    let identity = super::package_state::PackageStateIdentity::from_fixture(root)
        .expect("read package identity");
    let super::package_state::PackageStateIdentity::Straight { repos } = &identity else {
        panic!("straight layout must read as the Straight variant");
    };
    assert_eq!(
        repos
            .iter()
            .map(|repo| repo.name.as_str())
            .collect::<Vec<_>>(),
        vec!["compat", "dash.el"],
        "repos must be sorted by name"
    );
    for repo in repos {
        let head = std::process::Command::new("git")
            .arg("-C")
            .arg(repos_dir.join(&repo.name))
            .args(["rev-parse", "HEAD"])
            .output()
            .expect("spawn git");
        assert_eq!(
            repo.commit,
            String::from_utf8_lossy(&head.stdout).trim(),
            "the identity must record the repo's checked-out commit"
        );
    }

    // The record round-trips.
    let record = identity.to_record();
    let parsed =
        super::package_state::PackageStateIdentity::parse_record(&record).expect("parse record");
    assert_eq!(parsed.to_record(), record);

    // Host-local output keeps the identity: a rewritten build cache (new
    // bytes, new mtime) and rewritten repo worktree files change nothing.
    fs::write(
        straight.join("build-31.1-cache.el"),
        ";; a different cache the next materialization wrote\n",
    )
    .expect("rewrite build cache");
    fs::write(repos_dir.join("compat/compat.el"), ";; rewritten\n").expect("rewrite repo file");
    let unchanged = super::package_state::PackageStateIdentity::from_fixture(root)
        .expect("read unchanged identity");
    assert_eq!(
        identity.to_record(),
        unchanged.to_record(),
        "build-cache and worktree bytes must not enter the identity"
    );

    // A second fixture at a different path with the same repos has the
    // same identity: nothing host- or path-specific is hashed.
    let other = tempfile::tempdir().expect("second fixture root");
    let other_repos = other.path().join("tree/.local/straight/repos");
    git_repo(&other_repos.join("compat"), "compat.el");
    git_repo(&other_repos.join("dash.el"), "dash.el");
    let twin = super::package_state::PackageStateIdentity::from_fixture(other.path())
        .expect("read twin identity");
    assert_eq!(
        identity.to_record(),
        twin.to_record(),
        "two materializations of the same repos must agree"
    );

    // A different HEAD in one repo changes the identity.
    let moved = std::process::Command::new("git")
        .arg("-C")
        .arg(repos_dir.join("dash.el"))
        .args(["commit", "--quiet", "--allow-empty", "-m", "upstream moved"])
        .env("GIT_AUTHOR_NAME", "infra test")
        .env("GIT_AUTHOR_EMAIL", "test@neomacs.invalid")
        .env("GIT_COMMITTER_NAME", "infra test")
        .env("GIT_COMMITTER_EMAIL", "test@neomacs.invalid")
        .env("GIT_AUTHOR_DATE", "2005-04-02T22:18:00Z")
        .env("GIT_COMMITTER_DATE", "2005-04-02T22:18:00Z")
        .output()
        .expect("spawn git");
    assert!(
        moved.status.success(),
        "advance dash.el HEAD: {}",
        String::from_utf8_lossy(&moved.stderr)
    );
    let changed = super::package_state::PackageStateIdentity::from_fixture(root)
        .expect("read changed identity");
    assert_ne!(
        identity.to_record(),
        changed.to_record(),
        "a moved upstream commit must change the identity record"
    );
}

#[test]
fn straight_records_from_the_build_cache_schema_fail_closed() {
    // A record sealed against the retired build-cache identity cannot be
    // re-pinned: the digest it carries is host-local output.  Parsing must
    // refuse it and name re-materialization, so a stale sealed fixture
    // fails at open time with the fix stated.
    let record = "schema = 1\nlayout = straight\nbuild_cache = 684009cd\nrepos = 1\ncompat\n";
    let error = super::package_state::PackageStateIdentity::parse_record(record)
        .expect_err("schema-1 straight record must fail closed");
    assert!(
        error.contains("schema = 2") && error.contains("re-materialize"),
        "error must name the retirement and the fix: {error}"
    );
}

#[test]
fn spec_platform_pins_are_independent() {
    let owner = tempfile::tempdir().expect("spec dir");
    let spec_dir = owner.path();
    fs::write(
        spec_dir.join("synthetic-spec.toml"),
        "repo = \"https://example.invalid/fixture\"\n\
         revision = \"a1b2c3d4e5f6\"\n\
         packages = \"linux-digest\"\n\
         packages_macos = \"macos-digest\"\n",
    )
    .expect("write spec");
    let spec = super::common::Spec::load_from(spec_dir, "synthetic").expect("load spec");
    assert_eq!(spec.packages_for("linux"), Some("linux-digest"));
    assert_eq!(spec.packages_for("macos"), Some("macos-digest"));
    assert_eq!(
        spec.packages_for("windows"),
        None,
        "a host without a pin is unpinned, not bound to another host's set"
    );
}

#[test]
fn repinning_linux_updates_the_key_packages_for_selects() {
    let owner = tempfile::tempdir().expect("spec dir");
    let spec_dir = owner.path();
    fs::write(
        spec_dir.join("synthetic-spec.toml"),
        "repo = \"https://example.invalid/fixture\"\n\
         revision = \"a1b2c3d4e5f6\"\n\
         packages = \"legacy-digest\"\n\
         packages_linux = \"linux-digest\"\n",
    )
    .expect("write spec");
    let spec = super::common::Spec::load_from(spec_dir, "synthetic").expect("load spec");
    assert_eq!(
        spec.packages_for("linux"),
        Some("linux-digest"),
        "an explicit packages_linux line wins over the legacy packages line"
    );

    super::common::Spec::pin_packages_for_host(
        spec_dir,
        "synthetic",
        "new-digest",
        "re-pin",
        "linux",
    )
    .expect("re-pin");
    let text = fs::read_to_string(spec_dir.join("synthetic-spec.toml")).expect("read spec");
    assert!(
        text.contains("packages_linux = \"new-digest\""),
        "a Linux re-pin must rewrite the line lookup selects: {text}"
    );
    assert!(
        text.contains("packages = \"legacy-digest\""),
        "the legacy line is not the selected Linux pin here: {text}"
    );
    let repinned = super::common::Spec::load_from(spec_dir, "synthetic").expect("reload spec");
    assert_eq!(repinned.packages_for("linux"), Some("new-digest"));
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
    assert!(
        spec.host_packages().is_none(),
        "no pin recorded for this host yet: {spec:?}"
    );

    // The identity under test: whatever a fixture's PACKAGES record says.
    let identity = super::package_state::PackageStateIdentity::parse_record(
        "schema = 2\nlayout = straight\nrepos = 0\n",
    )
    .expect("synthetic record");
    let fixture_digest = super::package_state::digest(&identity.to_record());

    // Unpinned: the check passes for any identity -- nothing has committed
    // to a package set yet; `infra status` surfaces the unpinned state.
    assert!(
        super::common::check_package_pin(spec.host_packages(), "synthetic", &identity).is_ok(),
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
        pinned.host_packages(),
        Some(fixture_digest.as_str()),
        "{pinned:?}"
    );
    assert!(
        super::common::check_package_pin(pinned.host_packages(), "synthetic", &identity).is_ok()
    );

    // A changed fixture identity must fail the check with the fix named.
    let changed = super::package_state::PackageStateIdentity::parse_record(
        "schema = 2\nlayout = straight\nrepos = 1\nmelpa 0123456789abcdef0123456789abcdef01234567\n",
    )
    .expect("changed synthetic record");
    let error = super::common::check_package_pin(pinned.host_packages(), "synthetic", &changed)
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
        super::common::check_package_pin(repinned.host_packages(), "synthetic", &changed).is_ok(),
        "the re-pinned spec must accept the new identity"
    );
    assert!(
        super::common::check_package_pin(repinned.host_packages(), "synthetic", &identity).is_err(),
        "the re-pinned spec must refuse the old identity"
    );
    let spec_text =
        fs::read_to_string(spec_dir.join("synthetic-spec.toml")).expect("read spec after re-pin");
    assert!(
        spec_text.contains("2026-09-24 re-pin after deliberate re-materialization"),
        "the re-baseline must be logged in the spec: {spec_text}"
    );
}
