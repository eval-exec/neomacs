//! A metadata-only filesystem exercises integer widths without allocating a
//! huge file or depending on the test machine's inode/owner assignments.

use crate::emacs_core::eval::Context;
use crate::emacs_core::fileio::{
    AccessMode, EditorFileSystem, FileAttributeSnapshot, FileAttributeType, FileIdentity,
    FileMetadata, FilePrincipal, WriteRequest,
};
use crate::emacs_core::value::Value;
use std::{
    ffi::OsString,
    io,
    path::{Path, PathBuf},
};

struct LargeAttributes;

fn unsupported<T>() -> io::Result<T> {
    Err(io::ErrorKind::Unsupported.into())
}

impl EditorFileSystem for LargeAttributes {
    fn attributes(&self, path: &Path) -> io::Result<FileAttributeSnapshot> {
        if path != Path::new("/large") {
            return Err(io::ErrorKind::NotFound.into());
        }
        Ok(FileAttributeSnapshot {
            kind: FileAttributeType::Other,
            links: Some(1 << 62),
            user: Some(FilePrincipal {
                id: i64::MAX,
                name: None,
            }),
            group: Some(FilePrincipal {
                id: i64::MAX,
                name: None,
            }),
            accessed: None,
            modified: None,
            changed: None,
            len: u64::MAX,
            mode: None,
            identity: Some(FileIdentity {
                inode: u64::MAX,
                device: u64::MAX,
            }),
            legacy_group_change: false,
        })
    }
    fn metadata(&self, _: &Path, _: bool) -> io::Result<FileMetadata> {
        unsupported()
    }
    fn access(&self, _: &Path, _: AccessMode) -> bool {
        false
    }
    fn read(&self, _: &Path) -> io::Result<Vec<u8>> {
        unsupported()
    }
    fn read_directory(&self, _: &Path) -> io::Result<Vec<OsString>> {
        unsupported()
    }
    fn write(&self, _: &Path, _: &[u8], _: WriteRequest) -> io::Result<FileMetadata> {
        unsupported()
    }
    fn create_directory(&self, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
    fn remove_file(&self, _: &Path) -> io::Result<()> {
        unsupported()
    }
    fn remove_directory(&self, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
    fn rename(&self, _: &Path, _: &Path, _: bool) -> io::Result<()> {
        unsupported()
    }
    fn canonicalize(&self, _: &Path) -> io::Result<PathBuf> {
        unsupported()
    }
}

#[test]
fn file_attributes_preserve_integers_larger_than_target_fixnums() {
    let mut eval = Context::new();
    eval.install_editor_file_system(Box::new(LargeAttributes));
    assert_eq!(
        eval.eval_str(
            r##"(let ((a (file-attributes "/large" 'integer)))
      (and (= (nth 1 a) 4611686018427387904)
           (= (nth 2 a) 9223372036854775807)
           (= (nth 3 a) 9223372036854775807)
           (= (nth 7 a) 18446744073709551615)
           (= (nth 10 a) 18446744073709551615)
           (= (nth 11 a) 18446744073709551615)))"##
        )
        .unwrap(),
        Value::T
    );
}

#[test]
fn file_attributes_unknown_principal_names_fall_back_to_integers() {
    // GNU src/dired.c returns the numeric uid/gid when name lookup fails,
    // even when ID-FORMAT requests strings.
    let mut eval = Context::new();
    eval.install_editor_file_system(Box::new(LargeAttributes));
    assert_eq!(
        eval.eval_str(
            r##"(let ((a (file-attributes "/large" 'string)))
      (and (integerp (nth 2 a)) (integerp (nth 3 a))
           (= (nth 2 a) 9223372036854775807)
           (= (nth 3 a) 9223372036854775807)))"##
        )
        .unwrap(),
        Value::T
    );
}
