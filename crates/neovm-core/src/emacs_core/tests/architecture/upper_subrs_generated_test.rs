//! Freshness guard for the H3 unit-runtime upper-subr source.
//!
//! The tracked `upper_subrs.generated.rs` must be exactly what the restored
//! H3 generator renders from the H3 ledger (name/min/max/layer/cfg), and the
//! ledger must still match the canonical plan's non-eval rows. The xtask
//! command is the write path; this test is the in-suite ratchet that fails on
//! a hand edit or a stale ledger. It lives in core's architecture tests so it
//! runs even while eval's own lib-test target is still post-split red.

#[cfg(test)]
#[path = "../../../../../neovm-eval/build_support/upper_subrs_generated.rs"]
mod generator;

#[test]
fn the_upper_subrs_source_matches_the_ledger() {
    let root = super::architecture_production_source_roots::workspace_root();
    let rendered = generator::rendered(&root).expect("upper-subrs generator");
    let tracked = include_str!("../../../../../neovm-eval/src/upper_subrs.generated.rs");
    assert_eq!(
        rendered, tracked,
        "upper_subrs.generated.rs is stale; run `cargo xtask upper-subrs`"
    );
}
