//! R1c call-bearing AOT — integration test (Linux-only).
//!
//! Drives the REAL production path: emit a call-bearing leaf's `.so`, place it in
//! `NEOVM_AOT_DIR`, and serve it through `try_run_compiled` under
//! `NEOVM_AOT=force`. This MUST be an integration test (not a lib unit test):
//! the host's `neovm_jit_*` shims are exported into the DYNAMIC symbol table only
//! for integration-test binaries (`-rdynamic` + `--export-dynamic-symbol`, see
//! neovm-core/build.rs), so a call/cons `.so`'s undefined shim imports resolve at
//! `dlopen`. The lib unit-test binary is NOT `-rdynamic`'d.
//!
//! The scenario logic lives in a `#[doc(hidden)] pub` crate-internal self-test
//! (it needs crate-private types: ByteCodeFunction internals, obarray, Vm); this
//! integration test just sets the env and calls it, so the whole thing runs in
//! the (shim-exporting) integration-test process.

#![cfg(all(feature = "jit", target_os = "linux"))]

#[test]
fn aot_call_bearing_deopt_across_call_side_effect_once_and_eq() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Set the AOT env BEFORE any AOT code runs (OnceLock-memoized gates).
    // SAFETY: single-threaded test setup before any AOT entry point reads these.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    // The crate-internal self-test does the full emit→place→serve-from-AOT→assert
    // (call-bearing serve + deopt-across-call side-effect-exactly-once + #A
    // eq-identity + #B non-UTF-8). Returns Err(reason) on any failure.
    let r = neovm_core::emacs_core::jit::aot::testkit_call_bearing_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("call-bearing AOT self-test failed: {e}");
    }
}

/// R2-E E1a: a BASELINE-tier AOT leaf (build_leaf_fn::<ObjectModule>(aot=true))
/// emits + serves AOT==interp, INCLUDING a FORCED fixnum→bignum overflow deopt —
/// the genuinely-new baseline-deopt-resume-via-sidecar path (must-nail #1).
/// Integration test because a baseline `.so` may import shims (this body is
/// builtin-free, but the binary's shim export is the safe place to exercise it).
#[test]
fn aot_baseline_tier_emit_serve_and_forced_deopt_match_interp() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: single-threaded setup before any AOT entry point reads these
    // (nextest isolates each test in its own process → no OnceLock cross-talk).
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_baseline_aot_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("baseline-tier AOT self-test failed: {e}");
    }
}

/// R2-E audit follow-up (test gap a): a baseline-AOT deopt at a DEEPER stack
/// (depth 4) with LIVE RAW unboxed slots — `(* (+ a 1) (+ b 1))` forced to
/// overflow at the outer Mul. Exercises the cold raw-slot-retag + deeper
/// framestate-spill path the `(* x x)` selftest (pc=2/depth=2, no live raw slot)
/// never reaches. Integration test for the deopt resume's shim path.
#[test]
fn aot_baseline_deep_rawslot_deopt_matches_interp() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: single-threaded setup before any AOT entry point reads these.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    let r =
        neovm_core::emacs_core::jit::aot::testkit_baseline_deep_rawslot_deopt_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("baseline deep raw-slot deopt self-test failed: {e}");
    }
}

/// R2-E E1b (must-nail #2): a baseline-tier AOT leaf calling a builtin via
/// `Op::CallBuiltinSym` serves AOT==interp, with the callee SymId reloc'd BY NAME
/// (recipe encodes the name; served result correct) — the cross-session-correct
/// op-SymId reloc. Integration test: as of increment A the `length` CBSym site
/// lowers to the Tier-B `neovm_jit_cbsym_spec` fast shim (was
/// `neovm_jit_named_builtin`), which needs this shim-exporting binary.
#[test]
fn aot_baseline_callbuiltinsym_relocs_callee_by_name_and_matches_interp() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: single-threaded setup before any AOT entry point reads these.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_callbuiltinsym_aot_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("CallBuiltinSym AOT self-test failed: {e}");
    }
}

/// R2 increment A (CBSym-in-AOT) — THE DLOPEN + FAST-SHIM proof. A baseline-tier
/// AOT leaf whose body is a CallBuiltinSym intrinsic now emits the FAST shim
/// (`neovm_jit_cbsym_read` for Tier-A `(point)` / `neovm_jit_cbsym_spec` for Tier-B
/// `(length x)`) instead of the general `neovm_jit_named_builtin` path. A served
/// `.so` must therefore bind those two newly-AOT-importable shims against the host
/// at `dlopen` and RUN them — proving the shim-export (`#[unsafe(no_mangle)] pub` +
/// `shim_names.rs` + `JIT_SHIM_ANCHOR` + per-shim `--export-dynamic-symbol`) works.
/// The self-test also asserts the host fast-path counter moved (fast shim fired, no
/// slow-path bounce) and the served result == interp. MUST be an integration test:
/// the host's `neovm_jit_*` shims are exported into the dynamic symbol table only
/// for integration-test binaries (see neovm-core/build.rs).
#[test]
fn aot_baseline_cbsym_intrinsic_hits_fast_shim_via_dlopen() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: single-threaded setup before any AOT entry point reads these.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_cbsym_aot_fast_shim_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("CBSym-in-AOT fast-shim self-test failed: {e}");
    }
}

/// R2-E audit CRITICAL fix: the OTHER baseline op-SymId sites — a SYMBOL
/// `Op::Constant` and the dynamic-var ops `VarRef`/`VarSet`/`VarBind` — also
/// reloc their session-specific SymId BY NAME (not bake it). Two baseline bodies
/// (each forced to the baseline tier by a co-occurring `CallBuiltinSym`): the
/// quoted-symbol-const body and the var-bind/set/ref body. Each asserts the
/// served leaf's reloc set contains the symbol/var by name (decoy-grown intern
/// table = cross-session drift) + result == interp. Integration test: the var +
/// builtin shims need this shim-exporting binary.
#[test]
fn aot_baseline_symbol_const_and_var_ops_reloc_sym_by_name() {
    let dir = tempfile::tempdir().expect("tempdir");
    // SAFETY: single-threaded setup before any AOT entry point reads these.
    unsafe {
        std::env::set_var("NEOVM_AOT", "force");
        std::env::set_var("NEOVM_AOT_DIR", dir.path());
    }
    let r = neovm_core::emacs_core::jit::aot::testkit_baseline_op_symbol_reloc_selftest(dir.path());
    unsafe {
        std::env::remove_var("NEOVM_AOT");
        std::env::remove_var("NEOVM_AOT_DIR");
    }
    if let Err(e) = r {
        panic!("baseline op-SymId reloc self-test failed: {e}");
    }
}
