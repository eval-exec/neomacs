use std::io::ErrorKind;
use std::path::Path;

use super::*;

fn write_request(mode: WriteMode) -> WriteRequest {
    WriteRequest { mode, sync: true }
}

#[test]
fn memory_filesystem_preserves_complete_editor_write_semantics() {
    let filesystem = MemoryFileSystem::new();
    filesystem
        .create_directory(Path::new("/home/editor"), true)
        .expect("create virtual home");

    filesystem
        .write(
            Path::new("/home/editor/init.el"),
            b"alpha",
            write_request(WriteMode::CreateNew),
        )
        .expect("create file exclusively");
    assert_eq!(
        filesystem
            .write(
                Path::new("/home/editor/init.el"),
                b"duplicate",
                write_request(WriteMode::CreateNew),
            )
            .expect_err("exclusive create must reject an existing file")
            .kind(),
        ErrorKind::AlreadyExists,
    );

    filesystem
        .write(
            Path::new("/home/editor/init.el"),
            b"-beta",
            write_request(WriteMode::Append),
        )
        .expect("append file");
    filesystem
        .write(
            Path::new("/home/editor/init.el"),
            b"ALPHA",
            write_request(WriteMode::At(0)),
        )
        .expect("overwrite file prefix without truncating its suffix");
    assert_eq!(
        filesystem
            .read(Path::new("/home/editor/init.el"))
            .expect("read file"),
        b"ALPHA-beta",
    );
}

#[test]
fn memory_filesystem_renames_trees_and_enumerates_immediate_children() {
    let filesystem = MemoryFileSystem::new();
    filesystem
        .create_directory(Path::new("/workspace/tree/nested"), true)
        .expect("create tree");
    filesystem
        .write(
            Path::new("/workspace/tree/nested/note"),
            b"persistent",
            write_request(WriteMode::Truncate),
        )
        .expect("create note");

    filesystem
        .rename(
            Path::new("/workspace/tree"),
            Path::new("/workspace/moved"),
            false,
        )
        .expect("rename complete tree");

    assert!(!filesystem.access(Path::new("/workspace/tree"), AccessMode::Exists));
    assert_eq!(
        filesystem
            .read(Path::new("/workspace/moved/nested/note"))
            .expect("read renamed descendant"),
        b"persistent",
    );
    assert_eq!(
        filesystem
            .read_directory(Path::new("/workspace/moved"))
            .expect("enumerate directory"),
        vec![std::ffi::OsString::from("nested")],
    );
}

#[test]
fn memory_filesystem_rejects_paths_that_escape_its_root() {
    let filesystem = MemoryFileSystem::new();
    let error = filesystem
        .create_directory(Path::new("/safe/../../outside"), true)
        .expect_err("virtual paths must not escape their root");
    assert_eq!(error.kind(), ErrorKind::InvalidInput);
}
