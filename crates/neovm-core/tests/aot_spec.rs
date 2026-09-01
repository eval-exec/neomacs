//! R2 increment B2 — `Op::Call` speculation-in-AOT cross-session soundness corpus
//! (Linux-only integration test).
//!
//! Drives the REAL production path: emit a spec-bearing baseline leaf's `.so`
//! (with an obarray so `find_spec_sites` bakes the `Op::Call` subr/bytecode spec
//! fast paths), place it in `NEOVM_AOT_DIR`, then LOAD + serve it through
//! `try_run_compiled` under `NEOVM_AOT=force` against a FRESH obarray. This MUST
//! be an integration test (not a lib unit test): the three round-1 spec shims
//! (`neovm_jit_call_subr_spec` / `neovm_jit_pred_spec` / `neovm_jit_eq_incl_props_spec`)
//! are exported into the DYNAMIC symbol table only for integration-test binaries
//! (`-rdynamic` + `--export-dynamic-symbol`, see neovm-core/build.rs), so a
//! spec-bearing `.so`'s undefined shim imports resolve at `dlopen`.
//!
//! The scenario logic lives in a `#[doc(hidden)] pub` crate-internal self-test (it
//! needs crate-private types: obarray, Vm, ByteCodeFunction internals + the spec
//! counters); this integration test just sets the env and calls it, so the whole
//! thing runs in the (shim-exporting) integration-test process. It runs ALONE in
//! its own binary, so the process-global `SUBR_SPEC_*` counters are uncontended.

// debug_assertions: the self-test asserts on the debug-only SUBR_SPEC_* counters.
#![cfg(all(feature = "jit", target_os = "linux", debug_assertions))]

/// The full B2 cross-session corpus: (a) armed cross-session + FAST-from-call-1 for
/// the pred + subr spec shims (dlopen e2e; eq shim is export/import-audit-covered),
/// (b/c) THE CRUX re-alias→DISARM (never `is_record` on the wrong type), (d) the
/// DISARMED slot never re-arms.
#[test]
fn aot_spec_cross_session_arm_disarm_and_fast_from_call_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Set the AOT env BEFORE any AOT code runs (OnceLock-memoized gates).
    // SAFETY: single-threaded test setup before any AOT entry point reads these;
    // nextest isolates each test in its own process → no OnceLock cross-talk.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_spec_aot_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("spec-in-AOT cross-session self-test failed: {e}");
    }
}
