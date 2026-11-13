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

/// A compressed GNU install turns the source policy into a decompression
/// exercise: `make install` gzips Lisp, `load-suffixes` is narrowed to
/// `.el`, and jka-compr's verbose messages paint the echo area.  The
/// helm-gitignore TUI capture saw exactly `[uncompressing
/// rfc6068.el.gz...done]` spliced into Helm's `pattern:` line (P2
/// baseline), so the source-parity startup must silence that install-only
/// chatter.  The shadow `.el.gz` makes the condition observable
/// regardless of how the attested oracle was installed.
#[cfg(unix)]
#[test]
fn source_startup_silences_jka_compr_messages_for_compressed_libraries() {
    use std::process::Command;

    let (root, package_dir) = fixture();
    let shadow = root.path().join("shadow");
    fs::create_dir_all(&shadow).expect("create shadow load-path");
    let plain = shadow.join("rfc6068.el");
    fs::write(&plain, "(provide 'rfc6068)\n").expect("write shadow source");
    let gzip = Command::new("gzip")
        .arg("-c")
        .arg(&plain)
        .output()
        .expect("run gzip");
    assert!(gzip.status.success(), "gzip failed: {gzip:?}");
    fs::write(shadow.join("rfc6068.el.gz"), &gzip.stdout).expect("write compressed shadow");
    fs::remove_file(&plain).expect("remove the plain shadow so load picks the compressed one");

    let prepared =
        PreparedPackageSet::from_package_dir(("example", "1.2.3"), "example.el", package_dir)
            .expect("describe prepared package")
            .with_load_suffixes(LoadSuffixes::Source)
            .with_prelude(&format!(
                "(add-to-list 'load-path {})",
                elisp_string(&shadow.to_string_lossy())
            ));
    let startup = prepared
        .write_startup_file(&root.path().join("launch"))
        .expect("write startup file");
    let oracle = neomacs_test_oracle::gnu::gnu_oracle().executable_path();
    let home = root.path().join("home");
    let tmp = root.path().join("tmp");
    let runtime = root.path().join("runtime");
    for directory in [&home, &tmp, &runtime] {
        fs::create_dir_all(directory).expect("create canonical oracle roots");
    }

    // The oracle is an editor: spawn it through the one canonical package
    // environment instead of hand-applying HOME/TMPDIR/TZ to a raw Command
    // (the spawn-architecture tripwire requires this builder).
    let environment = neomacs_test_env::CanonicalEnvironment::package(
        &home,
        &tmp,
        neomacs_test_env::PackagePaths {
            scratch_root: root.path().to_path_buf(),
            workspace_root: neomacs_infra::workspace_root(),
            runtime_dir: runtime,
            melpa_cache_root: root.path().to_path_buf(),
        },
    )
    .expect("canonical package environment for the oracle probe");
    let mut command = environment.command(&oracle);
    command
        .arg("--batch")
        .arg("--quick")
        .arg("--load")
        .arg(&startup)
        .arg("--eval")
        .arg(r#"(progn (require 'rfc6068) (princ "NEOMACS-JKA-OK"))"#);
    for (name, value) in prepared.process_environment() {
        command.env(name, value);
    }
    let output = command.output().expect("run the GNU oracle");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "GNU failed: {output:?}\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("NEOMACS-JKA-OK"),
        "the shadow library did not load:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let combined = format!("{stdout}{stderr}");
    assert!(
        !combined.contains("uncompressing"),
        "the source-parity startup must silence jka-compr's verbose decompression \
         messages, which otherwise paint the echo area:\n{combined}"
    );
}
