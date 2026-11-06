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

use std::path::PathBuf;

use super::super::architecture_production_source_roots::{
    production_file, production_file_in, production_source_files, production_source_roots,
};
use super::{code_only, find_all, test_gated_ranges};

const SEAM_CONSTRUCT: &str = "src/emacs_core/runtime/eval/construct.rs";
const SEAM_PUMP_RECONSTRUCT: &str = "src/emacs_core/runtime/eval/pdump_reconstruct.rs";
const CARRIED: &str = "src/emacs_core/runtime/eval/green/carried.rs";

/// N13's ONE authorized install site (pinned like an N9 seam): the core
/// lifecycle table wires the four carried pairs to the lower modules'
/// generic `world_state_placeholder`/`swap_world_state` implementations.
const LIFECYCLE_HOOKS_CRATE: &str = "neovm-core";
const LIFECYCLE_HOOKS_FILE: &str = "emacs_core/lifecycle_hooks.rs";
const LIFECYCLE_WORLD_STATE_SPELLINGS: [&str; 8] = [
    "crate::emacs_core::sqlite::world_state_placeholder",
    "crate::emacs_core::sqlite::swap_world_state",
    "crate::emacs_core::dbusbind::world_state_placeholder",
    "crate::emacs_core::dbusbind::swap_world_state",
    "crate::emacs_core::timefns::world_state_placeholder",
    "crate::emacs_core::timefns::swap_world_state",
    "crate::emacs_core::terminal::pure::world_state_placeholder",
    "crate::emacs_core::terminal::pure::swap_world_state",
];

