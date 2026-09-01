// SINGLE SOURCE OF TRUTH for the `neovm_jit_*` runtime-shim name set (R2-C2).
//
// This list is the one authoritative enumeration of every runtime shim an AOT
// `.so` may import. It is consumed THREE ways, all via `include!` of THIS file
// so they can never drift:
//   1. `jit/aot.rs` — `MIR_SHIM_NAMES` (salted into `ABI_TAG`, and the hard
//      emit-time import-subset guard `assert_aot_imports_exported`).
//   2. `neovm-core/build.rs` — exports each shim into the lib's dynamic symbol
//      table (`-rdynamic` + per-shim `--export-dynamic-symbol`) for integration
//      tests' `.so` imports to resolve.
//   3. `crates/neomacs/build.rs` — the same export for the production `neomacs`
//      binary, so the dump-time preload `.so`'s imports resolve at runtime.
//
// MUST stay in sync with the shim DEFINITIONS in `compile.rs` (`#[no_mangle] pub
// extern "C" fn neovm_jit_*`) and the `JIT_SHIM_ANCHOR` array. `include!`-ing a
// bare `const` keeps this usable both as a crate item (aot.rs) and as a local
// const inside each build.rs `main` (no module/use context required).
const NEOVM_JIT_SHIM_NAMES: &[&str] = &[
    "neovm_jit_apply",
    // logand/logior/logxor bitwise intrinsic — emitted by AOT baseline leaves
    // (its Op::Call classification runs under Some(obarray) at emit), so an AOT
    // `.so` may import it: MUST be host-exported + salted into ABI_TAG.
    "neovm_jit_arith_spec",
    "neovm_jit_backedge",
    "neovm_jit_builtin1",
    "neovm_jit_builtin2",
    "neovm_jit_builtin3",
    "neovm_jit_builtin_slice",
    "neovm_jit_call",
    "neovm_jit_call_spec",
    // R2 increment B2 (Op::Call spec-in-AOT): the three round-1 subr-speculation
    // shims are now emitted by AOT baseline leaves (find_spec_sites' Op::Call pass
    // runs at emit under Some(obarray)), so an AOT `.so` may import them — they
    // MUST be host-exported + salted (were JIT-only through increment A).
    "neovm_jit_call_subr_spec",
    // R2 increment A (CBSym-in-AOT): the two CallBuiltinSym intrinsic shims are now
    // emitted by AOT baseline leaves too (their classification is name-canonical +
    // obarray-free), so an AOT `.so` may import them — they MUST be in the exported
    // + salted set (was JIT-only through round 2).
    "neovm_jit_cbsym_read",
    "neovm_jit_cbsym_spec",
    "neovm_jit_cons",
    "neovm_jit_eq_incl_props_spec",
    "neovm_jit_eq_slow",
    "neovm_jit_gc_push",
    "neovm_jit_gc_push_many",
    "neovm_jit_gc_restore",
    "neovm_jit_rootwin_grow",
    "neovm_jit_gc_save",
    "neovm_jit_integerp_slow",
    "neovm_jit_list",
    "neovm_jit_match_handler",
    "neovm_jit_named_builtin",
    "neovm_jit_numberp_slow",
    "neovm_jit_pop_handler",
    "neovm_jit_pred_spec",
    "neovm_jit_push_catch",
    "neovm_jit_push_cc",
    "neovm_jit_push_cc_raw",
    "neovm_jit_save_current_buffer",
    "neovm_jit_save_excursion",
    "neovm_jit_save_restriction",
    "neovm_jit_save_window_excursion",
    "neovm_jit_switch",
    "neovm_jit_switch_stale",
    "neovm_jit_symbolp_slow",
    "neovm_jit_throw",
    "neovm_jit_unbind",
    "neovm_jit_unwind_protect",
    "neovm_jit_varbind",
    "neovm_jit_varref",
    "neovm_jit_varset",
];
