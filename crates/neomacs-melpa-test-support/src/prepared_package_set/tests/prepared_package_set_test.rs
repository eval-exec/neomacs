use super::*;

fn fixture() -> (tempfile::TempDir, PathBuf) {
    let root = tempfile::tempdir().expect("create package fixture");
    let package_dir = root.path().join("elpa/example-1.2.3");
    fs::create_dir_all(&package_dir).expect("create prepared package directory");
    fs::write(package_dir.join("example.el"), "(provide 'example)\n")
        .expect("write prepared package source");
    (root, package_dir)
}

#[test]
fn prepared_package_set_exports_one_reusable_startup_contract() {
    let (root, package_dir) = fixture();
    let dependency_dir = root.path().join("dependencies/compat-30.1.0.0");
    fs::create_dir_all(&dependency_dir).expect("create dependency directory");
    let prepared =
        PreparedPackageSet::from_package_dir(("example", "1.2.3"), "example.el", package_dir)
            .expect("describe prepared package")
            .with_prepared_dependency(("compat", "30.1.0.0"), dependency_dir)
            .expect("add exact dependency")
            .with_prelude("(setq example-test-ready t)");

    let environment = prepared.process_environment();
    assert_eq!(environment.len(), 2);
    assert_eq!(environment[0].0, "NEOMACS_PACKAGE_USER_DIR");
    assert_eq!(environment[1].0, "NEOMACS_PACKAGE_SOURCE");

    let startup = prepared.startup_elisp();
    assert!(startup.starts_with(";;; -*- lexical-binding: t; -*-"));
    assert!(startup.contains("(package-initialize)"));
    assert!(startup.contains("example-test-ready"));
    assert!(startup.contains("compat"));
    assert!(startup.contains("30.1.0.0"));
    assert!(startup.contains("NEOMACS_PACKAGE_SOURCE"));
}

#[test]
fn prepared_package_set_writes_the_same_startup_contract_for_pty_launches() {
    let (root, package_dir) = fixture();
    let prepared =
        PreparedPackageSet::from_package_dir(("example", "1.2.3"), "example.el", package_dir)
            .expect("describe prepared package");

    let startup_file = prepared
        .write_startup_file(&root.path().join("launch"))
        .expect("write startup file");
    assert_eq!(
        fs::read_to_string(startup_file).expect("read startup file"),
        prepared.startup_elisp()
    );
}

/// An installed GNU ships its Lisp compressed (`pcase.el.gz`): `make install`
/// runs the sources through `GZIP_PROG`, while a build tree keeps them plain.
/// Restricting `load-suffixes` to `.el` before compression support is loaded
/// makes the first `require` of an editor library load `jka-compr.el.gz` so it
/// can decompress itself, and GNU signals `Recursive load` -- every batch in
/// the suite dies during `RestartProbe`.  The handler has to be in place first;
/// it loads through the default suffixes, from `jka-compr.elc` when installed.
#[test]
fn source_load_suffixes_preload_jka_compr_before_restricting() {
    let (root, package_dir) = fixture();
    let prepared =
        PreparedPackageSet::from_package_dir(("example", "1.2.3"), "example.el", package_dir)
            .expect("describe prepared package")
            .with_load_suffixes(LoadSuffixes::Source);
    let _keep_tempdir_alive = root;

    let startup = prepared.startup_elisp();
    let preload = startup
        .find("(require 'jka-compr)")
        .unwrap_or_else(|| panic!("the source policy must preload jka-compr:\n{startup}"));
    let restrict = startup
        .find("load-suffixes '(\".el\")")
        .unwrap_or_else(|| panic!("the source policy must restrict suffixes:\n{startup}"));
    assert!(
        preload < restrict,
        "jka-compr must be loaded before the suffixes are restricted:\n{startup}"
    );
}

#[test]
fn emacs_default_suffixes_leave_jka_compr_alone() {
    let (root, package_dir) = fixture();
    let prepared =
        PreparedPackageSet::from_package_dir(("example", "1.2.3"), "example.el", package_dir)
            .expect("describe prepared package")
            .with_load_suffixes(LoadSuffixes::EmacsDefault);
    let _keep_tempdir_alive = root;

    let startup = prepared.startup_elisp();
    assert!(!startup.contains("jka-compr"), "{startup}");
    assert!(!startup.contains("load-suffixes"), "{startup}");
}
