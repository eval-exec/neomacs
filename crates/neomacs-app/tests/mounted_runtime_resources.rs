use std::path::Path;

use neomacs_app::runtime_resources::{
    MountedRuntimeResources, RuntimeResourceBundle, RuntimeResourceError,
};
use neovm_core::emacs_core::fileio::RuntimeResourceStore;
use support::runtime_resources::{content_id, mounted_runtime_resources, runtime_archive};

mod support;

#[test]
fn runtime_resource_bundle_rejects_a_noncanonical_identity() {
    let archive = runtime_archive(&[("lisp/loadup.el", b"lisp"), ("etc/NEWS", b"etc")]);

    let error = RuntimeResourceBundle::from_assets(&archive, b"NOT-A-SHA-256-ID").unwrap_err();

    assert!(matches!(error, RuntimeResourceError::InvalidBundleId));
}

#[test]
fn authenticated_bundle_mounts_runtime_files_beneath_the_selected_root() {
    let mounted = mounted_runtime_resources(&[
        ("lisp/loadup.el", b"(provide 'loadup)"),
        ("etc/NEWS", b"news"),
        ("leim/leim-list.el", b"input methods"),
    ]);

    assert_eq!(
        mounted.file_contents(Path::new("/neomacs/lisp/loadup.el")),
        Some(b"(provide 'loadup)".as_slice()),
    );
    assert_eq!(
        mounted.file_contents(Path::new("/neomacs/etc/NEWS")),
        Some(b"news".as_slice()),
    );
    assert_eq!(
        mounted.file_contents(Path::new("lisp/loadup.el")),
        None,
        "the resource store must not impersonate paths outside its mount root",
    );
}

#[test]
fn mounted_runtime_resources_reject_a_digest_mismatch() {
    let archive = runtime_archive(&[("lisp/loadup.el", b"lisp"), ("etc/NEWS", b"etc")]);

    let bundle = RuntimeResourceBundle::from_assets(&archive, &[b'0'; 64]).unwrap();
    let error = MountedRuntimeResources::from_bundle(Path::new("/neomacs"), bundle).unwrap_err();

    assert!(matches!(
        error,
        RuntimeResourceError::ArchiveDigestMismatch { .. }
    ));
}

#[test]
fn mounted_runtime_resources_reject_paths_outside_owned_runtime_roots() {
    let archive = runtime_archive(&[
        ("lisp/loadup.el", b"lisp"),
        ("etc/NEWS", b"etc"),
        ("bin/neomacs", b"not a runtime resource"),
    ]);
    let id = content_id(&archive);

    let bundle = RuntimeResourceBundle::from_assets(&archive, id.as_bytes()).unwrap();
    let error = MountedRuntimeResources::from_bundle(Path::new("/neomacs"), bundle).unwrap_err();

    assert!(matches!(
        error,
        RuntimeResourceError::UnownedArchivePath(path) if path == Path::new("bin/neomacs")
    ));
}

#[test]
fn mounted_runtime_resources_reject_file_directory_path_conflicts() {
    for entries in [
        [
            ("lisp/pkg", b"file".as_slice()),
            ("lisp/pkg/load.el", b"child".as_slice()),
            ("etc/NEWS", b"etc".as_slice()),
        ],
        [
            ("lisp/pkg/load.el", b"child".as_slice()),
            ("lisp/pkg", b"file".as_slice()),
            ("etc/NEWS", b"etc".as_slice()),
        ],
    ] {
        let archive = runtime_archive(&entries);
        let id = content_id(&archive);
        let bundle = RuntimeResourceBundle::from_assets(&archive, id.as_bytes()).unwrap();

        let error =
            MountedRuntimeResources::from_bundle(Path::new("/neomacs"), bundle).unwrap_err();

        assert!(matches!(
            error,
            RuntimeResourceError::ConflictingArchivePath(path) if path == Path::new("lisp/pkg")
        ));
    }
}

#[test]
fn mounted_runtime_resources_require_lisp_and_data_trees() {
    let archive = runtime_archive(&[("lisp/loadup.el", b"lisp")]);
    let id = content_id(&archive);

    let bundle = RuntimeResourceBundle::from_assets(&archive, id.as_bytes()).unwrap();
    let error = MountedRuntimeResources::from_bundle(Path::new("/neomacs"), bundle).unwrap_err();

    assert!(matches!(
        error,
        RuntimeResourceError::MissingRequiredDirectory("etc")
    ));
}

#[test]
fn files_named_after_required_roots_do_not_count_as_runtime_trees() {
    let archive = runtime_archive(&[("lisp", b"not a tree"), ("etc", b"not a tree")]);
    let id = content_id(&archive);

    let bundle = RuntimeResourceBundle::from_assets(&archive, id.as_bytes()).unwrap();
    let error = MountedRuntimeResources::from_bundle(Path::new("/neomacs"), bundle).unwrap_err();

    assert!(matches!(
        error,
        RuntimeResourceError::MissingRequiredDirectory("lisp")
    ));
}
