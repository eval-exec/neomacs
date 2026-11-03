//! Exact call-site guards for the split's mint seals (peer rulings N9, N11,
//! N13; PEER-RULINGS.md 2026-11-02/03).
//!
//! * N9 — the one-time lower-declared, upper-installed hooks
//!   (`install_string_bytes_changed_hook`, `install_scratch_root_hooks`) may
//!   be called only at the two evaluator seams: `Context` construction and
//!   image restore. Any third install site is a new authority seam and must
//!   be ruled on, not slipped in.
//! * N11 — the pdump stub walker `for_each_stub_bytecode_child` keeps its
//!   GC-context `unsafe` contract; its callers are exactly the two GC
//!   phases plus the walker's own test.
//! * N13 — every carried World-state pair
//!   (`(world_state_placeholder, swap_world_state)` and its named variants)
//!   is referenced in production only by `eval/green/carried.rs` and the
//!   module that defines it. A later pair joins the table automatically:
//!   the names are read from `CARRIERS`, not pinned here.
//!
//! Reuse the plist seam's code-only scanner so comments, literals and
//! test-gated items obey the same scanning rule.

use std::path::{Path, PathBuf};

use super::{code_only, find_all, source_files, test_gated_ranges};

const SEAM_CONSTRUCT: &str = "src/emacs_core/runtime/eval/construct.rs";
const SEAM_PUMP_RECONSTRUCT: &str = "src/emacs_core/runtime/eval/pdump_reconstruct.rs";
const CARRIED: &str = "src/emacs_core/runtime/eval/green/carried.rs";

fn source_root() -> PathBuf {
    // This file lives at src/emacs_core/tests/architecture/.
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn production_rust_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    source_files(&source_root(), &mut files);
    files
        .into_iter()
        .filter(|path| {
            let text = path.to_string_lossy();
            !text.contains("/tests/") && !text.contains("/test_")
        })
        .collect()
}

/// Occurrences of `needle` in production code (comments and strings
/// stripped, `#[cfg(test)]` items skipped), as `path:line` strings.
/// Declaration sites (`fn needle(`) are skipped: the guards pin call sites.
fn production_sites(needle: &str) -> Vec<String> {
    let mut sites = Vec::new();
    for path in production_rust_files() {
        let Ok(source) = std::fs::read_to_string(&path) else {
            continue;
        };
        let code = code_only(&source);
        let gated = test_gated_ranges(&code);
        for offset in find_all(&code, needle) {
            if gated
                .iter()
                .any(|&(start, end)| start <= offset && offset <= end)
            {
                continue;
            }
            if identifier_neighbour(&code, offset, needle.len()) || is_declaration(&code, offset) {
                continue;
            }
            let line = code[..offset].matches('\n').count() + 1;
            sites.push(format!("{}:{line}", path.display()));
        }
    }
    sites
}

/// Whether the match is glued to a larger identifier (a longer name that
/// merely contains the needle).  Needles that end in `(` stop at their open
/// parenthesis, so only the left edge can glue.
fn identifier_neighbour(code: &str, offset: usize, len: usize) -> bool {
    let before = code[..offset].chars().next_back();
    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        return true;
    }
    let needle_tail_is_ident = code[offset..offset + len]
        .chars()
        .next_back()
        .is_some_and(|c| c.is_alphanumeric() || c == '_');
    let after = code[offset + len..].chars().next();
    needle_tail_is_ident && after.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// Whether the occurrence is the item's own declaration (`fn needle(`).
fn is_declaration(code: &str, offset: usize) -> bool {
    code[..offset].trim_end().ends_with("fn")
}

#[test]
fn n9_hook_installs_live_only_at_the_two_evaluator_seams() {
    for installer in [
        "install_string_bytes_changed_hook(",
        "install_scratch_root_hooks(",
    ] {
        let violations: Vec<String> = production_sites(installer)
            .into_iter()
            .filter(|site| {
                let file = site.rsplit_once(':').expect("path:line").0;
                !(file.ends_with(SEAM_CONSTRUCT) || file.ends_with(SEAM_PUMP_RECONSTRUCT))
            })
            .collect();
        assert!(
            violations.is_empty(),
            "N9 seal: {installer} called outside the two evaluator seams:\n{}",
            violations.join("\n")
        );
    }
}

#[test]
fn n11_stub_walker_callers_are_exactly_the_gc_phases() {
    let sites = production_sites("for_each_stub_bytecode_child(");
    let mut violations: Vec<String> = sites
        .into_iter()
        .filter(|site| {
            let file = site.rsplit_once(':').expect("path:line").0;
            !(file.ends_with("src/tagged/gc/incremental.rs")
                || file.ends_with("src/tagged/gc/mark_sweep.rs")
                || file.ends_with("src/tagged/pdump_image.rs"))
        })
        .collect();
    violations.sort();
    assert!(
        violations.is_empty(),
        "N11 seal: for_each_stub_bytecode_child called outside the pinned GC \
         phases (incremental.rs, mark_sweep.rs) and its own definition:\n{}",
        violations.join("\n")
    );
}

#[test]
fn n13_carried_world_state_pairs_are_referenced_only_from_carried() {
    let carried_path = source_root().join(CARRIED.strip_prefix("src/").expect("src-prefixed"));
    let carried_source =
        std::fs::read_to_string(&carried_path).expect("read eval/green/carried.rs");
    let carriers_table = carried_source
        .split("const CARRIERS")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .expect("CARRIERS table");

    // Every carried piece's placeholder and swap names, including the named
    // variants (`selection_`, `property_revision_`, `display_`).
    let mut names: Vec<&str> = carriers_table
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| {
            token.ends_with("_world_state_placeholder")
                || (token.starts_with("swap_") && token.ends_with("_world_state"))
        })
        .collect();
    names.sort_unstable();
    names.dedup();
    assert!(
        names.len() >= 20,
        "CARRIERS extraction lost pairs: {names:?}"
    );

    // Each name may appear in production only in carried.rs and in the
    // files that define it (each defining module owns its own pair).
    let mut violations = Vec::new();
    for name in names {
        let mut definers = Vec::new();
        for path in production_rust_files() {
            let Ok(source) = std::fs::read_to_string(&path) else {
                continue;
            };
            let code = code_only(&source);
            let gated = test_gated_ranges(&code);
            let declaration = format!("fn {name}(");
            if find_all(&code, &declaration)
                .into_iter()
                .any(|offset| !gated.iter().any(|&(s, e)| s <= offset && offset <= e))
            {
                definers.push(path.clone());
            }
        }
        assert!(
            !definers.is_empty(),
            "N13 seal: carried name {name} has no production definition"
        );
        for site in production_sites(name) {
            let file = site.rsplit_once(':').expect("path:line").0;
            let defining_file = definers
                .iter()
                .any(|definer| file == definer.display().to_string());
            if !defining_file && !file.ends_with(CARRIED) {
                violations.push(format!("{name} at {site}"));
            }
        }
    }
    violations.sort();
    assert!(
        violations.is_empty(),
        "N13 seal: carried World-state pairs referenced outside carried.rs \
         and their defining modules:\n{}",
        violations.join("\n")
    );
}
