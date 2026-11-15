//! Freshness guard for the GNU DEFSYM boot-membership table.
//!
//! `crates/neovm-value/src/intern/gnu_defsyms.generated.rs` must be exactly
//! what the generator renders from the checked-in ledger
//! (`gnu_defsyms.snapshot.tsv`). The ledger is the extraction from the pinned
//! GNU 31.1 source; `cargo xtask gnu-defsyms --source <checkout>` rewrites it,
//! and `cargo xtask gnu-defsyms --check` is the same check outside the suite.
//! This test lives in core's architecture tests so it runs even while
//! neovm-value's own lib-test target is not in a gate recipe.

#[cfg(test)]
#[path = "../../../../../neovm-value/build_support/gnu_defsyms.rs"]
mod generator;

#[path = "../../../../../neovm-value/src/intern/gnu_defsyms.generated.rs"]
mod table;

#[test]
fn the_gnu_defsym_table_matches_the_ledger() {
    let root = super::architecture_production_source_roots::workspace_root();
    let rendered = generator::rendered(&root).expect("gnu-defsyms generator");
    let tracked = include_str!("../../../../../neovm-value/src/intern/gnu_defsyms.generated.rs");
    assert_eq!(
        rendered, tracked,
        "gnu_defsyms.generated.rs is stale; run `cargo xtask gnu-defsyms`"
    );
}

#[test]
fn the_ledger_keeps_the_make_docfile_order_and_gate_split() {
    let root = super::architecture_production_source_roots::workspace_root();
    let ledger_path = root.join(generator::LEDGER);
    let text = std::fs::read_to_string(&ledger_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", ledger_path.display()));
    let rows = generator::parse_ledger(&text).expect("ledger parses");

    // One row per spelling, in make-docfile order (Qnil, Qt, Qunbound, Qerror,
    // Qlambda first, then strcmp of the C identifier).
    let mut previous: Option<(u8, String)> = None;
    for row in &rows {
        let (rank, c_name) = generator::sort_key(row);
        if let Some((previous_rank, previous_c_name)) = previous.as_ref() {
            assert!(
                (*previous_rank, previous_c_name.as_str()) < (rank, c_name),
                "ledger is not in make-docfile order: `{previous_c_name}` then `{c_name}`"
            );
        }
        previous = Some((rank, c_name.to_string()));
    }

    let emitted = rows.iter().filter(|row| row.gate.is_emitted()).count();
    assert_eq!(
        emitted,
        table::GNU_DEFSYMS.len(),
        "the generated table and the ledger disagree on the emitted row count"
    );

    // The classification is measured, not aspirational: the common platform
    // set, the Windows-only spellings, the Cocoa-only spellings, the backends
    // this port does not build, and the spellings GNU uninterns must all be
    // represented, so a generator change that drops a class is loud here.
    for gate in [
        generator::Gate::Common,
        generator::Gate::Windows,
        generator::Gate::Macos,
        generator::Gate::Unsupported,
        generator::Gate::Uninterned,
    ] {
        assert!(
            rows.iter().any(|row| row.gate == gate),
            "ledger has no `{}` rows",
            gate.as_str()
        );
    }
}
