//! T2 storage/code separation, including cfg(test) adapters.
use std::path::{Path, PathBuf};
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
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for p in sources(&src.join("tagged")) {
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
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut factory = Vec::new();
    let mut constructor = Vec::new();
    let mut updater = Vec::new();
    for p in sources(&src) {
        if p.components().any(|part| part.as_os_str() == "tests") {
            continue;
        }
        let text = std::fs::read_to_string(&p).expect("source");
        for line in text.lines() {
            if line.contains("impl CanonicalSubrFactory for ") {
                factory.push(p.clone());
            }
            if line.contains("SubrObj::new_unchecked(") {
                constructor.push(p.clone());
            }
            if line.contains("::update_static_subr_object_entry(") {
                updater.push(p.clone());
            }
        }
    }
    assert_eq!(
        factory,
        vec![src.join("emacs_core/runtime/eval/subr_abi.rs")]
    );
    assert_eq!(
        constructor,
        vec![src.join("emacs_core/runtime/eval/subr_abi.rs")]
    );
    assert_eq!(updater, vec![src.join("emacs_core/runtime/eval/mod.rs")]);
}
