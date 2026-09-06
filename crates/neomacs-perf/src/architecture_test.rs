//! Architecture pins for the harness/scenarios split: the engine stays an
//! engine, and scenario families stay in their own child modules.

use std::path::PathBuf;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn count_lines(relative: &str) -> usize {
    let path = crate_root().join(relative);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
        .lines()
        .count()
}

/// `harness.rs` was reduced from 4,547 to ~1,830 lines by moving every
/// scenario family's preparation, result schema, invariants, measurements,
/// and provenance manifest below `harness/scenarios/`. What remains is the
/// engine: editor launch, the sampling gate, capture, artifact publication,
/// and the one-line scenario dispatch. New scenario work goes in a child
/// module for its family (or a new one), not in the orchestrator.
#[test]
fn harness_stays_an_engine_after_the_scenario_split() {
    const CEILING: usize = 2_200;
    let lines = count_lines("src/harness.rs");
    assert!(
        lines <= CEILING,
        "src/harness.rs is {lines} lines (ceiling {CEILING}); put the new code in the \
         harness/scenarios/ child module for its family instead of growing the engine"
    );
}

/// Each scenario family owns one child module. A family growing past this
/// ceiling is doing enough to deserve splitting its own children (fixtures,
/// generators, wire codecs) the way the engine did.
#[test]
fn scenario_modules_stay_family_sized() {
    const CEILING: usize = 900;
    for module in [
        "bytecode",
        "editor_workload",
        "mx_tab",
        "org_journal_open",
        "rust_lsp",
        "sustained_native_video",
    ] {
        let relative = format!("src/harness/scenarios/{module}.rs");
        let lines = count_lines(&relative);
        assert!(
            lines <= CEILING,
            "{relative} is {lines} lines (ceiling {CEILING}); split the family's own \
             children instead of growing it"
        );
    }
}

/// The registry stays closed: exactly these six families, so adding a
/// scenario is a deliberate act that also touches the catalog, the dispatch,
/// and this pin.
#[test]
fn scenario_module_registry_is_pinned() {
    let registered: Vec<&str> = include_str!("harness/scenarios/mod.rs")
        .lines()
        .filter_map(|line| line.strip_prefix("pub(crate) mod "))
        .map(|rest| rest.trim_end_matches(';'))
        .collect();
    assert_eq!(
        registered,
        [
            "bytecode",
            "editor_workload",
            "mx_tab",
            "org_journal_open",
            "rust_lsp",
            "sustained_native_video",
        ],
        "update this pin when registering a scenario family"
    );
}
