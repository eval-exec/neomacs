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
// extern "C" fn neovm_jit_*`) and the `JIT_SHIM_TABLE` array. `include!`-ing a
// bare `const` keeps this usable both as a crate item (aot.rs) and as a local
// const inside each build.rs `main` (no module/use context required).
const NEOVM_JIT_SHIM_NAMES: &[&str] = &[
    "neovm_jit_hof_length",
    "neovm_jit_hof_start",
    "neovm_jit_hof_store",
    "neovm_jit_hof_cursor",
    "neovm_jit_hof_finish",
    "neovm_jit_hof_abort",
    "neovm_jit_apply",
    // `Op::Aref` / `Op::Aset` / `Op::Memq` / `Op::Assq`: value-returning shims
    // (baseline lowering, so AOT leaves import them too).
    "neovm_jit_aref",
    "neovm_jit_aset",
    "neovm_jit_assq",
    "neovm_jit_memq",
    // `Op::Setcar` / `Op::Setcdr`: value-returning shims, same contract.
    "neovm_jit_setcar",
    "neovm_jit_setcdr",
    // logand/logior/logxor bitwise intrinsic — emitted by AOT baseline leaves
    // (its Op::Call classification runs under Some(obarray) at emit), so an AOT
    // `.so` may import it: MUST be host-exported + salted into ABI_TAG.
    "neovm_jit_arith_spec",
    // Generic fallback of a feedback-`Other` arithmetic site (JIT-only today:
    // AOT publishes no feedback, so no AOT leaf emits it).
    "neovm_jit_arith_generic",
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
    // Boxes an f64 the compiled code computed in a register, for the
    // float-feedback arithmetic lowering.
    "neovm_jit_make_float",
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
    // The cold side of the entry stack guard of a leaf that can re-enter
    // Lisp (`compile::stack_guard`), baseline and MIR, JIT and AOT.
    "neovm_jit_stack_check",
    "neovm_jit_switch",
    "neovm_jit_switch_stale",
    "neovm_jit_symbolp_slow",
    "neovm_jit_throw",
    "neovm_jit_unbind",
    "neovm_jit_unwind_protect",
    "neovm_jit_varbind",
    "neovm_jit_varref",
    "neovm_jit_varset",
    // Tier profiling is emitted only by JIT leaves. Keep the exported shim
    // list/table aligned; their signatures are salted by ABI_TAG_VERSION.
    "neovm_jit_tier_request",
    "neovm_jit_t2_call_prof",
    "neovm_jit_t2_call_subr_prof",
    "neovm_jit_t2_call_feedback_prof",
    "neovm_jit_t2_call_feedback_census",
    "neovm_jit_t2_call_use_prof",
    "neovm_jit_t2_apply_use_prof",
    "neovm_jit_t2_record_call_use_target",
    // JIT-only emission, named and registered through the lazy shim table.
    "neovm_jit_direct_slow",
    "neovm_jit_call_census",
    "neovm_jit_call_spec_census",
    "neovm_jit_direct_framed",
    // Optional Opt-only exports; excluded from the frozen AOT ABI prefix.
    // Selected JIT-only sqrt witness guard; append to preserve old names.
    // Optional Opt-only exports; excluded from the frozen AOT ABI prefix.
    "neovm_jit_t2_record_array_use",
    "neovm_jit_sqrt_binding_valid",
    // Collection journaling; ABI31 also admits this existing string recorder to AOT.
    "neovm_jit_string_collection_write",
    // Cold GEN0 observation refinement, never an AOT import.
    "neovm_jit_unobserved_collection_owner",
    // Live symbol policy for JIT Throw; preserve the frozen AOT prefix.
    "neovm_jit_throw_with_symbols",
    // ABI31 collection-aware alternatives; legacy import identities stay exact.
    "neovm_jit_aset_collection",
    "neovm_jit_varset_collection",
    "neovm_jit_varbind_collection",
    "neovm_jit_unbind_collection",
    "neovm_jit_setcar_collection",
    "neovm_jit_setcdr_collection",
    // ABI52: the dedicated `fboundp` spec shim. It is declared by every body
    // with a subr-kind spec site (including AOT baseline leaves), so an AOT
    // `.so` may import it: appended AFTER the frozen ABI26 prefix and the
    // ABI31 collection block so no existing index moves.
    "neovm_jit_fboundp_spec",
];

