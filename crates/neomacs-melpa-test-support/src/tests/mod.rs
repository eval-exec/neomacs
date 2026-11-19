//! Tests about the crate as a whole rather than one module.

use super::*;

mod process_test;

// ATTEST-MELPA-SPAWN: the MELPA runtime resolver must attest the binary it
// spawns, so an ad-hoc filter over a MELPA-backed oracle family cannot spawn
// an editor whose Lisp tree was synced or stripped after the build.  Each
// scenario is its own #[test] because nextest runs every test in a fresh
// process: the attestation caches one verdict per (stamp, nextest run), so a
// second call in the same process would legitimately hit the cache instead of
// re-examining a changed tree.

fn attested_fixture() -> (tempfile::TempDir, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("tempdir");
    let root = directory.path().to_path_buf();
    let lisp = root.join("lisp");
    std::fs::create_dir_all(&lisp).expect("lisp tree");
    std::fs::write(lisp.join("loadup.el"), "(load \"subr\")\n").expect("loadup");
    for name in ["loaddefs", "subr"] {
        std::fs::write(lisp.join(format!("{name}.el")), "").expect("source");
        std::fs::write(lisp.join(format!("{name}.elc")), "").expect("bytecode");
    }
    let binary = root.join("neomacs");
    std::fs::write(&binary, b"test binary\n").expect("binary");
    neomacs_test_env::fresh_build_stamp::write_stamp(&binary, &lisp).expect("stamp");
    (directory, binary)
}

#[test]
fn neomacs_binary_attestation_accepts_a_fresh_built_pair() {
    let (directory, binary) = attested_fixture();
    attest_binary(&binary, directory.path()).expect("a fresh-built binary and tree attest");
}

#[test]
fn neomacs_binary_attestation_refuses_a_deleted_preloaded_elc() {
    let (directory, binary) = attested_fixture();
    std::fs::remove_file(directory.path().join("lisp/subr.elc")).expect("delete .elc");
    let message = attest_binary(&binary, directory.path()).expect_err("must refuse");
    assert!(
        message.contains("subr.elc"),
        "the refusal must name the missing bytecode: {message}"
    );
}

#[test]
fn neomacs_binary_attestation_refuses_an_unstamped_binary() {
    let (directory, _binary) = attested_fixture();
    let unstamped = directory.path().join("neomacs-unstamped");
    std::fs::write(&unstamped, b"other binary\n").expect("binary");
    let message = attest_binary(&unstamped, directory.path()).expect_err("must refuse");
    assert!(
        message.contains("fresh-build"),
        "the refusal must name the build recipe: {message}"
    );
}

/// The package-TUI save stage renders `Write file: <home>/.gitignore` on a
/// single 160-column row.  A deep workspace path used to push the sandbox
/// HOME past that bound because the case root lived under the MELPA cache
/// root; the package-TUI constructor picks a base that fits.
#[test]
fn melpa_sandbox_home_fits_the_package_tui_prompt_row() {
    for label in [
        "helm-gitignore-workflows",
        "helm-gitignore-list-failure",
        "helm-gitignore-generation-failure",
        "corfu",
    ] {
        let base = short_home_sandbox_base(label);
        let prefix_and_suffix = sanitize_label(label).len() + 1 + 6;
        let worst_home = base.as_os_str().len() + 1 + prefix_and_suffix + "/home".len();
        assert!(
            worst_home <= MAX_SANDBOX_HOME,
            "sandbox base {} for {label:?} leaves a {worst_home}-byte HOME",
            base.display()
        );

        let sandbox = MelpaSandbox::new_with_short_home(label).expect("create melpa sandbox");
        let prompt_width =
            "Write file: ".len() + sandbox.home.as_os_str().len() + "/.gitignore".len();
        assert!(
            prompt_width < 160,
            "label {label:?}: HOME {} needs a {prompt_width}-column prompt row",
            sandbox.home.display()
        );
    }
}

/// Every ordinary sandbox lives below the MELPA cache root inside the
/// checkout.  Editor filesystem policy then cannot depend on the host's
/// temporary directory: backups are not suppressed by
/// `normal-backup-enable-predicate`, and checkout fixtures find an ancestor
/// marker.  The host-path dependence this replaces made alarm-clock's backup
/// and auto-save-buffers-enhanced's checkout case fail in both editors once a
/// long batch label pushed the sandbox into `/tmp`.
#[test]
fn melpa_sandbox_new_lives_below_the_melpa_cache_root() {
    let sandbox = MelpaSandbox::new("sandbox-layout-contract").expect("create melpa sandbox");
    let cache_root = neomacs_infra::melpa_cache_root();
    assert!(
        sandbox.root().starts_with(&cache_root),
        "sandbox {} is not below the MELPA cache root {}",
        sandbox.root().display(),
        cache_root.display()
    );
    require_checkout_root(sandbox.root()).expect("an ordinary sandbox lives inside the checkout");
    assert!(
        outermost_checkout_root(sandbox.root()).is_some(),
        "no checkout marker above {}",
        sandbox.root().display()
    );
}

/// With `CARGO_TARGET_DIR` naming a directory outside the checkout, the MELPA
/// cache root -- and every case root below it -- is outside the checkout too.
/// `require_checkout_root` must refuse that layout by name, because GNU
/// backup policy and the checkout fixtures would otherwise derive their
/// observable behavior from the host path.
#[cfg(unix)]
#[test]
fn require_checkout_root_refuses_an_out_of_checkout_sandbox() {
    let directory = tempfile::tempdir_in("/tmp").expect("create /tmp sandbox");
    let message = require_checkout_root(directory.path()).expect_err("must refuse");
    assert!(
        message.contains("CARGO_TARGET_DIR"),
        "the refusal must name the recipe: {message}"
    );
    assert!(
        message.contains("symlink"),
        "the refusal must document the target-symlink recipe: {message}"
    );
}

/// `<worktree>/target` may be a symlink onto another volume when
/// `CARGO_TARGET_DIR` is unset. The checkout walk must stay on that logical
/// path: canonicalizing first follows the symlink and misses the checkout that
/// owns it. The directory the symlink points at stays refused.
#[cfg(unix)]
#[test]
fn outermost_checkout_root_keeps_a_logical_target_symlink_inside_the_checkout() {
    fn inside_checkout(mut path: &std::path::Path) -> bool {
        loop {
            if [".git", ".svn", ".cvs"]
                .iter()
                .any(|marker| path.join(marker).exists())
            {
                return true;
            }
            match path.parent() {
                Some(parent) if parent != path => path = parent,
                _ => return false,
            }
        }
    }

    if inside_checkout(std::path::Path::new("/tmp")) {
        eprintln!("skipping: /tmp itself is inside a checkout on this host");
        return;
    }
    let checkout = tempfile::tempdir_in("/tmp").expect("checkout");
    std::fs::write(checkout.path().join(".git"), "gitdir: /unused\n").expect("marker");
    let external = tempfile::tempdir_in("/tmp").expect("external target");
    let link = checkout.path().join("target");
    std::os::unix::fs::symlink(external.path(), &link).expect("symlink target");
    let logical = link.join("melpa").join("case-root");
    std::fs::create_dir_all(&logical).expect("logical case root");

    let found = outermost_checkout_root(&logical)
        .expect("logical target symlink stays inside the checkout");
    assert_eq!(found, checkout.path());

    let canonical = logical.canonicalize().expect("canonicalize");
    assert_ne!(canonical, logical);
    assert!(
        outermost_checkout_root(&canonical).is_none(),
        "an absolute path on the external volume is the skewing case"
    );
}
