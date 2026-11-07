//! Per-crate ratchet: callers of the default-only variable accessor may not
//! grow.
//!
//! The default-only accessor (`Context::default_value_only` /
//! `default_value_only_by_name`, and the `EditHost` forwarding of the same)
//! reads the variable's DEFAULT-ONLY value: for a buffer-local variable the
//! default (owner) binding, never the current buffer's binding. GNU spells
//! these reads `Fdefault_value` (src/data.c) or
//! `BVAR (&buffer_defaults, slot)` (src/buffer.c). A GNU C read of a
//! `DEFVAR_*` global, or a Lisp `symbol-value`, is the VISIBLE read and uses
//! `Context::builtin_var_value` / `builtin_var_value_by_name` instead.
//!
//! Counts at the C2 rename:
//!
//! - `neovm-eval` 3 -- the audited residue; every one is a genuine GNU
//!   default read and carries a GNU citation at the site:
//!   * `runtime/eval/mod.rs` `buffer_default_value` and
//!     `buffer_default_value_fixed`: GNU `BVAR (&buffer_defaults, slot)` for a
//!     slot-backed per-buffer variable (src/buffer.c:1116, 2289), else the
//!     variable's default cell;
//!   * `lisp/print/mod.rs` `bind_unreadable_printer_default`: GNU
//!     `specbind`'s LET_DEFAULT default-cell save (src/eval.c:3674-3710) for
//!     the print-unreadable-function nil binding at src/print.c:1711, which
//!     runs after print_prepare's destination-buffer switch
//!     (src/print.c:143-151). The printer_settings_gnu_*_local_callback
//!     fixtures require it.
//! - `neomacs-layout-engine` 30 and `neomacs` 11 -- UNAUDITED display-side
//!   callers. They are frozen at their pre-rename counts here and are NOT
//!   allowed: they are the queued, push-blocking P2-CORE-BACKLOG row
//!   "DISPLAY-SIDE DEFAULT-ONLY READS", which classifies each against GNU and
//!   migrates it to the visible read (overlay-arrow, mode/header-line and
//!   menu-bar variables are likely visible-binding).
//! - every other crate 0.
//!
//! The audited crates' total is 3 today. It may only DECREASE (the
//! print_buffer follow-up is expected to remove the binder read and lower the
//! baseline to 2); any new caller must be justified with GNU source lines here
//! and at the accessor site.

use std::path::{Path, PathBuf};

/// Frozen per-crate caller counts at the C2 rename. Every crate with a caller
/// is listed; a crate absent from this list must have none.
const BASELINE: &[(&str, usize)] = &[
    // Audited by the P2-CORE-BACKLOG default-reads audit; total must be 3.
    ("neovm-edit", 0),
    ("neovm-display", 0),
    ("neovm-system", 0),
    ("neovm-eval", 3),
    ("neovm-core", 0),
    // UNAUDITED display-side callers, frozen; see the module docs. These are
    // NOT allowed: P2-CORE-BACKLOG "DISPLAY-SIDE DEFAULT-ONLY READS" owns
    // classifying and migrating them.
    ("neomacs-layout-engine", 30),
    ("neomacs", 11),
];

/// The five crates the default-reads audit covered.
const AUDITED: &[&str] = &[
    "neovm-edit",
    "neovm-display",
    "neovm-system",
    "neovm-eval",
    "neovm-core",
];

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn count_crate(root: &Path, crate_name: &str) -> Vec<String> {
    fn walk(dir: &Path, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name == "tests" || name == "target" || name.starts_with("moved_") {
                    continue;
                }
                walk(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for (index, line) in text.lines().enumerate() {
                    let is_call = line.contains(".default_value_only(")
                        || line.contains(".default_value_only_by_name(");
                    if !is_call || line.contains("fn default_value_only") {
                        continue;
                    }
                    out.push(format!("{}:{}: {}", path.display(), index + 1, line.trim()));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(&root.join("crates").join(crate_name).join("src"), &mut out);
    out
}

/// Every crate directory that carries a `src/` tree.
fn workspace_crates(root: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(root.join("crates"))
        .expect("crates directory")
        .flatten()
        .filter(|entry| entry.path().join("src").is_dir())
        .filter_map(|entry| entry.file_name().to_str().map(str::to_owned))
        .collect();
    names.sort();
    names
}

#[test]
fn default_value_only_callers_never_grow() {
    let root = workspace_root();
    let mut problems = Vec::new();
    let mut audited_total = 0;
    for crate_name in workspace_crates(&root) {
        let sites = count_crate(&root, &crate_name);
        let baseline = BASELINE
            .iter()
            .find_map(|&(name, count)| (name == crate_name).then_some(count))
            .unwrap_or(0);
        if AUDITED.contains(&crate_name.as_str()) {
            audited_total += sites.len();
        }
        if sites.len() > baseline {
            let note = if AUDITED.contains(&crate_name.as_str()) {
                "audited crate: a new caller must be a genuine GNU Fdefault_value read, \
                 justified with GNU source lines at the site"
            } else {
                "UNAUDITED display-side crate: new default-only reads are not allowed; \
                 use the visible read (P2-CORE-BACKLOG DISPLAY-SIDE DEFAULT-ONLY READS)"
            };
            problems.push(format!(
                "{crate_name}: {} caller(s), frozen at {baseline} ({note}):\n  {}",
                sites.len(),
                sites.join("\n  ")
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "the default-only accessor grew (audited total {audited_total}, expected 3):\n{}",
        problems.join("\n")
    );
    assert_eq!(
        audited_total, 3,
        "the audited default-only accessor's total changed ({audited_total}, expected 3). If a \
         caller was legitimately removed, lower the per-crate BASELINE here; if one was added, it \
         must be a genuine GNU Fdefault_value read justified at the site with GNU source lines."
    );
}
