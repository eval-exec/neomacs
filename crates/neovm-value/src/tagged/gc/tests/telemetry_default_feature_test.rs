//! E3.3 static check: `gc-memory-telemetry` is an optional measurement build
//! only. Every default product feature set must exclude it, so the shipping
//! products exercise the stopped-GC ordinary-World gate even though a
//! telemetry build takes the explicit serial-only route.

use std::fs;
use std::path::{Path, PathBuf};

fn default_line_names_telemetry(manifest: &Path) -> bool {
    let Ok(text) = fs::read_to_string(manifest) else {
        return false;
    };
    let mut inside_default = false;
    for line in text.lines().map(str::trim) {
        if inside_default {
            if line.contains("gc-memory-telemetry") {
                return true;
            }
            if line.contains(']') {
                inside_default = false;
            }
            continue;
        }
        if !line.starts_with("default =") {
            continue;
        }
        if line.contains("gc-memory-telemetry") {
            return true;
        }
        // A multi-line array (`default = [` ... `]`) must be scanned whole;
        // the one-line match above is only the common case.
        inside_default = line.contains('[') && !line.contains(']');
    }
    false
}

#[test]
fn gc_memory_telemetry_is_in_no_default_product_feature_set() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root above crates/neovm-value");
    let mut manifests = vec![workspace.join("Cargo.toml")];
    for entry in fs::read_dir(workspace.join("crates")).expect("workspace crates directory") {
        manifests.push(entry.expect("crate entry").path().join("Cargo.toml"));
    }
    let offenders: Vec<PathBuf> = manifests
        .into_iter()
        .filter(|manifest| manifest.exists())
        .filter(|manifest| default_line_names_telemetry(manifest))
        .collect();
    assert!(
        offenders.is_empty(),
        "gc-memory-telemetry must stay out of every default feature set: {offenders:?}"
    );
}
