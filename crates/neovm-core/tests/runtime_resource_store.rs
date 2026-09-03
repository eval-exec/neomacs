use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use neovm_core::emacs_core::fileio::RuntimeResourceStore;
use neovm_core::{Context, Value};

#[derive(Default)]
struct MemoryRuntimeResources {
    files: BTreeMap<PathBuf, Vec<u8>>,
}

impl RuntimeResourceStore for MemoryRuntimeResources {
    fn file_contents(&self, path: &Path) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
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
