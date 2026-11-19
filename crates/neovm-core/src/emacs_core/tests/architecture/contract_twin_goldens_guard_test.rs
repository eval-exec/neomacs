//! Architecture guard: arm64 macOS contract goldens must track their x86 twin.
//!
//! Since d3f1fb0f2c every compile-fail contract runner selects fixtures through
//! `tests/thread_safety_ui/support/fixture_target.rs`: on aarch64-apple-darwin
//! an overlay under `<dir>/arm64_macos/` (an `include!` of the common fixture)
//! supplies the expected `.stderr`; Linux compiles the common fixture and
//! gates the common golden. Nothing re-blessed the overlays when batches 12
//! and 15 narrowed types, so exactly three arm64 overlays went stale while
//! their x86 twins were re-blessed (CI red at 34fa80beb2: flow_word
//! context_send/context_sync and p1rv era_cannot_be_shared).
//!
//! The peer's rule: for every `arm64_macos/**/*.stderr` that has an x86 twin,
//! the two must be identical modulo known per-target lines. This guard
//! projects each side through the same explicit normalization and compares
//! the result as a multiset:
//!
//! * drop the location lines (`-->` / `:::`): the overlay's `include!` path
//!   and span gutters legitimately differ;
//! * strip the line-number gutter (`^\d*\s*\|`), collapse whitespace, and
//!   drop separator-only lines: snippet indentation/line numbers differ
//!   between an included file and its source;
//! * drop trybuild chrome (the `┈` separators and the TRYBUILD=overwrite
//!   note) and the known macOS-only objc2 help block (rustc suggests
//!   `objc2::__macro_helpers` trait imports only on Darwin);
//! * compare as a MULTISET, not a sequence: rustc emits the "doesn't
//!   satisfy" note groups in a target-dependent order while their content is
//!   identical (verified for the flow_word twins).
//!
//! A stale overlay loses the type-path rendering its twin carries
//! (`header::FinalizerObj`, `keymap_lookup::SelectedGlobalMap`, ...), so the
//! multisets differ and the guard names both files. A missing twin is not
//! this guard's subject: the generic overlays have no common fixture.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::architecture_production_source_roots::workspace_root;

/// Sanity floor: the contract tree has 28 overlay/twin pairs (12 generic
/// overlays intentionally have no common fixture). If the scanner finds
/// fewer, the tree moved and the guard would pass vacuously.
const MIN_TWIN_PAIRS: usize = 20;

fn stderr_under_arm64(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries {
        let path = entry.expect("tests entry").path();
        if path.is_dir() {
            stderr_under_arm64(&path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "stderr")
            && path
                .components()
                .any(|component| component.as_os_str() == "arm64_macos")
        {
            found.push(path);
        }
    }
}

/// The common-fixture twin of an overlay: remove the `arm64_macos` component
/// (overlays are one or two levels below it: `<dir>/arm64_macos/name.stderr`
/// and `<dir>/arm64_macos/<group>/name.stderr`).
fn twin_of(overlay: &Path) -> Option<PathBuf> {
    let mut parts: Vec<_> = overlay.components().collect();
    let index = parts
        .iter()
        .position(|component| component.as_os_str() == "arm64_macos")?;
    parts.remove(index);
    let twin: PathBuf = parts.iter().collect();
    twin.is_file().then_some(twin)
}

fn normalize(text: &str) -> Vec<String> {
    let mut normalized = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("-->") || trimmed.starts_with(":::") {
            continue;
        }
        if line.contains("objc2::__macro_helpers")
            || trimmed == "= help: items from traits can only be used if the trait is in scope"
            || trimmed.starts_with("help: the following traits which provide `")
            || line.contains("If the actual output is the correct output you can bless it")
            || line.contains("your test with the environment variable TRYBUILD=overwrite")
            || trimmed.starts_with('┈')
        {
            continue;
        }
        let without_gutter = strip_line_number_gutter(line);
        let collapsed = without_gutter
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if collapsed.is_empty() || collapsed == "|" {
            continue;
        }
        normalized.push(collapsed);
    }
    normalized
}

fn strip_line_number_gutter(line: &str) -> &str {
    let bytes = line.as_bytes();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
        index += 1;
    }
    if index < bytes.len() && bytes[index] == b'|' {
        &line[index..]
    } else {
        line
    }
}

fn multiset(text: &str) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for line in normalize(text) {
        *counts.entry(line).or_insert(0) += 1;
    }
    counts
}

fn asymmetric_lines(
    left: &BTreeMap<String, usize>,
    right: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut lines = Vec::new();
    for (line, count) in left {
        let missing = count.saturating_sub(*right.get(line).unwrap_or(&0));
        for _ in 0..missing {
            lines.push(format!("only x86 twin: {line}"));
        }
    }
    for (line, count) in right {
        let missing = count.saturating_sub(*left.get(line).unwrap_or(&0));
        for _ in 0..missing {
            lines.push(format!("only arm64 overlay: {line}"));
        }
    }
    lines.truncate(12);
    lines
}

#[test]
fn arm64_contract_goldens_match_their_x86_twins() {
    let tests = workspace_root().join("crates/neovm-core/tests");
    assert!(
        tests.is_dir(),
        "contract test tree missing at {}",
        tests.display()
    );

    let mut overlays = Vec::new();
    stderr_under_arm64(&tests, &mut overlays);
    overlays.sort();

    let mut pairs = 0usize;
    let mut failures = Vec::new();
    for overlay in &overlays {
        let Some(twin) = twin_of(overlay) else {
            continue;
        };
        pairs += 1;
        let overlay_text = std::fs::read_to_string(overlay)
            .unwrap_or_else(|error| panic!("read {}: {error}", overlay.display()));
        let twin_text = std::fs::read_to_string(&twin)
            .unwrap_or_else(|error| panic!("read {}: {error}", twin.display()));
        let overlay_set = multiset(&overlay_text);
        let twin_set = multiset(&twin_text);
        if overlay_set != twin_set {
            let detail = asymmetric_lines(&twin_set, &overlay_set).join("\n  ");
            failures.push(format!(
                "{} does not track its x86 twin {} modulo known per-target lines:\n  {detail}",
                overlay.strip_prefix(&tests).unwrap_or(overlay).display(),
                twin.strip_prefix(&tests).unwrap_or(&twin).display(),
            ));
        }
    }

    assert!(
        pairs >= MIN_TWIN_PAIRS,
        "contract twin scanner found only {pairs} overlay/twin pairs (expected at least \
         {MIN_TWIN_PAIRS}); the test tree moved and this guard would pass vacuously"
    );
    assert!(
        failures.is_empty(),
        "stale arm64 contract goldens (re-bless the overlay on macmini, never copy the \
         x86 text; a legitimate new per-target line needs an explicit normalization rule \
         with its reason):\n{}",
        failures.join("\n")
    );
}
