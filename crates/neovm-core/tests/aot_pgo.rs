//! R2 increment C — AOT PGO persistence (Linux-only integration test).
//!
//! Persist proven-hot JIT leaves to `NEOVM_AOT_DIR` at shutdown so the NEXT session
//! serves them native + speculative from call 1. These tests drive the REAL
//! production path: emit a spec-bearing leaf's `.so` via the unified producer the
//! drain calls (`compile_leaf_to_object` with the live obarray), place it in
//! `NEOVM_AOT_DIR`, then LOAD + serve it through `try_run_compiled` under
//! `NEOVM_AOT=force` against a FRESH obarray. They MUST be integration tests (not
//! lib unit tests): the round-1 spec shims (`neovm_jit_pred_spec` /
//! `neovm_jit_call_subr_spec`) are exported into the DYNAMIC symbol table only for
//! `-rdynamic` test binaries (see neovm-core/build.rs), so a spec-bearing `.so`'s
//! undefined shim imports resolve at `dlopen`.
//!
//! The scenario logic lives in `#[doc(hidden)] pub` crate-internal self-tests (they
//! need crate-private types: obarray, Vm, ByteCodeFunction internals + the spec
//! counters); each integration test just sets the env and calls one, so the whole
//! thing runs in the (shim-exporting) integration-test process. Each runs ALONE in
//! its own binary/process (nextest), so the process-global `SUBR_SPEC_*` counters,
//! the `NEOVM_AOT*` OnceLock gates, and the frozen unit index are uncontended.

// debug_assertions: the self-tests assert on the debug-only SUBR_SPEC_* counters.
#![cfg(all(feature = "jit", target_os = "linux", debug_assertions))]

/// STEP 1 (GO/NO-GO): a pred-class body emitted via `compile_leaf_to_object` (the
/// drain's exact producer) round-trips runtime-emit → next-session-load: it serves
/// AOT-backed, fires the pred FAST shim FROM CALL 1, and equals the interpreter.
#[test]
fn pgo_roundtrip_runtime_emit_next_session_serves_fast_from_call_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Set the AOT env BEFORE any AOT code runs (OnceLock-memoized gates).
    // SAFETY: single-threaded test setup before any AOT entry point reads these;
    // nextest isolates each test in its own process → no OnceLock cross-talk.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_pgo_roundtrip_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("AOT-PGO round-trip self-test failed: {e}");
    }
}

/// STEP 2 (the drain): `drain_aot_pgo` persists a proven-hot JIT leaf under the
/// correct unit-index name, is idempotent (`.exists()` skip on a second drain), and
/// the persisted `.so` serves a FRESH session AOT-backed + pred-FAST-from-call-1.
#[test]
fn pgo_drain_persists_hot_leaf_and_next_session_serves_fast_from_call_1() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: single-threaded test setup before any AOT entry point reads these;
    // nextest isolates each test in its own process → no OnceLock cross-talk.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_PGO", "1");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
        // A bare `(recordp x)` is call-dominated, so the JIT profit gate would
        // decline it; disable the gate so it enters the JIT set (the drain's source)
        // as a spec-bearing leaf, mirroring a real profitable spec body without
        // arith (whose fixnum guard would deopt under the NEOVM_JIT_FORCE_DEOPT gate).
        std::env::set_var("NEOVM_JIT_PROFIT", "off");
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_pgo_drain_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_PGO");
        std::env::remove_var("NEOVM_AOT_DIR");
        std::env::remove_var("NEOVM_JIT_PROFIT");
    }
    if let Err(e) = r {
        panic!("AOT-PGO drain self-test failed: {e}");
    }
}

/// DEFAULT-OFF: with `NEOVM_AOT_PGO` UNSET, the drain writes nothing even though a
/// hot JIT leaf is staged and `NEOVM_AOT_DIR` is set (no surprise cache files). This
/// process deliberately never sets `NEOVM_AOT_PGO`.
#[test]
fn pgo_drain_default_off_writes_nothing() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: as above. NOTE: NEOVM_AOT_PGO is intentionally NOT set.
    unsafe {
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
        std::env::set_var("NEOVM_JIT_PROFIT", "off"); // stage the bare-call hot leaf
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_pgo_default_off_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT_DIR");
        std::env::remove_var("NEOVM_JIT_PROFIT");
    }
    if let Err(e) = r {
        panic!("AOT-PGO default-off self-test failed: {e}");
    }
}
