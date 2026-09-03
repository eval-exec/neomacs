use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::emacs_core::fileio::{MemoryFileSystem, RuntimeResourceNode, RuntimeResourceStore};
use crate::{Context, Value};

#[derive(Default)]
struct MemoryRuntimeResources {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl RuntimeResourceStore for MemoryRuntimeResources {
    fn mount_root(&self) -> &Path {
        Path::new("/neomacs")
    }

    fn node(&self, path: &Path) -> Option<RuntimeResourceNode<'_>> {
        if let Some(contents) = self.files.get(path) {
            Some(RuntimeResourceNode::File(contents))
        } else if self.files.keys().any(|file| {
            file.strip_prefix(path)
                .is_ok_and(|tail| tail.components().next().is_some())
        }) {
            Some(RuntimeResourceNode::Directory)
        } else {
            None
        }
    }

    fn directory_entries(&self, path: &Path) -> Option<Vec<OsString>> {
        if self.node(path) != Some(RuntimeResourceNode::Directory) {
            return None;
        }
        let entries = self
            .files
            .keys()
            .filter_map(|file| file.strip_prefix(path).ok())
            .filter_map(|tail| tail.components().next())
            .map(|component| component.as_os_str().to_owned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        Some(entries)
    }
}

fn evaluator_with_files(files: impl IntoIterator<Item = (&'static str, &'static [u8])>) -> Context {
    let mut resources = MemoryRuntimeResources::default();
    resources.files.extend(
        files
            .into_iter()
            .map(|(path, contents)| (PathBuf::from(path), contents.to_vec())),
    );

    let mut evaluator = Context::new();
    evaluator.install_runtime_resource_store(Box::new(resources));
    // Browser startup restores/finalizes its image with product resources
    // first, then installs the OPFS/tmp host namespace.
    evaluator.install_editor_file_system(Box::new(MemoryFileSystem::new()));
    evaluator.set_variable(
        "load-path",
        Value::list(vec![Value::string("/neomacs/lisp")]),
    );
    evaluator
}

#[test]
fn load_resolves_and_reads_lisp_from_the_context_runtime_mount() {
    let mut evaluator = evaluator_with_files([(
        "/neomacs/lisp/browser-runtime-probe.el",
        b"(setq browser-runtime-probe 73)\n".as_slice(),
    )]);

    let value = evaluator
        .eval_str("(progn (load \"browser-runtime-probe\") browser-runtime-probe)")
        .expect("mounted Lisp should be loadable through the ordinary load path");

    assert_eq!(value, Value::fixnum(73));
}

#[test]
fn require_resolves_and_reads_lisp_from_the_context_runtime_mount() {
    let mut evaluator = evaluator_with_files([(
        "/neomacs/lisp/browser-runtime-feature.el",
        b"(setq browser-runtime-feature-value 82)\n(provide 'browser-runtime-feature)\n".as_slice(),
    )]);

    let value = evaluator
        .eval_str("(progn (require 'browser-runtime-feature) browser-runtime-feature-value)")
        .expect("mounted Lisp should satisfy require through the ordinary load path");

    assert_eq!(value, Value::fixnum(82));
}

#[test]
fn autoload_resolves_and_reads_lisp_from_the_context_runtime_mount() {
    let mut evaluator = evaluator_with_files([(
        "/neomacs/lisp/browser-runtime-autoload.el",
        b"(fset 'browser-runtime-autoload (lambda () 91))\n".as_slice(),
    )]);

    let value = evaluator
        .eval_str(
            "(progn\
               (autoload 'browser-runtime-autoload \"browser-runtime-autoload\")\
               (browser-runtime-autoload))",
        )
        .expect("mounted Lisp should satisfy autoload through the ordinary load path");

    assert_eq!(value, Value::fixnum(91));
}

#[test]
fn ordinary_file_predicates_see_mounted_files_and_directories() {
    let mut evaluator =
        evaluator_with_files([("/neomacs/etc/NEWS", b"mounted release notes\n".as_slice())]);

    let visible = evaluator
        .eval_str(
            r#"(and (file-exists-p "/neomacs/etc/NEWS")
                     (file-readable-p "/neomacs/etc/NEWS")
                     (file-regular-p "/neomacs/etc/NEWS")
                     (file-directory-p "/neomacs/etc")
                     (file-accessible-directory-p "/neomacs/etc"))"#,
        )
        .expect("evaluate mounted resource predicates");

