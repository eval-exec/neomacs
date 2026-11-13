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
/// root; the harness now picks a base that fits.
#[test]
fn melpa_sandbox_home_fits_the_package_tui_prompt_row() {
    for label in [
        "helm-gitignore-workflows",
        "helm-gitignore-list-failure",
        "helm-gitignore-generation-failure",
        "corfu",
    ] {
        let base = sandbox_case_base(label);
        let prefix_and_suffix = sanitize_label(label).len() + 1 + 6;
        let worst_home = base.as_os_str().len() + 1 + prefix_and_suffix + "/home".len();
        assert!(
            worst_home <= MAX_SANDBOX_HOME,
            "sandbox base {} for {label:?} leaves a {worst_home}-byte HOME",
            base.display()
        );

        let sandbox = MelpaSandbox::new(label).expect("create melpa sandbox");
        let prompt_width =
            "Write file: ".len() + sandbox.home.as_os_str().len() + "/.gitignore".len();
        assert!(
            prompt_width < 160,
            "label {label:?}: HOME {} needs a {prompt_width}-column prompt row",
            sandbox.home.display()
        );
    }
}
