#![cfg(not(target_family = "wasm"))]

use std::cell::RefCell;
use std::io::Cursor;

use flate2::{Compression, GzBuilder};
use neomacs_app::runtime_resources::{
    RUNTIME_RESOURCE_ARCHIVE_ASSET, RUNTIME_RESOURCE_ID_ASSET, RuntimeResourceInstall,
    RuntimeResourceRoot,
};
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
fn packaged_runtime_resources_install_atomically_and_reuse_by_content_id() {
    let archive = runtime_archive(&[
        ("lisp/loadup.el", b"(provide 'loadup)"),
        ("etc/NEWS", b"news"),
        ("leim/leim-list.el", b"input methods"),
    ]);
    let id = bundle_id(&archive);
    let storage = tempfile::tempdir().unwrap();
    let opened = RefCell::new(Vec::new());
    let installed = RuntimeResourceRoot::prepare(storage.path(), |asset| {
        opened.borrow_mut().push(asset.to_owned());
        Ok(Cursor::new(match asset {
            RUNTIME_RESOURCE_ID_ASSET => id.as_bytes().to_vec(),
            RUNTIME_RESOURCE_ARCHIVE_ASSET => archive.clone(),
            other => panic!("unexpected asset {other}"),
        }))
    })
    .unwrap();

    assert_eq!(installed.install(), RuntimeResourceInstall::Installed);
    assert_eq!(installed.path(), storage.path().join(&id));
    assert_eq!(
        std::fs::read(installed.path().join("lisp/loadup.el")).unwrap(),
        b"(provide 'loadup)"
    );
    assert_eq!(
        opened.into_inner(),
        [RUNTIME_RESOURCE_ID_ASSET, RUNTIME_RESOURCE_ARCHIVE_ASSET]
    );

    let opened = RefCell::new(Vec::new());
    let reused = RuntimeResourceRoot::prepare(storage.path(), |asset| {
        opened.borrow_mut().push(asset.to_owned());
        assert_eq!(asset, RUNTIME_RESOURCE_ID_ASSET);
        Ok(Cursor::new(id.as_bytes().to_vec()))
    })
    .unwrap();
    assert_eq!(reused.install(), RuntimeResourceInstall::Reused);
    assert_eq!(opened.into_inner(), [RUNTIME_RESOURCE_ID_ASSET]);
}

#[test]
fn packaged_runtime_resources_reject_unowned_archive_roots() {
    let archive = runtime_archive(&[
        ("lisp/loadup.el", b"lisp"),
        ("etc/NEWS", b"etc"),
        ("bin/unowned", b"must not escape the runtime contract"),
    ]);
    let id = bundle_id(&archive);
    let storage = tempfile::tempdir().unwrap();

    let error = RuntimeResourceRoot::prepare(storage.path(), |asset| {
        Ok(Cursor::new(match asset {
            RUNTIME_RESOURCE_ID_ASSET => id.as_bytes().to_vec(),
            RUNTIME_RESOURCE_ARCHIVE_ASSET => archive.clone(),
            other => panic!("unexpected asset {other}"),
        }))
    })
    .unwrap_err();

    assert!(error.to_string().contains("unowned runtime resource path"));
    assert!(!storage.path().join(id).exists());
}

#[test]
fn packaged_runtime_resources_reject_archive_digest_mismatch() {
    let archive = runtime_archive(&[("lisp/loadup.el", b"lisp"), ("etc/NEWS", b"etc")]);
    let storage = tempfile::tempdir().unwrap();

    let error = RuntimeResourceRoot::prepare(storage.path(), |asset| {
        Ok(Cursor::new(match asset {
            RUNTIME_RESOURCE_ID_ASSET => vec![b'0'; 64],
            RUNTIME_RESOURCE_ARCHIVE_ASSET => archive.clone(),
            other => panic!("unexpected asset {other}"),
        }))
    })
    .unwrap_err();

    assert!(error.to_string().contains("digest does not match"));
}

#[test]
fn packaged_runtime_resources_reject_noncanonical_bundle_ids_before_extraction() {
    let storage = tempfile::tempdir().unwrap();
    let opened = RefCell::new(Vec::new());

    let error = RuntimeResourceRoot::prepare(storage.path(), |asset| {
        opened.borrow_mut().push(asset.to_owned());
        assert_eq!(asset, RUNTIME_RESOURCE_ID_ASSET);
        Ok(Cursor::new(b"../../not-an-id".to_vec()))
    })
    .unwrap_err();

    assert!(error.to_string().contains("64 lowercase hexadecimal"));
    assert_eq!(opened.into_inner(), [RUNTIME_RESOURCE_ID_ASSET]);
}