    assert_eq!(visible, Value::T);
    assert_eq!(
        evaluator
            .eval_str(r#"(file-writable-p "/neomacs/etc/NEWS")"#)
            .expect("check immutable mounted file"),
        Value::NIL,
    );
}

#[test]
fn insert_file_contents_reads_mounted_data_files() {
    let mut evaluator =
        evaluator_with_files([("/neomacs/etc/NEWS", b"mounted release notes\n".as_slice())]);

    let contents = evaluator
        .eval_str(
            r#"(progn
                  (erase-buffer)
                  (insert-file-contents "/neomacs/etc/NEWS")
                  (buffer-string))"#,
        )
        .expect("read mounted data through insert-file-contents");

    assert_eq!(contents.as_utf8_str(), Some("mounted release notes\n"));
}

#[test]
fn directory_files_enumerates_mounted_resource_children() {
    let mut evaluator = evaluator_with_files([
        ("/neomacs/etc/NEWS", b"news".as_slice()),
        ("/neomacs/etc/images/logo.txt", b"logo".as_slice()),
    ]);

    let names = evaluator
        .eval_str(r#"(directory-files "/neomacs/etc")"#)
        .expect("enumerate mounted data directory");

    assert_eq!(
        names,
        Value::list(vec![
            Value::string("."),
            Value::string(".."),
            Value::string("NEWS"),
            Value::string("images"),
        ])
    );
}

#[test]
fn root_directory_lists_the_immutable_runtime_mount() {
    let mut evaluator =
        evaluator_with_files([("/neomacs/etc/NEWS", b"mounted release notes\n".as_slice())]);

    let names = evaluator
        .eval_str(r##"(directory-files "/")"##)
        .expect("enumerate the unified filesystem root");

    assert_eq!(
        names,
        Value::list(vec![
            Value::string("."),
            Value::string(".."),
            Value::string("neomacs"),
        ])
    );
}

#[test]
fn copy_file_reads_an_immutable_runtime_resource_through_the_context_filesystem() {
    let mut evaluator =
        evaluator_with_files([("/neomacs/etc/NEWS", b"mounted release notes\n".as_slice())]);

    evaluator
        .eval_str(r##"(copy-file "/neomacs/etc/NEWS" "/copied-news")"##)
        .expect("copy-file should read across the immutable resource boundary");
    let contents = evaluator
        .eval_str(
            r##"(progn (erase-buffer) (insert-file-contents "/copied-news") (buffer-string))"##,
        )
        .expect("copied runtime data should be readable from mutable storage");

    assert_eq!(contents.as_utf8_str(), Some("mounted release notes\n"));
}

#[test]
fn deleting_an_immutable_runtime_resource_is_an_explicit_error() {
    let mut evaluator =
        evaluator_with_files([("/neomacs/etc/NEWS", b"mounted release notes\n".as_slice())]);

    let error = evaluator
        .eval_str(r##"(delete-file-internal "/neomacs/etc/NEWS")"##)
        .expect_err("immutable runtime resources must not look successfully deleted");

    assert!(matches!(
        error,
        crate::emacs_core::error::EvalError::Signal { .. }
    ));
    assert_eq!(
        evaluator
            .eval_str(r##"(file-exists-p "/neomacs/etc/NEWS")"##)
            .expect("resource must remain visible after rejected deletion"),
        Value::T,
    );
}
