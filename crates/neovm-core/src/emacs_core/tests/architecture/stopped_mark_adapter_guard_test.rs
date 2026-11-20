//! E3 stopped-mark adapters: slice 0 extracted the shared adapter, slice 1 put
//! the stopped major on it.
//!
//! `finish_stopped_mark_adapters` is the only production caller of the
//! post-mark steps. The serial stop (`complete_collection`), incremental
//! termination (`incremental_finish`), and the stopped major arm all call it;
//! the era path no longer calls `mark_and_sweep_weak_tables` under its test
//! `WeakSweepPermit`. Every step below the adapter is private, so the adapter
//! is their only caller.
//!
//! `mark_and_queue_doomed_finalizers`, `mark_and_sweep_weak_tables`, and the
//! three `reclaim_unreached_*` steps are private.

use super::architecture_plist_head_seam_guard_test::code_only;
use super::architecture_production_source_roots::production_source_files;

#[derive(Debug)]
struct Hit {
    workspace_relative: String,
    line: usize,
    enclosing: String,
    test_path: bool,
}

fn fn_name(line: &str) -> Option<String> {
    let mut rest = line.trim();
    if let Some(after_pub) = rest.strip_prefix("pub") {
        rest = after_pub.trim_start();
        if let Some(inside) = rest.strip_prefix('(') {
            let close = inside.find(')')?;
            rest = inside[close + 1..].trim_start();
        }
    }
    for _ in 0..2 {
        for prefix in ["const ", "async ", "unsafe "] {
            if let Some(after) = rest.strip_prefix(prefix) {
                rest = after;
            }
        }
    }
    let rest = rest.strip_prefix("fn ")?;
    let name: String = rest
        .chars()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

fn enclosing_fn(code: &str, start: usize) -> String {
    let mut offset = start;
    loop {
        let line_start = code[..offset]
            .rfind('\n')
            .map(|index| index + 1)
            .unwrap_or(0);
        if let Some(name) = fn_name(&code[line_start..offset.min(code.len())]) {
            return name;
        }
        if line_start == 0 {
            return String::new();
        }
        offset = line_start - 1;
    }
}

fn hits(needle: &str) -> Vec<Hit> {
    let mut found = Vec::new();
    for file in production_source_files() {
        let source = std::fs::read_to_string(&file.path)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.path.display()));
        let code = code_only(&source);
        let mut from = 0;
        while let Some(offset) = code[from..].find(needle) {
            let start = from + offset;
            found.push(Hit {
                line: code[..start].matches('\n').count() + 1,
                enclosing: enclosing_fn(&code, start),
                test_path: file.is_test_path(),
                workspace_relative: file.workspace_relative(),
            });
            from = start + needle.len();
        }
    }
    found
}

fn definition_line(relative_suffix: &str, signature: &str) -> String {
    let file = production_source_files()
        .into_iter()
        .find(|file| file.workspace_relative().ends_with(relative_suffix))
        .unwrap_or_else(|| panic!("missing {relative_suffix}"));
    let source = std::fs::read_to_string(&file.path).expect("read definition");
    let code = code_only(&source);
    code.lines()
        .find(|line| line.contains(signature))
        .unwrap_or_else(|| panic!("{signature} missing from {relative_suffix}"))
        .to_owned()
}

#[test]
fn stopped_mark_adapters_have_one_production_order() {
    let queue = hits(".mark_and_queue_doomed_finalizers(");
    assert_eq!(queue.len(), 1, "{queue:?}");
    assert!(
        queue[0]
            .workspace_relative
            .ends_with("tagged/gc/mark_sweep.rs"),
        "{queue:?}"
    );
    assert_eq!(queue[0].enclosing, "finish_stopped_mark_adapters");
    assert!(!queue[0].test_path);

    for needle in [
        ".reclaim_unreached_retired_processes(",
        ".reclaim_unreached_killed_buffers(",
        ".reclaim_unreached_deleted_frames(",
    ] {
        let found = hits(needle);
        assert_eq!(found.len(), 1, "{needle}: {found:?}");
        assert!(
            found[0]
                .workspace_relative
                .ends_with("tagged/gc/mark_sweep.rs"),
            "{needle}: {found:?}"
        );
        assert_eq!(
            found[0].enclosing, "finish_stopped_mark_adapters",
            "{needle}: {found:?}"
        );
        assert!(!found[0].test_path, "{needle}");
    }

    // Slice 1: the stopped major arm calls the adapter, so the weak sweep has
    // one production caller and no era-only call survives.
    let weak = hits(".mark_and_sweep_weak_tables(");
    assert_eq!(weak.len(), 1, "{weak:?}");
    assert!(
        weak[0]
            .workspace_relative
            .ends_with("tagged/gc/mark_sweep.rs")
            && weak[0].enclosing == "finish_stopped_mark_adapters"
            && !weak[0].test_path,
        "{weak:?}"
    );

    let adapter = hits(".finish_stopped_mark_adapters(");
    let production: Vec<_> = adapter.iter().filter(|hit| !hit.test_path).collect();
    assert_eq!(production.len(), 3, "{adapter:?}");
    assert!(
        production.iter().any(|hit| {
            hit.workspace_relative.ends_with("tagged/gc/mark_sweep.rs")
                && hit.enclosing == "complete_collection"
        }),
        "{production:?}"
    );
    assert!(
        production.iter().any(|hit| {
            hit.workspace_relative.ends_with("tagged/gc/incremental.rs")
                && hit.enclosing == "incremental_finish"
        }),
        "{production:?}"
    );
    assert!(
        production.iter().any(|hit| {
            hit.workspace_relative
                .ends_with("mutator_registration/collection.rs")
                && hit.enclosing == "run"
        }),
        "{production:?}"
    );
    let tests: Vec<_> = adapter
        .iter()
        .filter(|hit| hit.test_path)
        .map(|hit| hit.workspace_relative.as_str())
        .collect();
    assert_eq!(
        tests,
        ["neovm-value/src/tagged/gc/tests/memory_inventory_test.rs"],
        "{adapter:?}"
    );

    let queue_def = definition_line(
        "tagged/gc/mark_sweep.rs",
        "fn mark_and_queue_doomed_finalizers",
    );
    assert!(
        !queue_def.contains("pub"),
        "doomed-finalizer scan is private: {queue_def}"
    );
    let weak_def = definition_line("tagged/gc/mark_sweep.rs", "fn mark_and_sweep_weak_tables");
    assert!(
        !weak_def.contains("pub"),
        "weak sweep is private to the adapter: {weak_def}"
    );
}
