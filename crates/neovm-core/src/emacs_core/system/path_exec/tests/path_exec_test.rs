//! Every shipped layout gets a row here: the probe in
//! [`super::path_exec_candidates`] is only as good as the set of trees it was
//! written against, and a packaging script that stages a directory this file
//! does not name is a script whose binary will not find its own dump.

use super::*;
use tempfile::tempdir;

/// GNU's macOS bundle: `ns_applibexecdir=${ns_appdir}/Contents/MacOS/libexec`
/// (`configure.ac:2792`).  The executable is `Contents/MacOS/<name>`, so the
/// archlib is the `libexec` directory beside it.
#[test]
fn bundle_libexec_beside_the_executable_wins() {
    let dir = tempdir().expect("tempdir");
    let macos = dir.path().join("neomacs.app/Contents/MacOS");
    let libexec = macos.join("libexec");
    std::fs::create_dir_all(&libexec).expect("stage bundle");

    let resolved = path_exec_for_executable(&macos.join("neomacs"));

    assert_eq!(resolved.dir(), libexec.as_path());
    assert_eq!(resolved.source(), PathExecSource::BundleLibexec);
    assert!(resolved.source().is_installed());
}

/// GNU's `archlibdir='${libexecdir}/emacs/${version}/${configuration}'`
/// (`configure.ac:290`) reached from `<prefix>/bin/emacs`.
#[test]
fn installed_prefix_resolves_to_the_versioned_archlib() {
    let dir = tempdir().expect("tempdir");
    let prefix = dir.path().join("usr");
    let bin = prefix.join("bin");
    let archlib = prefix.join(archlib_relative_path());
    std::fs::create_dir_all(&bin).expect("stage bin");
    std::fs::create_dir_all(&archlib).expect("stage archlib");

    let resolved = path_exec_for_executable(&bin.join("neomacs"));

    assert_eq!(resolved.dir(), archlib.as_path());
    assert_eq!(resolved.source(), PathExecSource::InstalledArchLib);
}

/// GNU `init_callproc`, `src/callproc.c:1986`: "Running uninstalled, so
/// default to tem rather than PATH_EXEC".  A cargo build tree ships no
/// `libexec`, so `PATH_EXEC` must stay the directory holding the executable,
/// `neomacsclient` and the dump -- which is what this port answered before
/// the concept existed, and must keep answering.
#[test]
fn build_tree_without_libexec_falls_back_to_the_executable_directory() {
    let dir = tempdir().expect("tempdir");
    let release = dir.path().join("target/release");
    std::fs::create_dir_all(&release).expect("stage build tree");

    let resolved = path_exec_for_executable(&release.join("neomacs"));

    assert_eq!(resolved.dir(), release.as_path());
    assert_eq!(resolved.source(), PathExecSource::Uninstalled);
    assert!(!resolved.source().is_installed());
}

/// The bundle shape is probed before the installed-prefix shape, so a tree
/// that happens to have both is unambiguous.
#[test]
fn bundle_libexec_outranks_the_installed_archlib() {
    let dir = tempdir().expect("tempdir");
    let prefix = dir.path().join("usr");
    let bin = prefix.join("bin");
    let beside = bin.join(LIBEXEC);
    let archlib = prefix.join(archlib_relative_path());
    std::fs::create_dir_all(&beside).expect("stage beside");
    std::fs::create_dir_all(&archlib).expect("stage archlib");

    let resolved = path_exec_for_executable(&bin.join("neomacs"));

    assert_eq!(resolved.dir(), beside.as_path());
}

/// A non-directory named `libexec` is not an archlib.  Probing must test for
/// a directory, not mere existence, or a stray file silently captures
/// `exec-directory`.
#[test]
fn a_libexec_file_is_not_an_archlib() {
    let dir = tempdir().expect("tempdir");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("stage bin");
    std::fs::write(bin.join(LIBEXEC), b"not a directory").expect("stage decoy");

    let resolved = path_exec_for_executable(&bin.join("neomacs"));

    assert_eq!(resolved.dir(), bin.as_path());
    assert_eq!(resolved.source(), PathExecSource::Uninstalled);
}

/// The candidate list is the contract the packaging scripts stage against;
/// pin its shape so a reordering has to be deliberate.
#[test]
fn candidate_order_is_bundle_then_archlib_then_uninstalled() {
    let candidates = path_exec_candidates(Path::new("/opt/neomacs/bin/neomacs"));

    assert_eq!(
        candidates,
        vec![
            (
                PathBuf::from("/opt/neomacs/bin/libexec"),
                PathExecSource::BundleLibexec
            ),
            (
                PathBuf::from("/opt/neomacs").join(archlib_relative_path()),
                PathExecSource::InstalledArchLib
            ),
            (
                PathBuf::from("/opt/neomacs/bin"),
                PathExecSource::Uninstalled
            ),
        ]
    );
}

/// GNU spells the archlib tail `emacs/${version}/${configuration}`
/// (`configure.ac:290`); ours substitutes the product name and carries the
/// real Rust target triple, never the pinned `system-configuration` spelling.
#[test]
fn archlib_relative_path_mirrors_gnu_archlibdir() {
    assert_eq!(
        archlib_relative_path(),
        PathBuf::from("libexec")
            .join("neomacs")
            .join(ARCHLIB_VERSION)
            .join(HOST_TRIPLE)
    );
    assert!(
        HOST_TRIPLE.contains('-'),
        "host triple should be a target triple, got {HOST_TRIPLE:?}"
    );
    assert!(
        !ARCHLIB_VERSION.is_empty(),
        "archlib version should be the workspace version"
    );
}
