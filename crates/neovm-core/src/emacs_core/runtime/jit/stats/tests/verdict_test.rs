use super::verdict::{MirVerdict, profile_row};

/// Pre-build gates accumulate, joined by `+`; nothing else joins them.
#[test]
fn jit_mir_verdict_joins_pre_build_gates() {
    let mut v = MirVerdict::default();
    assert_eq!(v.render(), None, "no event, no verdict");
    v.gate("gate_rest");
    v.gate("gate_opt");
    v.gate("gate_prefix");
    assert_eq!(
        v.render().as_deref(),
        Some("gate_rest+gate_opt+gate_prefix")
    );
}

/// The first bail key names the verdict: a failed lowering records its
/// adapter key before the generic `lower:` one.
#[test]
fn jit_mir_verdict_first_bail_wins() {
    let mut v = MirVerdict::default();
    v.bail("opaque-bind:SaveExcursion");
    v.bail("lower:UnsupportedOp(\"mir-pure-shim-op\")");
    assert_eq!(v.render().as_deref(), Some("opaque-bind:SaveExcursion"));
    // A later gate does not join a bail key (`gate:` is a tier-gate key, not
    // a pre-build gate).
    let mut g = MirVerdict::default();
    g.bail("gate:loop-opaque:Call");
    g.gate("gate_opt");
    assert_eq!(g.render().as_deref(), Some("gate:loop-opaque:Call"));
}

/// `taken` replaces anything noted before it.
#[test]
fn jit_mir_verdict_taken() {
    let mut v = MirVerdict::default();
    v.taken();
    assert_eq!(v.render().as_deref(), Some("taken"));
}

/// The rendered verdict is one report token: a `key=value` field and a CSV
/// column, whatever the bail key contains.
#[test]
fn jit_mir_verdict_renders_one_token() {
    let mut v = MirVerdict::default();
    v.bail("build:UnsupportedOp(\"inconsistent stack depth\"),x=y");
    assert_eq!(
        v.render().as_deref(),
        Some("build:UnsupportedOp(\"inconsistent_stack_depth\")_x_y")
    );
}

/// `#mir` profile rows have three columns (the census reader, which keeps
/// rows with at least 13, skips them); a compile with no verdict prints `-`.
#[test]
fn jit_mir_verdict_profile_row() {
    assert_eq!(profile_row("17", Some("taken")), "#mir,17,taken\n");
    assert_eq!(profile_row("-", None), "#mir,-,-\n");
    let row = profile_row("3", Some("gate:generic-call:call+cbsym"));
    assert_eq!(row.trim_end().split(',').count(), 3, "{row}");
}
