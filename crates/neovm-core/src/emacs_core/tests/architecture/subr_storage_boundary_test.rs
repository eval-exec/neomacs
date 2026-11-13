//! T2 storage/code separation, including cfg(test) adapters.
use std::path::{Path, PathBuf};

use super::architecture_production_source_roots::{
    production_dir_in, production_file, production_source_files,
};
fn sources(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).expect("source directory") {
        let p = entry.expect("source entry").path();
        if p.is_dir() {
            out.extend(sources(&p));
        } else if p.extension().is_some_and(|ext| ext == "rs") {
            out.push(p);
        }
    }
    out.sort();
    out
}
#[test]
fn subr_storage_has_no_upper_entry_type_or_module() {
    for p in sources(&production_dir_in("neovm-value", "tagged")) {
        let text = std::fs::read_to_string(&p).expect("source");
        assert!(
            !text.contains("SubrFn"),
            "upper entry type under {}",
            p.display()
        );
        assert!(
            !text.contains("eval::subr_abi"),
            "upper entry module under {}",
            p.display()
        );
    }
}
#[test]
fn subr_factory_constructor_and_refresh_have_one_production_site() {
    let mut factory = Vec::new();
    let mut constructor = Vec::new();
    let mut updater = Vec::new();
    for file in production_source_files() {
        if file.relative.split('/').any(|part| part == "tests") {
            continue;
        }
        let text = std::fs::read_to_string(&file.path).expect("source");
        for line in text.lines() {
            if line.contains("impl CanonicalSubrFactory for ") {
                factory.push(file.path.clone());
            }
            if line.contains("SubrObj::new_unchecked(") {
                constructor.push(file.path.clone());
            }
            if line.contains("::update_static_subr_object_entry(") {
                updater.push(file.path.clone());
            }
        }
    }
    assert_eq!(
        factory,
        vec![production_file("src/emacs_core/runtime/eval/subr_abi.rs")]
    );
    assert_eq!(
        constructor,
        vec![production_file("src/emacs_core/runtime/eval/subr_abi.rs")]
    );
    assert_eq!(
        updater,
        vec![production_file("src/emacs_core/runtime/eval/mod.rs")]
    );
}