fn production_rust_files() -> Vec<PathBuf> {
    production_source_files()
        .into_iter()
        .map(|file| file.path)
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

/// The path token around `offset` in comment/string-stripped code.
fn path_token_at(code: &str, offset: usize) -> &str {
    let bytes = code.as_bytes();
    let mut start = offset;
    while start > 0
        && (bytes[start - 1].is_ascii_alphanumeric()
            || bytes[start - 1] == b'_'
            || bytes[start - 1] == b':')
    {
        start -= 1;
    }
    let mut end = offset;
    while end < bytes.len()
        && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_' || bytes[end] == b':')
    {
        end += 1;
    }
    &code[start..end]
}

/// The pinned lifecycle install site: exactly the eight module-path
/// spellings of the four carried pairs' lower implementations, each once and
/// at its own offset. A ninth generic-name occurrence in the table is an
/// unpinned seam and fails; a missing spelling fails too (the seam stays
/// wired). The reference scan exempts this file only after this returns.
fn pinned_lifecycle_world_state_sites() -> PathBuf {
    let path = production_file_in(LIFECYCLE_HOOKS_CRATE, LIFECYCLE_HOOKS_FILE);
    let source = std::fs::read_to_string(&path).expect("read the core lifecycle hook table");
    let code = code_only(&source);
    let gated = test_gated_ranges(&code);
    let mut expected: Vec<usize> = Vec::new();
    let mut violations = Vec::new();
    for spelling in LIFECYCLE_WORLD_STATE_SPELLINGS {
        let live: Vec<usize> = find_all(&code, spelling)
            .into_iter()
            .filter(|&offset| {
                !gated
                    .iter()
                    .any(|&(start, end)| start <= offset && offset <= end)
                    && path_token_at(&code, offset) == spelling
            })
            .collect();
        if live.len() != 1 {
            violations.push(format!(
                "{spelling}: expected exactly one pinned spelling, found {}",
                live.len()
            ));
        }
        let name = spelling.rsplit("::").next().expect("path segment");
        for offset in live {
            expected.push(offset + spelling.len() - name.len());
        }
    }
    // Every generic-name occurrence in the table must be one of the pinned
    // spellings; anything else is a ninth install spelling.
    for name in ["world_state_placeholder", "swap_world_state"] {
        for offset in find_all(&code, name) {
            if gated
                .iter()
                .any(|&(start, end)| start <= offset && offset <= end)
            {
                continue;
            }
            if identifier_neighbour(&code, offset, name.len()) || is_declaration(&code, offset) {
                continue;
            }
            if !expected.contains(&offset) {
                let line = code[..offset].matches('\n').count() + 1;
                violations.push(format!(
                    "unpinned generic-name occurrence at {}:{line}",
                    path.display()
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "N13 seal: the lifecycle install site must be exactly the eight pinned \
         spellings:\n{}",
        violations.join("\n")
    );
    path
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
    let carried_path = production_file(CARRIED);
    let carried_source =
        std::fs::read_to_string(&carried_path).expect("read eval/green/carried.rs");
    let carriers_table = carried_source
        .split("const CARRIERS")
        .nth(1)
        .and_then(|rest| rest.split("];").next())
        .expect("CARRIERS table");
    // The one authorized install site is pinned up front: it must be exactly
    // the eight module-path spellings below, and its occurrences are exempt
    // from the general reference scan only after that pin holds.
    let lifecycle_hooks = pinned_lifecycle_world_state_sites();

    // Every carried piece's placeholder and swap, as the table spells them:
    // full paths, one pair per row. A row owned by the core crate is spelled
    // `crate::…`; a row whose definitions live in an extracted lower crate is
    // spelled with that crate's lib name (`neovm_text::…`, `neovm_value::…`).
    // The accepted roots are derived from the production source roots (crate
    // directory with `-` -> `_`), not a hand-kept crate list, so a future
    // extracted crate is covered automatically. The bare trailing segments
    // (`world_state_placeholder`, `swap_world_state`) repeat across rows, so
    // names are the deduplicated last segments — well below the row count —
    // while the pair count is the path count halved.
    let mut accepted_roots: Vec<String> = vec!["crate".to_owned()];
    accepted_roots.extend(
        production_source_roots()
            .into_iter()
            .map(|root| root.crate_dir.replace('-', "_")),
    );
    let mut paths: Vec<&str> = carriers_table
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == ':'))
        .filter(|token| {
            token
                .split_once("::")
                .is_some_and(|(first, _)| accepted_roots.iter().any(|root| root == first))
        })
        .collect();
    paths.sort_unstable();
    paths.dedup();
    // Count rows on the comment/literal-stripped table text so a later
    // spelling change cannot silently drop a row while the paths still look
    // plausible: every row is one `(placeholder, swap),` tuple.
    let rows = code_only(carriers_table).matches("),").count();
    assert_eq!(
        paths.len() / 2,
        rows,
        "CARRIERS extraction found {} pairs but the table has {rows} rows: {paths:?}",
        paths.len() / 2
    );
    assert!(
        paths.len() >= 26 && paths.len() % 2 == 0,
        "CARRIERS extraction lost pairs: {paths:?}"
    );
    let mut names: Vec<&str> = paths
        .iter()
        .map(|path| path.rsplit("::").next().expect("path segment"))
        .collect();
    names.sort_unstable();
    names.dedup();

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
            // A carried name is defined either by its own `fn name(` decl or
            // by a renaming import (`world_state_placeholder as <name>`) in
            // its module — the three named pairs (selection, property
            // revision, display) re-export the shared helpers under module-
            // specific names.
            let declaration = format!("fn {name}(");
            let alias = format!(" as {name}");
            let defined = find_all(&code, &declaration)
                .into_iter()
                .chain(code.match_indices(&alias).map(|(offset, _)| offset))
                .any(|offset| !gated.iter().any(|&(s, e)| s <= offset && offset <= e));
            if defined {
                definers.push(path.clone());
            }
        }
        assert!(
            !definers.is_empty(),
            "N13 seal: carried name {name} has no production definition"
        );
        for site in production_sites(name) {
            let (file, line) = site.rsplit_once(':').expect("path:line");
            // The pinned lifecycle table's two generic names are checked as
            // one unit above; every other carried name in that file still
            // goes through the base rules below.
            if std::path::Path::new(file) == lifecycle_hooks.as_path()
                && matches!(name, "world_state_placeholder" | "swap_world_state")
            {
                continue;
            }
            // A renaming import binds the name; it is not a reference. The
            // plain pair members appear as the source of the three named
            // aliases (window, selection, property revision).
            let use_binding = std::fs::read_to_string(file).ok().is_some_and(|source| {
                // Line numbers in `site` count the comment-stripped
                // text, so walk the same text.
                let stripped = code_only(&source);
                let lines: Vec<&str> = stripped.lines().collect();
                let mut index = match line.parse::<usize>().expect("line number").checked_sub(1) {
                    Some(zero_based) if zero_based < lines.len() => zero_based,
                    _ => return false,
                };
                // Walk back at most a use-tree's worth of lines: if we
                // find a `use` opener before any statement-ending `;`,
                // this mention is an import binding (possibly the source
                // side of a rename), not a reference.
                let mut walked = 0;
                loop {
                    let text = lines[index].trim_start();
                    // `use`, `pub use`, `pub(crate) use`, …: any use
                    // statement opener with a visibility prefix.
                    if text.starts_with("use ") || text.contains(" use ") {
                        return true;
                    }
                    if lines[index].contains(';') || index == 0 || walked == 12 {
                        return false;
                    }
                    index -= 1;
                    walked += 1;
                }
            });
            let defining_file = definers
                .iter()
                .any(|definer| file == definer.display().to_string());
            if !defining_file && !use_binding && !file.ends_with(CARRIED) {
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