// The AOT selections below are indices into the one name list above, so a
// list edit that moves a name would silently select the wrong shim (the
// emit-time import audit notices only a leaf that imports it). These const
// checks make every selection a compile-time fact: `cargo check` fails
// unless each selected index still names its shim and the frozen ABI26
// prefix still ends at its boundary name. They are const-evaluated and emit
// nothing: the literals, the selected set and the ABI tag are unchanged.
#[allow(dead_code)] // this file is also included by build scripts
const fn aot_shim_name_is(index: usize, name: &str) -> bool {
    if index >= NEOVM_JIT_SHIM_NAMES.len() {
        return false;
    }
    let candidate = NEOVM_JIT_SHIM_NAMES[index].as_bytes();
    let wanted = name.as_bytes();
    if candidate.len() != wanted.len() {
        return false;
    }
    let mut at = 0;
    while at < candidate.len() {
        if candidate[at] != wanted[at] {
            return false;
        }
        at += 1;
    }
    true
}

#[allow(dead_code)] // this file is also included by build scripts
const _: () = {
    // ABI26's frozen prefix ends where the appended names begin.
    assert!(NEOVM_JIT_AOT_ABI_SHIM_COUNT <= NEOVM_JIT_SHIM_NAMES.len());
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_ABI_SHIM_COUNT - 1,
        "neovm_jit_direct_framed"
    ));
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_ABI_SHIM_COUNT,
        "neovm_jit_t2_record_array_use"
    ));
    // Every ABI31 collection import keeps its name.
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES[0],
        "neovm_jit_string_collection_write"
    ));
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES[1],
        "neovm_jit_aset_collection"
    ));
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES[2],
        "neovm_jit_varset_collection"
    ));
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES[3],
        "neovm_jit_varbind_collection"
    ));
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES[4],
        "neovm_jit_unbind_collection"
    ));
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES[5],
        "neovm_jit_setcar_collection"
    ));
    assert!(aot_shim_name_is(
        NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES[6],
        "neovm_jit_setcdr_collection"
    ));
};

// Keep ABI26's original AOT prefix unchanged. ABI31 adds only the collection
// imports selected below; the Opt array/sqrt and observation-refinement names
// stay JIT-only and an AOT import of one still fails closed.
// Keep the selection in the same single source as the exported names.
#[allow(dead_code)] // this file is also included by build scripts
const NEOVM_JIT_AOT_ABI_SHIM_COUNT: usize = 70;

// ABI31 admits the existing string recorder and six appended store exports
// to AOT, while the selected Opt/refinement suffix stays JIT-only. Indices
// reference the one name list above; no second literal name list can drift.
#[allow(dead_code)] // this file is also included by build scripts
const NEOVM_JIT_AOT_COLLECTION_SHIM_INDICES: &[usize] = &[72, 75, 76, 77, 78, 79, 80];

// ABI52 admits one appended name: the dedicated `fboundp` spec shim. Its
// index is DERIVED from the one name list at compile time, so editing the
// list can never leave a stale index: `aot_shim_index` fails `cargo check`
// when the name is missing. The frozen ABI26 prefix 0..=69 and the ABI31
// collection block are untouched by the derivation.
#[allow(dead_code)] // this file is also included by build scripts
const fn aot_shim_index(name: &str) -> usize {
    let wanted = name.as_bytes();
    let mut i = 0;
    while i < NEOVM_JIT_SHIM_NAMES.len() {
        let candidate = NEOVM_JIT_SHIM_NAMES[i].as_bytes();
        if candidate.len() == wanted.len() {
            let mut j = 0;
            while j < wanted.len() && candidate[j] == wanted[j] {
                j += 1;
            }
            if j == wanted.len() {
                return i;
            }
        }
        i += 1;
    }
    panic!("AOT shim name missing from NEOVM_JIT_SHIM_NAMES");
}

#[allow(dead_code)] // this file is also included by build scripts
const NEOVM_JIT_AOT_FBOUNDP_SHIM_INDEX: usize = aot_shim_index("neovm_jit_fboundp_spec");
