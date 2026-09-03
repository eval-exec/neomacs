use std::io::Cursor;
use std::path::Path;

use flate2::{Compression, GzBuilder};
use neomacs_app::runtime_resources::{MountedRuntimeResources, RuntimeResourceError};
use neovm_core::emacs_core::fileio::RuntimeResourceStore;
use sha2::{Digest, Sha256};
use tar::{Builder, Header};

fn runtime_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let encoder = GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), Compression::fast());
    let mut archive = Builder::new(encoder);
    for (path, contents) in entries {
        let mut header = Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o644);
        header.set_uid(0);
        header.set_gid(0);
        header.set_mtime(0);
        header.set_cksum();
        archive
            .append_data(&mut header, path, Cursor::new(*contents))
            .unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap()
}

fn bundle_id(archive: &[u8]) -> String {
    Sha256::digest(archive)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn authenticated_bundle_mounts_runtime_files_beneath_the_selected_root() {
    let archive = runtime_archive(&[
        ("lisp/loadup.el", b"(provide 'loadup)"),
        ("etc/NEWS", b"news"),
        ("leim/leim-list.el", b"input methods"),
    ]);
    let id = bundle_id(&archive);

    let mounted =
        MountedRuntimeResources::from_bundle(Path::new("/neomacs"), &archive, id.as_bytes())
            .expect("authenticate and mount runtime bundle");

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

    let error = MountedRuntimeResources::from_bundle(Path::new("/neomacs"), &archive, &[b'0'; 64])
        .unwrap_err();

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
    let id = bundle_id(&archive);

    let error =
        MountedRuntimeResources::from_bundle(Path::new("/neomacs"), &archive, id.as_bytes())
            .unwrap_err();

    assert!(matches!(
        error,
        RuntimeResourceError::UnownedArchivePath(path) if path == Path::new("bin/neomacs")
    ));
}

#[test]
fn mounted_runtime_resources_require_lisp_and_data_trees() {
    let archive = runtime_archive(&[("lisp/loadup.el", b"lisp")]);
    let id = bundle_id(&archive);

    let error =
        MountedRuntimeResources::from_bundle(Path::new("/neomacs"), &archive, id.as_bytes())
            .unwrap_err();

    assert!(matches!(
        error,
        RuntimeResourceError::MissingRequiredDirectory("etc")
    ));
}
