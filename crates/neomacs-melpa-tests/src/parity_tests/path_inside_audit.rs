//! MELPA fixtures must not call GNU `file-in-directory-p`.
//!
//! That predicate confirms the shared root with `file-equal-p`
//! (lisp/files.el), which compares the full `file-attributes` list. A
//! directory shared by the suite changes mtime between those two reads, and
//! the check returns nil for a path that is inside. Fixture containment goes
//! through `neomacs-melpa-path-inside-p` in the package startup form.

use std::fs;
use std::path::Path;

#[test]
fn melpa_fixtures_do_not_call_file_in_directory_p() {
    let call = concat!("(", "file-in-directory-p");
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut offenders = Vec::new();
    for dir in ["src/parity_tests", "src/gui_parity_tests"] {
        collect_calls(&manifest.join(dir), call, &mut offenders);
    }
    assert!(
        offenders.is_empty(),
        "MELPA fixtures must call neomacs-melpa-path-inside-p:\n{}",
        offenders.join("\n")
    );
}

fn collect_calls(dir: &Path, call: &str, offenders: &mut Vec<String>) {
    let entries =
        fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("read entry: {error}"));
        let path = entry.path();
        if path.is_dir() {
            collect_calls(&path, call, offenders);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        for (index, line) in text.lines().enumerate() {
            if line.contains(call) {
                offenders.push(format!("{}:{}", path.display(), index + 1));
            }
        }
    }
}
