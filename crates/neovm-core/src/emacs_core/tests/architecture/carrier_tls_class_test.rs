//! Architecture guard: every production `thread_local!` static has a
//! carrier TLS class (peer review condition 4).
//!
//! The authoritative binary census is the 226 TLS symbols in the shipped
//! profiling binary; this scan covers source statics, including the ones a
//! feature/cfg combination keeps out of that binary. A new `thread_local!`
//! without an entry here fails the test.

use std::path::Path;

#[path = "tls_cfg_policy.rs"]
mod cfg_policy;

#[path = "tls_source_orphan_guard_test.rs"]
mod source_orphan_guard;

use super::architecture_production_source_roots::{
    crates_dir as workspace_crates_dir, production_file,
};

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
enum TlsClass {
    OwnerCarried,
    CarrierIntrinsic,
    MustReset,
    /// Not emitted in the shipped binary census (instrumentation/probe or
    /// cfg/feature-gated); holds no Lisp Values.
    SourceOnly,
}

/// One entry per production `static NAME` inside a `thread_local!` block of
/// the workspace. Classes come from the 226-record classification; extra
/// source-only statics are `SourceOnly`.
/// Sentinel module for nonworkspace shipped-binary records (third-party,
/// linker, or compiler TLS). Workspace statics use their actual source key,
/// including fixture statics whose enclosing module gates are proved below.
const BINARY_CENSUS: &str = "<binary-census-or-test-gated>";

/// One entry per (defining module, `static NAME`) pair inside a
/// production `thread_local!` block of the workspace, plus the
/// shipped-census records the source scan cannot see (BINARY_CENSUS).
///
/// The module is the workspace-relative source path from `crates/`
/// (the same spelling MUST_RESET_MODULE_COVERAGE uses).  Keying by
/// (module, name) keeps a shared static name from letting one module
/// inherit another module's class: a Lisp-holding record can no longer
/// pick up a SourceOnly row with no reset merely because the names
/// match (peer CARRIER-TLS-KEY).  Duplicate (module, name) keys are
/// rejected by `duplicate_classification_keys` in
/// `every_production_thread_local_has_a_carrier_tls_class`.
const TLS_CLASSIFICATION: &[(&str, &str, TlsClass)] = &[
    // Value-free per-thread debug emission observations; instrumentation only.
    // SourceOnly: no Lisp Value; a Cell<usize> emission count compiled only
    // under cfg(any(test, debug_assertions)), not owner or heap state.
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/direct_call.rs",
        "DIRECT_SITES_EMITTED_HERE",
        TlsClass::SourceOnly,
    ),
    // SourceOnly: no Lisp Value; a debug/test-only Cell<usize> emission count
    // under cfg(any(test, debug_assertions)), not owner or heap state.
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "TAIL_CALLS_UNROOTED",
        TlsClass::SourceOnly,
    ),
    // SourceOnly: no Lisp Value; a Cell<usize> engagement count compiled only
    // under cfg(any(test, debug_assertions)), not owner or heap state.
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/stack_guard.rs",
        "STACK_GUARDS_EMITTED",
        TlsClass::SourceOnly,
    ),
    // Ordinary waits retain the native stack on the same Running carrier
    // (eval/green/scheduler.rs wait_for_wake recv; carrier.rs Running slot,
    // carrier_loop accepts the next job only after run_carrier_job returns).
    // switch.rs retires/parks the old job before the NEXT job's deferred reset.
    // Deferred reuse asserts zero before clearing; PARK must preserve depth.
    // MustReset: cache::reset_carrier_tls asserts zero before its zero store
    // at deferred reuse. Static and store share cfg(any(test, debug_assertions));
    // ordinary Green waits retain the live native frame on the same carrier.
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "NATIVE_DEPTH",
        TlsClass::MustReset,
    ),
    // Profiling observations belong to one job; eval deferred reset clears them.
    // MustReset: var_fast::reset_carrier_tls, called by the eval deferred reset,
    // clears every event. Static and reset share cfg(any(test, feature = "vm-profile"));
    // variable_cache_events_survive_park_but_reset_between_jobs verifies the interval.
    (
        "neovm-eval/src/emacs_core/runtime/eval/var_fast.rs",
        "VAR_CACHE_EVENTS",
        TlsClass::MustReset,
    ),
    // Fuzzing-enabled lexical overrides hold no Lisp Values. Each real setter
    // restores the previous value with a private Drop guard, including unwind.
    // CarrierIntrinsic: value-free mode; with_dfa_mode owns a lexical Drop guard.
    // dfa_overrides_restore_nested_and_unwound_state proves normal/unwind restoration.
    (
        "neovm-text/src/emacs_core/text/regex/dfa.rs",
        "DFA_MODE_OVERRIDE",
        TlsClass::CarrierIntrinsic,
    ),
    // CarrierIntrinsic: value-free boolean; with_cold_path restores its saved slot.
    // dfa_overrides_restore_nested_and_unwound_state proves normal/unwind restoration.
    (
        "neovm-text/src/emacs_core/text/regex/dfa.rs",
        "DFA_COLD_OVERRIDE",
        TlsClass::CarrierIntrinsic,
    ),
    // CarrierIntrinsic: value-free boolean; with_first_step restores its saved slot.
    // dfa_overrides_restore_nested_and_unwound_state proves normal/unwind restoration.
    (
        "neovm-text/src/emacs_core/text/regex/dfa.rs",
        "DFA_FIRST_STEP_OVERRIDE",
        TlsClass::CarrierIntrinsic,
    ),
    // CarrierIntrinsic: value-free engine enum; with_regex_engine_override uses TlsScope.
    // differential_overrides_restore_nested_and_unwound_state proves lexical restoration.
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "REGEX_ENGINE_OVERRIDE",
        TlsClass::CarrierIntrinsic,
    ),
    // CarrierIntrinsic: value-free search policy; with_fastmap_disabled uses TlsScope.
    // differential_overrides_restore_nested_and_unwound_state proves lexical restoration.
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "SEARCH_OPTIMIZATION_OVERRIDE",
        TlsClass::CarrierIntrinsic,
    ),
    // CarrierIntrinsic: value-free boolean; with_anchor_alt restores with TlsScope.
    // anchor_override_restores_nested_and_unwound_state proves normal/unwind restoration.
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "ANCHOR_ALT_OVERRIDE",
        TlsClass::CarrierIntrinsic,
    ),
    // Enclosing-module fixture gates are checked by
    // source_only_fixture_gates_still_enclose_their_tls below. These rows do
    // not exempt a production static when a gate is removed or a fixture moves.
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "FORCE_ENABLED",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "INJECTED",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "INJECTED_PRELOAD",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "INJECTED_PREKEYS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "HASH_CALLS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "FORCED_PREWARM_POLICY",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tls_scope.rs",
        "SCALAR",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tls_scope.rs",
        "VALUES",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tls_scope.rs",
        "STACK",
        TlsClass::SourceOnly,
    ),
    // CarrierIntrinsic: `Capture::begin` installs the observation and its Drop
    // guard (and `pause`) restores the previous value on every exit path,
    // including unwind, so the slot is empty between jobs and never carries
    // retired Lisp values past its lexical producer scope.
    (
        "neomacs-layout-engine/src/cursor_source_dependencies.rs",
        "ACTIVE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/front_diag.rs",
        "ACTIVE",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "ACTIVE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/array_snapshot.rs",
        "ACTIVE_ARRAY_KINDS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/tier2.rs",
        "ACTIVE_BUILD",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile.rs",
        "ACTIVE_CALL_HEAVY",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/call_feedback.rs",
        "ACTIVE_CALL_SOURCE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/call_feedback.rs",
        "ACTIVE_CALL_TARGETS_READ",
        TlsClass::MustReset,
    ),
    (
        "neovm-system/src/emacs_core/system/dynamic_module/mod.rs",
        "ACTIVE_ENVS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/call_feedback.rs",
        "ACTIVE_FEEDBACK_HOLDS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/inline.rs",
        "ACTIVE_FUSED",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/perf_map.rs",
        "ACTIVE_LABEL",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/snapshot.rs",
        "ACTIVE_NO_INLINE_CALL_SITES",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/snapshot.rs",
        "ACTIVE_NUMERIC_FEEDBACK",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "ACTIVE_REGALLOC",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "ACTIVE_REGION",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/code_placement.rs",
        "ALLOCATION_HINT",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "ATOM_NAME_CACHE", TlsClass::CarrierIntrinsic),
    (BINARY_CENSUS, "BUF", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/front_diag.rs",
        "BUFFER",
        TlsClass::MustReset,
    ),
    (
        "neovm-system/src/emacs_core/system/dbusbind/connection.rs",
        "BUSES",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile.rs",
        "BYPASS_PROFIT_GATE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/mod.rs",
        "BYTECODE_BRANCH_POLL_COUNT",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/value/mod.rs",
        "BYTECODE_DATA_ACCESS_COUNT",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/string_pos_cache.rs",
        "CACHE",
        TlsClass::OwnerCarried,
    ),
    (BINARY_CENSUS, "CACHED", TlsClass::CarrierIntrinsic),
    (
        "neovm-value/src/string_pos_cache.rs",
        "CACHE_COLLECTION_EPOCH",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/string_pos_cache.rs",
        "CACHE_HEAP",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "CALL_KIND_COUNTS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "CALL_SITES",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-text/src/emacs_core/text/ccl/mod.rs",
        "CCL_REGISTRY",
        TlsClass::OwnerCarried,
    ),
    // CarrierIntrinsic: `with_ccl_obarray` publishes the view pointer in a
    // scoped slot and a `Drop` guard restores the previous (null) value
    // before the frame ends (`ccl/mod.rs:139-167`), including on unwind, so
    // the record is null between jobs and never carries retired state.
    (
        "neovm-text/src/emacs_core/text/ccl/mod.rs",
        "CCL_VARIABLES",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-text/src/emacs_core/text/charset/mod.rs",
        "CHARSET_MAP_MEMO",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/charset/mod.rs",
        "CHARSET_REGISTRY",
        TlsClass::OwnerCarried,
    ),
    // regex/emacs.rs:10773/10774 reset_carrier_tls clears the memo.
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "CHAR_TABLE_ASCII_FOLD",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/data/window_state.rs",
        "CHAR_TABLE_LAYOUT_MUTATION_EPOCH",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neomacs-layout-engine/src/display_status_line.rs",
        "CHROME_GENERATION_RECORD",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "CLOSE_COUNT", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/runtime/eval/data/xdisp_column_spec.rs",
        "COLUMN_SPEC_CONSUMED",
        TlsClass::MustReset,
    ),
    (
        "neovm-edit/src/emacs_core/editing/undo/mod.rs",
        "COMPACTION_IN_PROGRESS",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "COMPILED",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "COMPILED_HEAP",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "COMPILED_OBARRAY",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/composite/mod.rs",
        "COMPOSITION_REGISTRY",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-value/src/tagged/gc/cold_gc.rs",
        "CONCURRENT_HASH_MUTATOR_ID",
        TlsClass::MustReset,
    ),
    (
        "neomacs-display-protocol/src/input_latency.rs",
        "CONSUMED",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "CONTEXT", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/runtime/pdump/runtime.rs",
        "CORE_PDUMP_HOOKS",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/verdict.rs",
        "CURRENT",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/shims.rs",
        "CURRENT_LEAF_BASES",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "CURRENT_PARKER", TlsClass::CarrierIntrinsic),
    (BINARY_CENSUS, "CURRENT_PASS", TlsClass::CarrierIntrinsic),
    // sqlite/mod.rs:71/80 world_state_placeholder + swap_world_state (both maps).
    (
        "neovm-system/src/emacs_core/system/sqlite/mod.rs",
        "DB_HANDLES",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/bg.rs",
        "DEFERRED",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/bg.rs",
        "DEFER_SCOPE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/cold_exits.rs",
        "DEOPT_TAIL",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/tls_scope.rs",
        "DESTROYED_FIRST",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "DROPPED_LEAF_TOTALS",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "DTORS", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/lisp/load/mod.rs",
        "EAGER_EXPANSION_FAILURE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "ENTRY_COUNTS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/inline_vars.rs",
        "ENV",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/string_pos_cache.rs",
        "EPOCH",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/bg.rs",
        "EPOCH_DISCARDS",
        TlsClass::MustReset,
    ),
    (
        "neovm-display/src/emacs_core/display/xfaces/mod.rs",
        "FACE_ATTR_STATE",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc/cons_block_trailer.rs",
        "FAULT",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/file_notify/mod.rs",
        "FILE_NOTIFY_STATE",
        TlsClass::OwnerCarried,
    ),
    (BINARY_CENSUS, "FILTERING", TlsClass::CarrierIntrinsic),
    (
        "neomacs-layout-engine/src/incremental_layout/chrome_memo.rs",
        "FINGERPRINT_RECORD",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "FLONUM_CENSUS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/epoch.rs",
        "FN_CELL_UNCHANGED",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/epoch.rs",
        "FN_EPOCH_BUMPS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/epoch.rs",
        "FN_REDEFINED",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "FOO", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/lisp/print/error_format.rs",
        "FORMAT_OBJECT_STACK",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/tagged/gc/mutator_registration/gate.rs",
        "GATE_THREAD",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/data/dynamic_module_registry.rs",
        "GLOBAL_REFS",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/mod.rs",
        "GLOBAL_SUBR_LOOKUP_COUNT",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/mod.rs",
        "GLOBAL_SUBR_TABLE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "GUARD_COUNT",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/absent_layer.rs",
        "HANDLE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/collections.rs",
        "HASH_TABLE_TEST_ALIASES",
        TlsClass::OwnerCarried,
    ),
    (
        "neomacs-layout-engine/src/display_status_line.rs",
        "HEADER_LINE_EVAL_COUNT",
        TlsClass::MustReset,
    ),
    // AbsentLayer H4 hit vector (eval absent_layer.rs): test-support
    // instrumentation, holds hit labels only, no Lisp Values.
    (
        "neovm-eval/src/emacs_core/runtime/eval/absent_layer.rs",
        "HITS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-system/src/emacs_core/system/perf_trace/mod.rs",
        "HOTPATH_STATE",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "ID", TlsClass::CarrierIntrinsic),
    // lowering.rs:9249/9258 reset_carrier_tls via jit_compile.
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "IMM_POOL",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats.rs",
        "INLINE_CENSUS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "INLINE_DEPS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/epoch.rs",
        "INLINE_EVICTED_LEAVES",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "INLINE_EVICTIONS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/heap_inline.rs",
        "INLINE_HEAP_SITES",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/mod.rs",
        "INLINE_SUBR_TABLE",
        TlsClass::MustReset,
    ),
    // macOS-only menu redirect (display-runtime macos_quit_redirect.rs):
    // absent from the shipped Linux census and holds an ObjC target, not
    // Lisp Values.
    (
        "neomacs-display-runtime/src/render_thread/macos_quit_redirect.rs",
        "INSTALLED",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/arithmetic.rs",
        "INTEGER_WIDTH",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/intern/mod.rs",
        "INTERN_STR_CACHE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-value/src/error/mod.rs",
        "IN_FLIGHT_ROOTS",
        TlsClass::OwnerCarried,
    ),
    // Invariant flag: the real deferred GC boundary rejects a live mutation
    // closure before zeroing it; never silently hide a leaked extent.
    // MustReset: gc_thread::reset_carrier_tls calls mutate::reset_carrier_tls
    // at deferred reuse, asserting GEN-5 before zeroing. Static, reset and call share
    // cfg(any(test, debug_assertions, feature = "test-support")); plain optimized
    // production without test-support contains neither the flag nor this reset.
    (
        "neovm-value/src/tagged/mutate.rs",
        "IN_HEAP_MUT_CLOSURE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/reg_abi.rs",
        "LAMBDA_LIST",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile.rs",
        "LAST_CLIF_SIZE",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "LAST_INSTANT", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "LAST_IR_STATS",
        TlsClass::MustReset,
    ),
    (
        "neovm-edit/src/emacs_core/editing/indent/mod.rs",
        "LAST_KNOWN_COLUMN",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/symbol/leaf_activity.rs",
        "LEAF_ACTIVE",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/subr/leaf.rs",
        "LEAF_BY_SUBR",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/tier2/policy.rs",
        "LEDGER",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/mod.rs",
        "LISP_REGEX_PATTERN_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/mod.rs",
        "LITERAL_TRT_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "LOADED_UNITS",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "LOCAL_WAKER", TlsClass::CarrierIntrinsic),
    (BINARY_CENSUS, "LOCK_HELD", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats.rs",
        "LOOP_MARK",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/symbols.rs",
        "LOSSAGE_SIZE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "MATCHER_OVERFLOW",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "MATCH_SCRATCH",
        TlsClass::MustReset,
    ),
    (
        "neomacs-display-protocol/src/glyph_matrix.rs",
        "MATERIALIZE_CALL_COUNT",
        TlsClass::SourceOnly,
    ),
    (
        "neomacs-layout-engine/src/tty_menu_bar.rs",
        "MENU_BAR_ITEMS_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats.rs",
        "MIR_BAIL_REASONS",
        TlsClass::MustReset,
    ),
    (
        "neomacs-layout-engine/src/display_status_line.rs",
        "MODE_LINE_EVAL_COUNT",
        TlsClass::MustReset,
    ),
    (
        "neovm-edit/src/buffer/text_snapshot.rs",
        "MODE_OVERRIDE",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-system/src/emacs_core/system/dynamic_module/mod.rs",
        "MODULE_CTX",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/intern/mod.rs",
        "NAME_CANONICAL_SYMBOL_CACHE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/stubs.rs",
        "NEOMACS_CLIPBOARD_TEXT",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/stubs.rs",
        "NEOMACS_MONITORS",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/stubs.rs",
        "NEOMACS_PRIMARY_SELECTION_TEXT",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "OBSERVATION_STATE_ACCESSES",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/symbol/property_revision.rs",
        "OBSERVED",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/cconv_memo.rs",
        "OBSERVED_FUNCTION_WRITES",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "OBSERVED_OWNER_PUBLICATIONS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "OP_COUNTS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "OP_PAIR_COUNTS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "OSR_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "OSR_PENDING",
        TlsClass::MustReset,
    ),
    (
        "neovm-edit/src/buffer/composition.rs",
        "OVERRIDE",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tls_scope.rs",
        "OWNED",
        TlsClass::SourceOnly,
    ),
    (BINARY_CENSUS, "PASS_TIME", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/runtime/pdump/convert.rs",
        "PDUMP_LOAD_NAME_REMAP",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/pdump/convert.rs",
        "PDUMP_LOAD_SYM_IDENTITY",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/pdump/convert.rs",
        "PDUMP_LOAD_SYM_REMAP",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/pdump/runtime.rs",
        "PDUMP_RUNTIME_STATE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/asm_dump.rs",
        "PENDING",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/perf_map.rs",
        "PENDING_CALLEE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/bg.rs",
        "PENDING_COUNT",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/shims.rs",
        "PENDING_FLOW",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/bg.rs",
        "PENDING_IDS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/shims.rs",
        "PENDING_ROOT_SWEEP_FLOOR",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/shims.rs",
        "PENDING_SHIM_PANIC",
        TlsClass::MustReset,
    ),
    (
        BINARY_CENSUS,
        "PER_HASHER_NONDETERMINISM",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/phases.rs",
        "PHASE_CLOCK",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "PIKE_FALLBACK",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "PIKE_SCRATCH",
        TlsClass::MustReset,
    ),
    // platform_surface.rs test_override: test-support-only scoped surface
    // override, RAII-restored and holding no Lisp Values.
    (
        "neovm-eval/src/platform_surface.rs",
        "PLATFORM_SURFACE_TEST_OVERRIDE",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "PRELOAD_UNIT",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "PREV_OP",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "PREWARM_HASHES",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "PREWARM_STUBS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        "PREWARM_STUBS_PENDING",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/print/mod.rs",
        "PRINT_BYTES_OBJECT_STACK",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/print/mod.rs",
        "PRINT_CALL_DEPTH",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/print/mod.rs",
        "PRINT_NUMBER_INDEX",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/print/mod.rs",
        "PRINT_OBJECT_STACK",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/data/process_state.rs",
        "PROCESS_NAME_REGISTRY",
        TlsClass::CarrierIntrinsic,
    ),
    (BINARY_CENSUS, "PROFILER", TlsClass::CarrierIntrinsic),
    // macOS-only menu redirect target (display-runtime
    // macos_quit_redirect.rs): absent from the shipped Linux census and holds
    // a retained AppKit object, not Lisp Values.
    (
        "neomacs-display-runtime/src/render_thread/macos_quit_redirect.rs",
        "QUIT_REDIRECT",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/quit.rs",
        "QUIT_REQUESTED_TLS",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "RECENT_CLEARED_CELLS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "RECENT_CLEARED_MASKS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "RECENT_READS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "REDEFINED_PINS",
        TlsClass::MustReset,
    ),
    // CarrierIntrinsic, proven from the initialization side: the frame_layout
    // display hooks are installed only on the main evaluator at startup and
    // redisplay (neomacs/src/main.rs:4215-4240, 4932-4949); a Green worker
    // owner is built from `bootstrap.seed()` (green/bootstrap.rs:141-174)
    // whose `ExecutionSeed` carries no display hook (construct.rs:44-61), and
    // `Context::assemble` leaves redisplay_fn, redisplay_prepare_fn,
    // font_shape_fn, display_idle_maintenance_fn and frame_snapshot_fn None
    // and window_layout_query_adapter Unavailable (construct.rs:2231-2239).
    // Every direct frame_layout entry point is reached only from those
    // main-thread startup/redisplay paths (main.rs:4215, 4219, 4239-4240,
    // 4932-4949, 6155; secondary_tty.rs:70 via the main redisplay closure),
    // so no carrier executes one and REDISPLAY_RUNTIME is never initialized
    // on a carrier: it holds no per-job state and nothing resets it.
    (
        "neomacs/src/frame_layout.rs",
        "REDISPLAY_RUNTIME",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/mod.rs",
        "REGEX_CACHE_COLLECTION_EPOCH",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/mod.rs",
        "REGEX_CACHE_HEAP",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-system/src/emacs_core/system/dbusbind/fd.rs",
        "REGISTERED",
        TlsClass::CarrierIntrinsic,
    ),
    (BINARY_CENSUS, "REGISTRATION", TlsClass::CarrierIntrinsic),
    // The green guard's debug-only emission counter (jit/compile/lowering.rs):
    // a cfg(debug_assertions) `Cell<u64>` that holds no Value.
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "RELEASE_REVALIDATION_EMITTED",
        TlsClass::SourceOnly,
    ),
    // sqlite/mod.rs:80/85 swap_world_state swaps the result-set map.
    (
        "neovm-system/src/emacs_core/system/sqlite/mod.rs",
        "RESULT_SETS",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "RETAG_COUNT",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/symbol/property_revision.rs",
        "REVISION",
        TlsClass::CarrierIntrinsic,
    ),
    (BINARY_CENSUS, "RNG", TlsClass::CarrierIntrinsic),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "ROOTWIN_CARRY",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/tagged/gc/mutator_registration/a8_probes/suppression.rs",
        "SCOPE",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/mod.rs",
        "SCRATCH_GC_ROOTS",
        TlsClass::MustReset,
    ),
    (BINARY_CENSUS, "SCRATCH_SPACE", TlsClass::CarrierIntrinsic),
    (
        "neovm-text/src/emacs_core/text/regex/mod.rs",
        "SEARCH_PATTERN_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/symbol/mod.rs",
        "SEED_SKIP_OBARRAY_SYMBOL_CELLS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/direct_call/self_only.rs",
        "SELF_SOURCE",
        TlsClass::MustReset,
    ),
    (
        "neomacs/src/tty_output_windows.rs",
        "SESSION",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/shared.rs",
        "SHARED_JIT",
        TlsClass::MustReset,
    ),
    (
        "neomacs-terminfo/src/native/padding.rs",
        "SINK",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats/epoch.rs",
        "SPEC_REVALIDATIONS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "STACK_CURSOR_LIVE",
        TlsClass::SourceOnly,
    ),
    (BINARY_CENSUS, "STACK_LIMIT", TlsClass::CarrierIntrinsic),
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "STANDARD_BYTE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-text/src/emacs_core/text/casetab/mod.rs",
        "STANDARD_CASE_TABLE_HEAP",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-text/src/emacs_core/text/casetab/mod.rs",
        "STANDARD_CASE_TABLE_OBJECT",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-text/src/emacs_core/text/category/mod.rs",
        "STANDARD_CATEGORY_TABLE_HEAP",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-text/src/emacs_core/text/category/mod.rs",
        "STANDARD_CATEGORY_TABLE_OBJECT",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "STANDARD_SYNTAX_TABLE_HEAP",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "STANDARD_SYNTAX_TABLE_OBJECT",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "STATE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-value/src/tagged/value.rs",
        "STATIC_SUBR_OBJECTS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/bg.rs",
        "STATS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/shared.rs",
        "STATS",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats.rs",
        "STATS",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/regex/dfa.rs",
        "STATS",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/parse_cache.rs",
        "STATS",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/tagged/value.rs",
        "SUBR_BY_SYM",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "SUBR_COUNTS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tls_scope.rs",
        "SURVIVOR",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/intern/mod.rs",
        "SYMBOL_CACHE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "SYNTAX_CODE_OBJECTS",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "SYNTAX_CODE_OBJECTS_HEAP",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "SYNTAX_FLAT_ASCII_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "SYNTAX_FLAT_ASCII_ENTRY_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/tier2.rs",
        "T2_STATS",
        TlsClass::MustReset,
    ),
    (
        "neomacs-layout-engine/src/display_status_line.rs",
        "TAB_LINE_EVAL_COUNT",
        TlsClass::MustReset,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_BARRIER_WINDOW",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_CONCURRENT_ACTIVE",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_CONCURRENT_HASH_ACTIVE",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_CONS_BARRIER_WINDOW",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_DUMP_SPAN",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_ID",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_PARTITION_ACTIVE",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_SATB_CACHE",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TAGGED_HEAP_WRITE_TRACKING_MODE",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-display/src/emacs_core/display/terminal/pure.rs",
        "TERMINAL_LISP_STATE",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-display/src/emacs_core/display/terminal/pure.rs",
        "TERMINAL_MANAGER",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "TEST_FALLBACK_TAGGED_HEAP",
        TlsClass::SourceOnly,
    ),
    // Test-support heap graveyard (value test_support/authority.rs): the
    // TestHeapScope route retires heaps at thread exit; feature-only, absent
    // from the shipped census, and no per-job reset applies (heaps are not
    // Lisp Values).
    (
        "neovm-value/src/test_support/authority.rs",
        "TEST_HEAP_GRAVEYARD",
        TlsClass::SourceOnly,
    ),
    (BINARY_CENSUS, "TEXT", TlsClass::CarrierIntrinsic),
    (BINARY_CENSUS, "THREAD", TlsClass::CarrierIntrinsic),
    (
        "neovm-value/src/intern/mod.rs",
        "THREAD_CACHE_EPOCH",
        TlsClass::CarrierIntrinsic,
    ),
    (BINARY_CENSUS, "THREAD_DATA", TlsClass::CarrierIntrinsic),
    (BINARY_CENSUS, "THREAD_GUARD", TlsClass::CarrierIntrinsic),
    (BINARY_CENSUS, "THREAD_ID", TlsClass::CarrierIntrinsic),
    (
        "neovm-value/src/value/mod.rs",
        "THREAD_LOCAL_ALLOCATION_COUNTS",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neovm-system/src/emacs_core/system/timefns/mod.rs",
        "TIME_ZONE_RULE",
        TlsClass::CarrierIntrinsic,
    ),
    (
        "neomacs-layout-engine/src/gui_chrome.rs",
        "TOOL_BAR_ITEMS_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/inline_vars.rs",
        "UNBIND_SITES",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/direct_call.rs",
        "UNBOUNDED_BODY",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/lowering.rs",
        "UNTAG_COUNT",
        TlsClass::MustReset,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "VARREF_ALIAS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/bytecode/vm.rs",
        "VARREF_COUNTS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/tagged/gc.rs",
        "VERIFY_MARKED_OBJECTS",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-value/src/intern/mod.rs",
        "VISIBLE_SYMBOL_NAME_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-system/src/emacs_core/system/platform/w32/mod.rs",
        "W32_VALID_CODEPAGES",
        TlsClass::SourceOnly,
    ),
    (
        "neovm-eval/src/emacs_core/lisp/native/builtins/hooks.rs",
        "WINDOW_CONFIGURATION_SNAPSHOTS",
        TlsClass::OwnerCarried,
    ),
    (
        "neovm-eval/src/emacs_core/runtime/eval/data/window_state.rs",
        "WINDOW_PARAMETERS_LAYOUT_GENERATION",
        TlsClass::CarrierIntrinsic,
    ),
    (
        BINARY_CENSUS,
        "WORKER_THREAD_STATE",
        TlsClass::CarrierIntrinsic,
    ),
    // CarrierIntrinsic: this is the linker-synthesized ELF TLS module base
    // (binary census only; no `thread_local!` in the workspace), the address
    // the OS thread's TLS block is reached through. It is valid for the
    // thread's whole lifetime, carries no Lisp state and cannot be reset by
    // source code.
    (
        BINARY_CENSUS,
        "_TLS_MODULE_BASE_",
        TlsClass::CarrierIntrinsic,
    ),
    (
        BINARY_CENSUS,
        "_rjem_je_tsd_tls",
        TlsClass::CarrierIntrinsic,
    ),
    (BINARY_CENSUS, "{closure#0}", TlsClass::CarrierIntrinsic),
    (BINARY_CENSUS, "{closure#1}", TlsClass::CarrierIntrinsic),
];

/// Look up one `(module, name)` record.
///
/// Both halves must match: a static name that several modules use (ACTIVE,
/// HANDLE, STATS) classifies per defining module, so one module's row can
/// never silently answer for another's.
fn classify(module: &str, name: &str) -> Option<TlsClass> {
    TLS_CLASSIFICATION
        .iter()
        .find(|(classified_module, classified_name, _)| {
            *classified_module == module && *classified_name == name
        })
        .map(|(_, _, class)| *class)
}

/// Duplicate `(module, name)` keys in the classification table.
///
/// A duplicate key makes the class ambiguous: `classify` returns the first
/// row, so a later row with a different class is silently ignored. That is
/// exactly the bare-name hazard the module key removes, and it is the only
/// failure mode a same-module duplicate can still produce, so it fails the
/// guard instead of being resolved by row order.
fn duplicate_classification_keys(rows: &[(&str, &str, TlsClass)]) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut duplicates = Vec::new();
    for (module, name, _) in rows {
        let key = format!("{module}::{name}");
        if !seen.insert(key.clone()) {
            duplicates.push(key);
        }
    }
    duplicates
}

/// Same-file multi-occurrence keys and the exact number of `thread_local!`
/// statics each covers.
///
/// The scan's module unit is the source file, so two statics with one name in
/// two function bodies of one file share one (module, name) row and its class.
/// The expected count makes a third occurrence fail the guard until the row
/// (and its class) is re-reviewed; keys with a single occurrence need no row.
const SAME_FILE_MULTI_OCCURRENCE: &[(&str, &str, usize)] = &[(
    "neovm-eval/src/emacs_core/runtime/eval/absent_layer.rs",
    "HANDLE",
    2,
)];

/// Violations of the same-file occurrence contract:
/// - a key scanned more than once must carry an expected-count row with
///   exactly that count, and a listed key must not drift from its count;
/// - an expected-count row must name a key the scan actually finds;
/// - single-occurrence keys need no row and are not violations.
fn same_file_occurrence_violations(
    occurrences: &std::collections::BTreeMap<(String, String), usize>,
    expected: &[(&str, &str, usize)],
) -> Vec<String> {
    let mut violations = Vec::new();
    for ((module, name), count) in occurrences {
        match expected.iter().find(|(expected_module, expected_name, _)| {
            expected_module == module && expected_name == name
        }) {
            Some((_, _, expected_count)) if expected_count == count => {}
            Some((_, _, expected_count)) => violations.push(format!(
                "{module}::{name}: {count} scanned occurrence(s), expected {expected_count}"
            )),
            None if *count > 1 => violations.push(format!(
                "{module}::{name}: {count} scanned occurrences with no expected-count row"
            )),
            None => {}
        }
    }
    for (module, name, expected_count) in expected {
        let key = ((*module).to_owned(), (*name).to_owned());
        if !occurrences.contains_key(&key) {
            violations.push(format!(
                "{module}::{name}: expected-count row names no scanned static \
                 (expected {expected_count})"
            ));
        }
    }
    violations
}

/// Closed interim spellings. Exact source text and an owned parsed cfg
/// attribute are both required; a later cfg evaluator may replace this list.
const ACCEPTED_TEST_ONLY_GATES: &[&str] = &[
    "#[cfg(test)]",
    "#[cfg(any(test, feature = \"test-support\"))]",
];

/// A source-root-to-TLS module route. Every module load is pinned, including
/// inline modules (which repeat their declaring file). `file:line` names the
/// actual gated declaration along this route, not an inferred surrounding cfg.
struct EnclosingTestGate {
    root_file: &'static str,
    file: &'static str,
    line: usize,
    cfg: &'static str,
    module: &'static str,
    resolved_files: &'static [&'static str],
    names: &'static [&'static str],
    reason: &'static str,
}

const ENCLOSING_TEST_GATES: &[EnclosingTestGate] = &[
    EnclosingTestGate {
        root_file: "neovm-eval/src/lib.rs",
        file: "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        line: 5789,
        cfg: "#[cfg(any(test, feature = \"test-support\"))]",
        module: "emacs_core::jit::aot::test_support",
        resolved_files: &[
            "neovm-eval/src/emacs_core/mod.rs",
            "neovm-eval/src/emacs_core/runtime/jit/mod.rs",
            "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
            "neovm-eval/src/emacs_core/runtime/jit/aot.rs",
        ],
        names: &[
            "FORCE_ENABLED",
            "INJECTED",
            "INJECTED_PRELOAD",
            "INJECTED_PREKEYS",
            "HASH_CALLS",
            "FORCED_PREWARM_POLICY",
        ],
        reason: "Per-test AOT knobs (LoadedUnit holds code/descriptor pointers, no Lisp Values), \
                 absent from shipped dependency graphs; a carrier reset must not clear them \
                 mid-test. The manifest provenance guard and normal/build feature graphs \
                 establish that the allowlisted feature is not enabled in production.",
    },
    EnclosingTestGate {
        root_file: "neovm-value/src/lib.rs",
        file: "neovm-value/src/tls_scope.rs",
        line: 163,
        cfg: "#[cfg(test)]",
        module: "tls_scope::tests",
        resolved_files: &[
            "neovm-value/src/tls_scope.rs",
            "neovm-value/src/tls_scope.rs",
        ],
        names: &["SCALAR", "VALUES", "STACK"],
        reason: "cfg(test)-only u32 Cell/Vec fixtures for lexical TLS scope restoration; \
                 no Lisp Values or production carrier state.",
    },
];

fn normalized_path(path: &Path) -> Result<std::path::PathBuf, String> {
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !result.pop() {
                    return Err("path escapes its root".into());
                }
            }
            component => result.push(component.as_os_str()),
        }
    }
    Ok(result)
}

fn gate_route_has_no_symlinks(root: &Path, path: &Path) -> Result<(), String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "module route is outside the source root")?;
    let mut cursor = root.to_path_buf();
    for component in relative.components() {
        cursor.push(component.as_os_str());
        let metadata = std::fs::symlink_metadata(&cursor).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "symlink module routes are unmodelled: {}",
                cursor.display()
            ));
        }
    }
    Ok(())
}

/// Check the raw path exists before normalization: erasing a nonexistent
/// `directory/..` component would otherwise invent a Rust module candidate.
fn pinned_gate_file(
    root: &Path,
    actual: &Path,
    expected: &str,
) -> Result<std::path::PathBuf, String> {
    if !actual.is_file() {
        return Err(format!("missing module file {}", actual.display()));
    }
    gate_route_has_no_symlinks(root, actual)?;
    let actual_normal = normalized_path(actual)?;
    if actual_normal != normalized_path(&root.join(expected))? {
        return Err(format!(
            "module retarget: {} != {expected}",
            actual.display()
        ));
    }
    let canonical = actual.canonicalize().map_err(|error| error.to_string())?;
    if !canonical.starts_with(root) {
        return Err("module resolves outside the source root".into());
    }
    let expected_canonical = root
        .join(expected)
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if canonical != expected_canonical {
        return Err("module traversal resolves to another pinned file".into());
    }
    Ok(actual_normal)
}

/// All library/bin roots of the owner, including automatic src/bin/name.rs
/// and src/bin/name/main.rs targets. Unmodelled target forms fail closed.
fn gate_owner_roots(root: &Path, file: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut directory = file.parent().ok_or("source root has no parent")?;
    let manifest = loop {
        let candidate = directory.join("Cargo.toml");
        if candidate.is_file() {
            break candidate;
        }
        if directory == root {
            return Err("module route has no owning Cargo.toml".into());
        }
        directory = directory
            .parent()
            .filter(|path| path.starts_with(root))
            .ok_or("module route has no owning Cargo.toml")?;
    };
    gate_route_has_no_symlinks(root, &manifest)?;
    let source = std::fs::read_to_string(&manifest).map_err(|error| error.to_string())?;
    let cargo: toml::Value =
        toml::from_str(&source).map_err(|error| format!("root manifest: {error}"))?;
    let package = cargo
        .get("package")
        .and_then(toml::Value::as_table)
        .ok_or("module source does not belong to a Cargo package")?;
    let enabled = |name: &str| -> Result<bool, String> {
        package.get(name).map_or(Ok(true), |value| {
            value
                .as_bool()
                .ok_or_else(|| format!("nonboolean package.{name}"))
        })
    };
    let mut roots = Vec::new();
    match cargo.get("lib") {
        Some(toml::Value::Table(library)) => {
            let path = library.get("path").map_or(Ok("src/lib.rs"), |value| {
                value.as_str().ok_or("nonstring lib.path")
            })?;
            roots.push(directory.join(path));
        }
        Some(_) => return Err("lib must be a table".into()),
        None if enabled("autolib")? && directory.join("src/lib.rs").is_file() => {
            roots.push(directory.join("src/lib.rs"))
        }
        None => {}
    }
    if enabled("autobins")? {
        if directory.join("src/main.rs").is_file() {
            roots.push(directory.join("src/main.rs"));
        }
        let bins = directory.join("src/bin");
        if bins.exists() {
            gate_route_has_no_symlinks(root, &bins)?;
            for entry in std::fs::read_dir(&bins).map_err(|error| error.to_string())? {
                let entry = entry.map_err(|error| error.to_string())?;
                let path = entry.path();
                if entry
                    .file_type()
                    .map_err(|error| error.to_string())?
                    .is_symlink()
                {
                    return Err("symlink automatic Cargo targets are unmodelled".into());
                }
                if path.is_file() && path.extension().is_some_and(|extension| extension == "rs") {
                    roots.push(path);
                } else if path.is_dir() && path.join("main.rs").is_file() {
                    roots.push(path.join("main.rs"));
                }
            }
        }
    }
    if let Some(binaries) = cargo.get("bin") {
        for binary in binaries
            .as_array()
            .ok_or("bin must be an array of tables")?
        {
            let table = binary.as_table().ok_or("bin must be a table")?;
            let path = table
                .get("path")
                .and_then(toml::Value::as_str)
                .ok_or("explicit Cargo bin without a literal path is unmodelled")?;
            roots.push(directory.join(path));
        }
    }
    let mut unique = std::collections::BTreeSet::new();
    for path in roots {
        let normal = normalized_path(&path)?;
        let relative = normal
            .strip_prefix(root)
            .map_err(|_| "Cargo root leaves the source root")?
            .to_str()
            .ok_or("non-UTF8 Cargo root")?;
        unique.insert(pinned_gate_file(root, &path, relative)?);
    }
    Ok(unique.into_iter().collect())
}

/// Arbitrary foo.rs files cannot claim crate-root mounting semantics.
fn verified_gate_root(root: &Path, relative: &str) -> Result<std::path::PathBuf, String> {
    let file = pinned_gate_file(root, &root.join(relative), relative)?;
    if !gate_owner_roots(root, &file)?.contains(&file) {
        return Err(format!(
            "{} is not a declared library/bin root",
            file.display()
        ));
    }
    Ok(file)
}

struct GateModuleScope {
    file: std::path::PathBuf,
    module_dir: std::path::PathBuf,
    inside_inline: bool,
    source: String,
    items: Vec<syn::Item>,
    attributes: Vec<syn::Attribute>,
}

impl GateModuleScope {
    fn read(file: std::path::PathBuf, module_dir: std::path::PathBuf) -> Result<Self, String> {
        let source = std::fs::read_to_string(&file).map_err(|error| error.to_string())?;
        let syntax =
            syn::parse_file(&source).map_err(|error| format!("{}: {error}", file.display()))?;
        Ok(Self {
            file,
            module_dir,
            inside_inline: false,
            source,
            items: syntax.items,
            attributes: syntax.attrs,
        })
    }
}

fn gate_attribute_text(source: &str, attr: &syn::Attribute) -> Option<String> {
    use syn::spanned::Spanned;
    let span = attr.span();
    let offset = |point: proc_macro2::LineColumn| -> Option<usize> {
        let index = point.line.checked_sub(1)?;
        let prefix: usize = source.split_inclusive('\n').take(index).map(str::len).sum();
        let line = source.split_inclusive('\n').nth(index)?;
        // proc_macro2 columns count Unicode characters, not UTF-8 bytes.
        let column = line
            .char_indices()
            .map(|(at, _)| at)
            .chain(std::iter::once(line.len()))
            .nth(point.column)?;
        Some(prefix + column)
    };
    source
        .get(offset(span.start())?..offset(span.end())?)
        .map(str::to_owned)
}

fn gate_module_path(module: &syn::ItemMod) -> Result<Option<std::path::PathBuf>, String> {
    let mut path = None;
    for attr in &module.attrs {
        if attr.path().is_ident("cfg_attr") {
            return Err("cfg_attr module resolution is unmodelled".into());
        }
        if attr.path().is_ident("path") {
            if path.is_some() {
                return Err("multiple module path attributes".into());
            }
            let syn::Meta::NameValue(value) = &attr.meta else {
                return Err("unmodelled module path".into());
            };
            let syn::Expr::Lit(expr) = &value.value else {
                return Err("nonliteral module path".into());
            };
            let syn::Lit::Str(value) = &expr.lit else {
                return Err("nonstring module path".into());
            };
            path = Some(std::path::PathBuf::from(value.value()));
        } else if ![
            "cfg",
            "doc",
            "allow",
            "warn",
            "deny",
            "forbid",
            "expect",
            "deprecated",
        ]
        .iter()
        .any(|name| attr.path().is_ident(name))
        {
            return Err("unknown module attribute can alter its contents".into());
        }
    }
    Ok(path)
}

fn accepted_test_gate_attribute(source: &str, attr: &syn::Attribute) -> bool {
    use syn::parse::Parser;
    if !attr.path().is_ident("cfg") {
        return false;
    }
    let Some(text) = gate_attribute_text(source, attr) else {
        return false;
    };
    if !ACCEPTED_TEST_ONLY_GATES.contains(&text.as_str()) {
        return false;
    }
    let Ok(accepted) = syn::Attribute::parse_outer.parse_str(&text) else {
        return false;
    };
    let Ok(predicate) = attr.parse_args::<syn::Meta>() else {
        return false;
    };
    cfg_policy::interim_cfg_is_test_only(&predicate)
        && matches!(accepted.as_slice(), [accepted] if accepted.meta == attr.meta)
}

/// One model for both the pinned route and the production-alias audit. An
/// explicitly path-loaded file owns its parent directory, while a normal
/// foo.rs mount owns parent/foo. Inline path bases preserve this distinction.
fn descend_gate_module(
    root: &Path,
    scope: &GateModuleScope,
    module: &syn::ItemMod,
    expected: Option<&str>,
) -> Result<GateModuleScope, String> {
    let path = gate_module_path(module)?;
    let base = if scope.inside_inline {
        &scope.module_dir
    } else {
        scope.file.parent().ok_or("declaring file parent")?
    };
    let check = |actual: &Path| -> Result<std::path::PathBuf, String> {
        if let Some(expected) = expected {
            return pinned_gate_file(root, actual, expected);
        }
        let normal = normalized_path(actual)?;
        let relative = normal
            .strip_prefix(root)
            .map_err(|_| "module leaves the source root")?;
        pinned_gate_file(
            root,
            actual,
            relative.to_str().ok_or("non-UTF8 module path")?,
        )
    };
    if let Some((_, items)) = &module.content {
        let file = check(&scope.file)?;
        let module_dir = match path {
            Some(path) => base.join(path),
            None => scope.module_dir.join(module.ident.to_string()),
        };
        if !normalized_path(&module_dir)?.starts_with(root) {
            return Err("inline module directory escapes the source root".into());
        }
        // A purely inline module needs no on-disk child directory. Still
        // reject every existing symlink prefix before a later child is used.
        let mut existing = module_dir.as_path();
        while !existing.exists() {
            if std::fs::symlink_metadata(existing).is_ok_and(|meta| meta.file_type().is_symlink()) {
                return Err("symlink inline module directory is unmodelled".into());
            }
            existing = existing
                .parent()
                .ok_or("inline module directory has no parent")?;
        }
        gate_route_has_no_symlinks(root, existing)?;
        return Ok(GateModuleScope {
            file,
            module_dir,
            inside_inline: true,
            source: scope.source.clone(),
            items: items.clone(),
            attributes: scope.attributes.clone(),
        });
    }
    let (file, module_dir) = if let Some(path) = path {
        let file = base.join(path);
        let parent = file.parent().ok_or("path module parent")?.to_path_buf();
        (file, parent)
    } else {
        let name = module.ident.to_string();
        let plain = scope.module_dir.join(format!("{name}.rs"));
        let directory = scope.module_dir.join(&name).join("mod.rs");
        match (plain.is_file(), directory.is_file()) {
            (true, false) => (plain, scope.module_dir.join(&name)),
            (false, true) => (directory, scope.module_dir.join(&name)),
            _ => return Err(format!("missing/ambiguous default module {name}")),
        }
    };
    GateModuleScope::read(check(&file)?, module_dir)
}

/// External-file claims need a complete production-capable ownership graph.
/// A module with a closed, parsed test gate cannot mount production aliases.
/// Everything else is inspected; opaque expansions and block-local module
/// declarations are refused rather than guessed to be module-free.
fn reject_production_gate_aliases(
    root: &Path,
    root_file: &Path,
    target: &Path,
) -> Result<(), String> {
    use syn::visit::Visit;
    struct HiddenMount(Option<&'static str>);
    impl<'ast> Visit<'ast> for HiddenMount {
        fn visit_item_mod(&mut self, _: &'ast syn::ItemMod) {
            self.0 = Some("block-local module mounts are unmodelled");
        }
        fn visit_macro(&mut self, _: &'ast syn::Macro) {
            self.0 = Some("opaque production macros can mount the claimed file");
        }
        fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
            if ![
                "cfg",
                "doc",
                "allow",
                "warn",
                "deny",
                "forbid",
                "expect",
                "deprecated",
                "inline",
                "cold",
                "must_use",
                "no_mangle",
                "export_name",
            ]
            .iter()
            .any(|name| attr.path().is_ident(name))
            {
                self.0 = Some("opaque production attributes can mount the claimed file");
            }
        }
    }
    let target = target.canonicalize().map_err(|error| error.to_string())?;
    let roots = gate_owner_roots(root, root_file)?;
    let mut pending = roots
        .iter()
        .map(|file| GateModuleScope::read(file.clone(), file.parent().ok_or("root parent")?.into()))
        .collect::<Result<Vec<_>, String>>()?;
    let mut mounted: std::collections::BTreeSet<_> = roots.into_iter().collect();
    while let Some(scope) = pending.pop() {
        if scope
            .file
            .canonicalize()
            .map_err(|error| error.to_string())?
            == target
        {
            return Err(format!(
                "production-capable mount of claimed file {}",
                target.display()
            ));
        }
        let mut inner = HiddenMount(None);
        for attr in &scope.attributes {
            inner.visit_attribute(attr);
        }
        if let Some(problem) = inner.0 {
            return Err(problem.into());
        }
        for item in &scope.items {
            if let syn::Item::Mod(module) = item {
                if module
                    .attrs
                    .iter()
                    .any(|attr| accepted_test_gate_attribute(&scope.source, attr))
                {
                    continue;
                }
                let child = descend_gate_module(root, &scope, module, None)?;
                if module.content.is_none()
                    && !mounted.insert(
                        child
                            .file
                            .canonicalize()
                            .map_err(|error| error.to_string())?,
                    )
                {
                    return Err("duplicate/cyclic production file mounts are unmodelled".into());
                }
                pending.push(child);
                continue;
            }
            let mut hidden = HiddenMount(None);
            if let syn::Item::Macro(item) = item {
                for attr in &item.attrs {
                    hidden.visit_attribute(attr);
                }
                if !item
                    .mac
                    .path
                    .segments
                    .last()
                    .is_some_and(|part| part.ident == "thread_local")
                {
                    return Err("opaque production item macro can mount the claimed file".into());
                }
                let body = syn::parse2::<syn::File>(item.mac.tokens.clone())
                    .map_err(|error| error.to_string())?;
                for item in &body.items {
                    hidden.visit_item(item);
                }
            } else {
                hidden.visit_item(item);
            }
            if let Some(problem) = hidden.0 {
                return Err(problem.into());
            }
        }
    }
    Ok(())
}

/// Check literal path aliases throughout the source tree, including foreign
/// packages. Foreign opaque macro-generated mounts are deliberately outside
/// this bounded audit; owner-package opaque expansions are refused above.
fn reject_literal_gate_aliases(
    root: &Path,
    targets: &std::collections::BTreeSet<std::path::PathBuf>,
    route_paths: &std::collections::BTreeSet<(std::path::PathBuf, usize, usize)>,
) -> Result<(), String> {
    use syn::{spanned::Spanned, visit::Visit};
    fn paths(meta: &syn::Meta, found: &mut Vec<std::path::PathBuf>) -> Result<(), String> {
        match meta {
            syn::Meta::NameValue(value) if value.path.is_ident("path") => {
                let syn::Expr::Lit(expr) = &value.value else {
                    return Err("nonliteral module path".into());
                };
                let syn::Lit::Str(value) = &expr.lit else {
                    return Err("nonstring module path".into());
                };
                found.push(value.value().into());
            }
            syn::Meta::List(list) if list.path.is_ident("cfg_attr") => {
                for meta in list
                    .parse_args_with(
                        syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated,
                    )
                    .map_err(|error| error.to_string())?
                {
                    paths(&meta, found)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    struct LiteralPaths<'a> {
        file: &'a Path,
        dirs: Vec<std::path::PathBuf>,
        inside_inline: bool,
        block_depth: usize,
        targets: &'a std::collections::BTreeSet<std::path::PathBuf>,
        route: &'a std::collections::BTreeSet<(std::path::PathBuf, usize, usize)>,
        error: Option<String>,
    }
    impl<'ast> Visit<'ast> for LiteralPaths<'_> {
        fn visit_block(&mut self, block: &'ast syn::Block) {
            self.block_depth += 1;
            syn::visit::visit_block(self, block);
            self.block_depth -= 1;
        }
        fn visit_item_mod(&mut self, module: &'ast syn::ItemMod) {
            if self.error.is_some() {
                return;
            }
            let Some(parent) = self.file.parent() else {
                self.error = Some("literal-path source has no parent".into());
                return;
            };
            let bases = if self.block_depth != 0 {
                let mut bases = self.dirs.clone();
                bases.push(parent.into());
                bases
            } else if self.inside_inline {
                self.dirs.clone()
            } else {
                vec![parent.into()]
            };
            let mut literal = Vec::new();
            let mut conditional = false;
            for attr in &module.attrs {
                let mut values = Vec::new();
                if let Err(error) = paths(&attr.meta, &mut values) {
                    self.error = Some(error);
                    return;
                }
                if !values.is_empty() && attr.path().is_ident("cfg_attr") {
                    conditional = true;
                }
                for path in values {
                    for base in &bases {
                        let candidate = base.join(&path);
                        if !candidate.is_file()
                            && normalized_path(&candidate)
                                .is_ok_and(|path| self.targets.contains(&path))
                        {
                            self.error = Some(format!(
                                "unresolved literal path can name protected file {}",
                                candidate.display()
                            ));
                            return;
                        }
                        if candidate.is_file() {
                            match candidate.canonicalize() {
                                Ok(target) if self.targets.contains(&target) => {
                                    let span = attr.span().start();
                                    let key = (self.file.to_path_buf(), span.line, span.column);
                                    if !self.route.contains(&key) {
                                        self.error = Some(format!(
                                            "literal path alias of protected file {} at {}:{}",
                                            target.display(),
                                            self.file.display(),
                                            span.line
                                        ));
                                        return;
                                    }
                                }
                                Ok(_) => {}
                                Err(error) => {
                                    self.error = Some(error.to_string());
                                    return;
                                }
                            }
                        }
                    }
                    literal.push(path);
                }
            }
            if module.content.is_none() {
                return;
            }
            let mut dirs = Vec::new();
            if literal.is_empty() || conditional {
                dirs.extend(
                    self.dirs
                        .iter()
                        .map(|dir| dir.join(module.ident.to_string())),
                );
            }
            for path in literal {
                dirs.extend(bases.iter().map(|base| base.join(&path)));
            }
            let previous_dirs = std::mem::replace(&mut self.dirs, dirs);
            let previous_inline = std::mem::replace(&mut self.inside_inline, true);
            syn::visit::visit_item_mod(self, module);
            self.dirs = previous_dirs;
            self.inside_inline = previous_inline;
        }
    }
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| error.to_string())?;
            if kind.is_symlink() {
                return Err("symlink source entries are unmodelled in literal-path audit".into());
            }
            if kind.is_dir() {
                pending.push(path);
                continue;
            }
            if path.file_name().is_some_and(|name| name == "Cargo.toml") {
                let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
                let cargo: toml::Value =
                    toml::from_str(&source).map_err(|error| error.to_string())?;
                if cargo.get("package").is_some() {
                    for source_root in gate_owner_roots(root, &path)? {
                        let canonical = source_root
                            .canonicalize()
                            .map_err(|error| error.to_string())?;
                        if targets.contains(&canonical) {
                            return Err(format!(
                                "Cargo source-root alias of protected file {} in {}",
                                canonical.display(),
                                path.display()
                            ));
                        }
                    }
                }
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let syntax = syn::parse_file(&source)
                .map_err(|error| format!("literal alias scan {}: {error}", path.display()))?;
            let parent = path.parent().ok_or("literal-path source has no parent")?;
            // A foreign non-mod.rs file may have been mounted normally, by
            // #[path], or as a Cargo root. Consider every possible directory.
            let mut dirs = vec![parent.to_path_buf()];
            if let Some(stem) = path.file_stem() {
                dirs.push(parent.join(stem));
            }
            let mut visitor = LiteralPaths {
                file: &path,
                dirs,
                inside_inline: false,
                block_depth: 0,
                targets,
                route: route_paths,
                error: None,
            };
            visitor.visit_file(&syntax);
            if let Some(error) = visitor.error {
                return Err(error);
            }
        }
    }
    Ok(())
}

fn resolve_enclosing_test_gate(
    root: &Path,
    gate: &EnclosingTestGate,
) -> Result<Vec<String>, String> {
    use syn::spanned::Spanned;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let root_file = verified_gate_root(&root, gate.root_file)?;
    let mut scope = GateModuleScope::read(
        root_file.clone(),
        root_file.parent().ok_or("root parent")?.into(),
    )?;
    let parts: Vec<_> = gate.module.split("::").collect();
    if parts.len() != gate.resolved_files.len() || parts.iter().any(|part| part.is_empty()) {
        return Err("every module route step needs one pinned resolved file".into());
    }
    let mut gate_seen = false;
    let mut gate_owns_definition_text = false;
    let mut protected_files = std::collections::BTreeSet::new();
    let mut route_paths = std::collections::BTreeSet::new();
    let mut evidence = Vec::new();
    for (part, expected) in parts.iter().zip(gate.resolved_files) {
        let matches: Vec<_> = scope
            .items
            .iter()
            .filter_map(|item| match item {
                syn::Item::Mod(module) if module.ident == *part => Some(module),
                _ => None,
            })
            .collect();
        let [module] = matches.as_slice() else {
            return Err(format!(
                "missing/ambiguous explicit module {part}; include/macro mounts are unmodelled"
            ));
        };
        let module = (**module).clone();
        if scope.file == normalized_path(&root.join(gate.file))? {
            for attr in &module.attrs {
                if attr.path().is_ident("cfg") && attr.span().start().line == gate.line {
                    if gate_attribute_text(&scope.source, attr).as_deref() != Some(gate.cfg) {
                        return Err("cited gate text changed".into());
                    }
                    if !ACCEPTED_TEST_ONLY_GATES.contains(&gate.cfg) {
                        return Err("cited cfg is not an accepted test-only spelling".into());
                    }
                    if !accepted_test_gate_attribute(&scope.source, attr) {
                        return Err("actual parsed cfg differs from the accepted gate".into());
                    }
                    gate_seen = true;
                    gate_owns_definition_text = true;
                }
            }
        }
        let external_after_gate = gate_seen && module.content.is_none();
        if gate_seen {
            for attr in &module.attrs {
                if attr.path().is_ident("path") {
                    let span = attr.span().start();
                    route_paths.insert((scope.file.clone(), span.line, span.column));
                }
            }
        }
        if external_after_gate {
            gate_owns_definition_text = false;
        }
        scope = descend_gate_module(&root, &scope, &module, Some(expected))?;
        if external_after_gate {
            protected_files.insert(
                scope
                    .file
                    .canonicalize()
                    .map_err(|error| error.to_string())?,
            );
        }
        evidence.push(format!(
            "{part} -> {}",
            scope
                .file
                .strip_prefix(&root)
                .map_err(|error| error.to_string())?
                .display()
        ));
    }
    if !gate_seen {
        return Err("cited cfg no longer gates this module route".into());
    }
    if !gate_owns_definition_text {
        for target in &protected_files {
            reject_production_gate_aliases(&root, &root_file, target)?;
        }
        reject_literal_gate_aliases(&root, &protected_files, &route_paths)?;
        evidence.push("ownership: all owner lib/bin roots plus workspace literal path/Cargo-root aliases; foreign opaque macro mounts are outside this bounded audit".into());
    } else {
        evidence.push(
            "ownership: same-file inline cfg remains attached under every physical-file mount"
                .into(),
        );
    }
    let mut names = std::collections::BTreeSet::new();
    for item in &scope.items {
        let syn::Item::Macro(item) = item else {
            continue;
        };
        if !item
            .mac
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "thread_local")
        {
            return Err("include/macro-generated TLS contents are unmodelled".into());
        }
        let body = syn::parse2::<syn::File>(item.mac.tokens.clone())
            .map_err(|error| format!("unmodelled thread_local body: {error}"))?;
        for item in body.items {
            let syn::Item::Static(item) = item else {
                return Err("thread_local body must declare statics".into());
            };
            names.insert(item.ident.to_string());
        }
    }
    if !gate.names.iter().all(|name| names.contains(*name)) {
        return Err("cited statics no longer belong to the resolved gated module".into());
    }
    Ok(evidence)
}

#[test]
fn source_only_fixture_gates_still_enclose_their_tls() {
    for gate in ENCLOSING_TEST_GATES {
        let evidence =
            resolve_enclosing_test_gate(&workspace_crates_dir(), gate).unwrap_or_else(|error| {
                panic!(
                    "SourceOnly gate {}:{} {} ({}): {error}",
                    gate.file, gate.line, gate.cfg, gate.reason
                )
            });
        eprintln!("verified {}: {}", gate.module, evidence.join("; "));
        let file = gate.resolved_files.last().expect("gate has a module route");
        for name in gate.names {
            assert!(matches!(classify(file, name), Some(TlsClass::SourceOnly)));
        }
    }
}

fn gate_fixture(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = workspace_crates_dir()
        .parent()
        .expect("workspace root")
        .join("tmp");
    std::fs::create_dir_all(&tmp).expect("workspace fixture directory");
    let fixture = tempfile::tempdir_in(tmp).expect("module fixture tree");
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname='gate-fixture'\nversion='0.0.0'\n[lib]\npath='lib.rs'\n",
    )
    .expect("fixture manifest");
    for (path, source) in files {
        let path = fixture.path().join(path);
        std::fs::create_dir_all(path.parent().expect("fixture parent"))
            .expect("fixture directories");
        std::fs::write(path, source).expect("fixture source");
    }
    fixture
}

fn fixture_gate(
    module: &'static str,
    resolved_files: &'static [&'static str],
) -> EnclosingTestGate {
    EnclosingTestGate {
        root_file: "lib.rs",
        file: "lib.rs",
        line: 1,
        cfg: "#[cfg(test)]",
        module,
        resolved_files,
        names: &["SCALAR"],
        reason: "test fixture",
    }
}

const FIXTURE_TLS: &str = "std::thread_local! { static SCALAR: u32 = 0; }\n";

#[test]
fn planted_removed_fixture_gate_turns_the_surface_red() {
    let gate = fixture_gate("tests", &["lib.rs"]);
    let fixture = gate_fixture(&[(
        "lib.rs",
        "#[cfg(test)]\nmod tests { std::thread_local! { static SCALAR: u32 = 0; } }\n",
    )]);
    assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_ok());
    for source in [
        "// gate removed\nmod tests { std::thread_local! { static SCALAR: u32 = 0; } }\n",
        "#[cfg(test)]\nmod tests {}\nstd::thread_local! { static SCALAR: u32 = 0; }\n",
        "#[cfg(test)]\nmod tests { const TEXT: &str = \"static SCALAR: u32 = 0;\"; }\n",
    ] {
        std::fs::write(fixture.path().join("lib.rs"), source).expect("mutated fixture");
        assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_err());
    }
}

#[test]
fn fixture_gate_resolves_inline_default_path_and_nested_modules() {
    let cases: &[(&str, &[&str], &[(&str, &str)])] = &[
        (
            "tests",
            &["lib.rs"],
            &[(
                "lib.rs",
                "#[cfg(test)]\nmod tests { std::thread_local! { static SCALAR: u32 = 0; } }\n",
            )],
        ),
        (
            "tests",
            &["tests.rs"],
            &[
                ("lib.rs", "#[cfg(test)]\nmod tests;\n"),
                ("tests.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests",
            &["tests/mod.rs"],
            &[
                ("lib.rs", "#[cfg(test)]\nmod tests;\n"),
                ("tests/mod.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests",
            &["other/custom.rs"],
            &[
                (
                    "lib.rs",
                    "#[cfg(test)]\n#[path=\"other/custom.rs\"] mod tests;\n",
                ),
                ("other/custom.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests::child",
            &["tests.rs", "tests/child.rs"],
            &[
                ("lib.rs", "#[cfg(test)]\nmod tests;\n"),
                ("tests.rs", "mod child;\n"),
                ("tests/child.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests::child",
            &["tests/mod.rs", "tests/child/mod.rs"],
            &[
                ("lib.rs", "#[cfg(test)]\nmod tests;\n"),
                ("tests/mod.rs", "mod child;\n"),
                ("tests/child/mod.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests::child",
            &["other/custom.rs", "other/child.rs"],
            &[
                (
                    "lib.rs",
                    "#[cfg(test)]\n#[path=\"other/custom.rs\"] mod tests;\n",
                ),
                ("other/custom.rs", "mod child;\n"),
                ("other/child.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests::inline::child",
            &["tests.rs", "tests.rs", "tests/inline/payload.rs"],
            &[
                ("lib.rs", "#[cfg(test)]\nmod tests;\n"),
                (
                    "tests.rs",
                    "mod inline { #[path=\"payload.rs\"] mod child; }\n",
                ),
                ("tests/inline/payload.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests::child",
            &["lib.rs", "redirect/payload.rs"],
            &[
                (
                    "lib.rs",
                    "#[cfg(test)]\n#[path=\"redirect\"] mod tests { #[path=\"payload.rs\"] mod child; }\n",
                ),
                ("redirect/payload.rs", FIXTURE_TLS),
            ],
        ),
    ];
    for &(module, files, sources) in cases {
        let fixture = gate_fixture(sources);
        let evidence = resolve_enclosing_test_gate(fixture.path(), &fixture_gate(module, files))
            .unwrap_or_else(|error| panic!("{module} {files:?}: {error}"));
        eprintln!("positive {module}: {}", evidence.join("; "));
    }
}

#[test]
fn fixture_gate_checks_the_actual_external_declaring_context() {
    let fixture = gate_fixture(&[
        ("lib.rs", "mod owner;\n"),
        ("owner.rs", "#[cfg(test)]\nmod tests;\n"),
        ("owner/tests.rs", FIXTURE_TLS),
    ]);
    let gate = EnclosingTestGate {
        file: "owner.rs",
        ..fixture_gate("owner::tests", &["owner.rs", "owner/tests.rs"])
    };
    assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_ok());
    let fixture = gate_fixture(&[
        ("lib.rs", "#[path=\"custom.rs\"] mod owner;\n"),
        ("custom.rs", "#[cfg(test)]\nmod tests;\n"),
        ("tests.rs", FIXTURE_TLS),
    ]);
    let gate = EnclosingTestGate {
        file: "custom.rs",
        ..fixture_gate("owner::tests", &["custom.rs", "tests.rs"])
    };
    assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_ok());
}

#[test]
fn fixture_gate_resolves_path_relative_to_a_non_mod_declaring_file() {
    let fixture = gate_fixture(&[
        ("lib.rs", "#[cfg(test)]\nmod tests;\n"),
        ("tests.rs", "#[path=\"payload.rs\"] mod child;\n"),
        ("payload.rs", FIXTURE_TLS),
    ]);
    let gate = fixture_gate("tests::child", &["tests.rs", "payload.rs"]);
    assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_ok());
}

#[cfg(unix)]
#[test]
fn fixture_gate_rejects_symlink_parent_traversal_to_another_file() {
    let fixture = gate_fixture(&[
        (
            "lib.rs",
            "#[cfg(test)]\n#[path=\"alias/../chosen.rs\"] mod tests;\n",
        ),
        ("chosen.rs", FIXTURE_TLS),
        ("other/chosen.rs", "// Rust loads this other file.\n"),
    ]);
    std::fs::create_dir_all(fixture.path().join("other/deep")).expect("symlink target");
    std::os::unix::fs::symlink("other/deep", fixture.path().join("alias")).expect("module symlink");
    assert!(
        resolve_enclosing_test_gate(fixture.path(), &fixture_gate("tests", &["chosen.rs"]))
            .is_err()
    );
}

#[test]
fn fixture_gate_rejects_unmodelled_or_retargeted_module_routes() {
    let cases: &[(&str, &[&str], &[(&str, &str)])] = &[
        (
            "tests",
            &["tests.rs"],
            &[("lib.rs", "#[cfg(test)]\nmod tests;\n")],
        ),
        (
            "tests",
            &["tests.rs"],
            &[
                ("lib.rs", "#[cfg(test)]\nmod tests;\n"),
                ("tests.rs", FIXTURE_TLS),
                ("tests/mod.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests",
            &["original.rs"],
            &[
                (
                    "lib.rs",
                    "#[cfg(test)]\n#[path=\"changed.rs\"] mod tests;\n",
                ),
                ("original.rs", FIXTURE_TLS),
                ("changed.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests",
            &["tests.rs"],
            &[
                (
                    "lib.rs",
                    "#[cfg(test)]\n#[cfg_attr(probe,path=\"other.rs\")] mod tests;\n",
                ),
                ("tests.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests",
            &["mounted.rs"],
            &[
                ("lib.rs", "include!(\"declarations.rs\");\n"),
                (
                    "declarations.rs",
                    "#[cfg(test)]\n#[path=\"mounted.rs\"] mod tests;\n",
                ),
                ("mounted.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests",
            &["tests.rs"],
            &[
                (
                    "lib.rs",
                    "macro_rules! declare { () => { #[cfg(test)] mod tests; } }\ndeclare!();\n",
                ),
                ("tests.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests",
            &["lib.rs"],
            &[
                (
                    "lib.rs",
                    "#[cfg(test)]\nmod tests { include!(\"payload.rs\"); }\n",
                ),
                ("payload.rs", FIXTURE_TLS),
            ],
        ),
        (
            "tests::child",
            &["original.rs", "shared.rs"],
            &[
                (
                    "lib.rs",
                    "#[cfg(test)]\n#[path=\"changed.rs\"] mod tests;\n",
                ),
                ("original.rs", "#[path=\"shared.rs\"] mod child;\n"),
                ("changed.rs", "#[path=\"shared.rs\"] mod child;\n"),
                ("shared.rs", FIXTURE_TLS),
            ],
        ),
    ];
    for &(module, files, sources) in cases {
        let fixture = gate_fixture(sources);
        assert!(
            resolve_enclosing_test_gate(fixture.path(), &fixture_gate(module, files)).is_err(),
            "{module} {files:?}"
        );
    }
}

#[test]
fn external_fixture_gate_rejects_other_mounts_and_opaque_owner_code() {
    for live in [
        "#[path=\"shared.rs\"] mod live;\n",
        "fn f() { #[path=\"shared.rs\"] mod live; }\n",
        "opaque_module!();\n",
        "fn f() { opaque_module!(); }\n",
        "#[opaque_attribute] fn f() {}\n",
    ] {
        let source = format!("#[cfg(test)]\n#[path=\"shared.rs\"] mod tests;\n{live}");
        let fixture = gate_fixture(&[("lib.rs", &source), ("shared.rs", FIXTURE_TLS)]);
        assert!(
            resolve_enclosing_test_gate(fixture.path(), &fixture_gate("tests", &["shared.rs"]))
                .is_err(),
            "{live}"
        );
    }
}

#[test]
fn external_fixture_gate_checks_second_and_automatic_bin_roots() {
    for bin in ["runner.rs", "src/bin/runner.rs", "src/bin/runner/main.rs"] {
        let prefix = if bin == "runner.rs" {
            ""
        } else if bin.ends_with("/main.rs") {
            "../../../"
        } else {
            "../../"
        };
        let live = format!("#[path=\"{prefix}shared.rs\"] mod live;\nfn main() {{}}\n");
        let fixture = gate_fixture(&[
            ("lib.rs", "#[cfg(test)]\n#[path=\"shared.rs\"] mod tests;\n"),
            ("shared.rs", FIXTURE_TLS),
            (bin, &live),
        ]);
        if bin == "runner.rs" {
            std::fs::write(fixture.path().join("Cargo.toml"), "[package]\nname='gate-fixture'\nversion='0.0.0'\n[lib]\npath='lib.rs'\n[[bin]]\nname='runner'\npath='runner.rs'\n").expect("second bin manifest");
        }
        assert!(
            resolve_enclosing_test_gate(fixture.path(), &fixture_gate("tests", &["shared.rs"]))
                .is_err(),
            "{bin}"
        );
    }
}

#[test]
fn external_fixture_gate_rejects_cross_package_literal_paths() {
    for source in [
        "#[path=\"../../shared.rs\"] mod live;\n",
        "#[path=\"missing/../../../shared.rs\"] mod live;\n",
        "mod nested { #[path=\"../../../shared.rs\"] mod live; }\n",
    ] {
        let fixture = gate_fixture(&[
            ("lib.rs", "#[cfg(test)]\n#[path=\"shared.rs\"] mod tests;\n"),
            ("shared.rs", FIXTURE_TLS),
            (
                "foreign/Cargo.toml",
                "[package]\nname='foreign'\nversion='0.0.0'\n",
            ),
            ("foreign/src/lib.rs", source),
            ("foreign/src/nested/unused.rs", ""),
        ]);
        assert!(
            resolve_enclosing_test_gate(fixture.path(), &fixture_gate("tests", &["shared.rs"]))
                .is_err(),
            "{source}"
        );
    }
    // A foreign alias of an intermediate file also bypasses the owner gate.
    let fixture = gate_fixture(&[
        ("lib.rs", "#[cfg(test)]\n#[path=\"entry.rs\"] mod tests;\n"),
        ("entry.rs", "mod payload;\n"),
        ("payload.rs", FIXTURE_TLS),
        (
            "foreign/src/lib.rs",
            "#[path=\"../../entry.rs\"] mod live;\n",
        ),
    ]);
    assert!(
        resolve_enclosing_test_gate(
            fixture.path(),
            &fixture_gate("tests::payload", &["entry.rs", "payload.rs"])
        )
        .is_err()
    );
}

#[test]
fn external_fixture_gate_rejects_foreign_cargo_source_root_aliases() {
    for target in [
        "[lib]\npath='../shared.rs'\n",
        "[[bin]]\nname='foreign'\npath='../shared.rs'\n",
    ] {
        let manifest = format!("[package]\nname='foreign'\nversion='0.0.0'\n{target}");
        let fixture = gate_fixture(&[
            ("lib.rs", "#[cfg(test)]\n#[path=\"shared.rs\"] mod tests;\n"),
            ("shared.rs", FIXTURE_TLS),
            ("foreign/Cargo.toml", &manifest),
        ]);
        assert!(
            resolve_enclosing_test_gate(fixture.path(), &fixture_gate("tests", &["shared.rs"]))
                .is_err(),
            "{target}"
        );
    }
}

#[test]
fn fixture_gate_uses_character_spans_and_the_actual_cfg_ast() {
    let spoof = format!(
        "/*{} #[cfg(test)] */ #[cfg(unix)] mod tests {{ {FIXTURE_TLS} }}\n",
        "é".repeat(16)
    );
    let fixture = gate_fixture(&[("lib.rs", &spoof)]);
    let gate = fixture_gate("tests", &["lib.rs"]);
    assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_err());
    std::fs::write(
        fixture.path().join("lib.rs"),
        format!("/* é🦀 */ #[cfg(test)] mod tests {{ {FIXTURE_TLS} }}\n"),
    )
    .expect("Unicode gate fixture");
    assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_ok());
}

#[cfg(unix)]
#[test]
fn fixture_gate_rejects_a_manifest_root_that_traverses_a_symlink() {
    let fixture = gate_fixture(&[
        (
            "lib.rs",
            "#[cfg(test)]\nmod tests { std::thread_local! { static SCALAR: u32 = 0; } }\n",
        ),
        (
            "other/lib.rs",
            "// Cargo loads this different crate root.\n",
        ),
    ]);
    std::fs::create_dir_all(fixture.path().join("other/deep")).expect("symlink target");
    std::os::unix::fs::symlink("other/deep", fixture.path().join("alias")).expect("root symlink");
    std::fs::write(
        fixture.path().join("Cargo.toml"),
        "[package]\nname='gate-fixture'\nversion='0.0.0'\n[lib]\npath='alias/../lib.rs'\n",
    )
    .expect("mutated manifest root");
    assert!(
        resolve_enclosing_test_gate(fixture.path(), &fixture_gate("tests", &["lib.rs"])).is_err()
    );
}

#[test]
fn fixture_gate_rejects_duplicate_named_declarations() {
    let fixture = gate_fixture(&[(
        "lib.rs",
        "#[cfg(test)]\nmod tests { std::thread_local! { static SCALAR: u32 = 0; } }\n#[cfg(not(test))]\nmod tests {}\n",
    )]);
    assert!(
        resolve_enclosing_test_gate(fixture.path(), &fixture_gate("tests", &["lib.rs"])).is_err()
    );
}

#[test]
fn fixture_gate_requires_a_proven_test_cfg_and_real_crate_root() {
    for cfg in [
        "#[cfg(any(test,feature=\"jit\"))]",
        "#[cfg(not(test))]",
        "#[cfg(neomacs_have_dbus)]",
        "#[cfg(all(test,neomacs_have_dbus))]",
    ] {
        let fixture =
            gate_fixture(&[("lib.rs", &format!("{cfg}\nmod tests {{ {FIXTURE_TLS} }}\n"))]);
        let gate = EnclosingTestGate {
            cfg,
            ..fixture_gate("tests", &["lib.rs"])
        };
        assert!(
            resolve_enclosing_test_gate(fixture.path(), &gate).is_err(),
            "{cfg}"
        );
    }
    for cfg in [
        "#[cfg(test)]",
        "#[cfg(any(test, feature = \"test-support\"))]",
    ] {
        let fixture =
            gate_fixture(&[("lib.rs", &format!("{cfg}\nmod tests {{ {FIXTURE_TLS} }}\n"))]);
        let gate = EnclosingTestGate {
            cfg,
            ..fixture_gate("tests", &["lib.rs"])
        };
        assert!(
            resolve_enclosing_test_gate(fixture.path(), &gate).is_ok(),
            "{cfg}"
        );
    }
    let fixture = gate_fixture(&[
        ("lib.rs", ""),
        (
            "unused.rs",
            "#[cfg(test)]\nmod tests { std::thread_local! { static SCALAR: u32 = 0; } }\n",
        ),
    ]);
    let gate = EnclosingTestGate {
        root_file: "unused.rs",
        file: "unused.rs",
        resolved_files: &["unused.rs"],
        ..fixture_gate("tests", &["lib.rs"])
    };
    assert!(resolve_enclosing_test_gate(fixture.path(), &gate).is_err());
}

/// Only attributes attached to this item can make it test-only. A nearby
/// method's cfg must not hide production carrier state after the method ends.
fn is_test_only_block(mut before: &str) -> bool {
    use syn::parse::Parser;
    let mut test_only = false;
    loop {
        before = before.trim_end();
        let line_start = before.rfind('\n').map_or(0, |at| at + 1);
        let line = before[line_start..].trim_start();
        if line.starts_with("//") {
            before = &before[..line_start];
            continue;
        }
        if before.ends_with("*/") {
            let Some(open) = before.rfind("/*") else {
                return false;
            };
            before = &before[..open];
            continue;
        }
        if !before.ends_with(']') {
            return test_only;
        }

        // Walk an entire attribute, including multiline cfgs and nested [].
        let mut depth = 0;
        let open = before.char_indices().rev().find_map(|(at, ch)| {
            match ch {
                ']' => depth += 1,
                '[' => depth -= 1,
                _ => {}
            }
            (ch == '[' && depth == 0).then_some(at)
        });
        let Some(open) = open.and_then(|at| at.checked_sub(1)) else {
            return false;
        };
        if before.as_bytes()[open] != b'#' {
            return false;
        }
        let Ok(attributes) = syn::Attribute::parse_outer.parse_str(&before[open..]) else {
            return false;
        };
        let [attribute] = attributes.as_slice() else {
            return false;
        };
        // Conditional attributes need the complete evaluator in the next
        // member. Keep them visible even beside another recognized cfg.
        if attribute.path().is_ident("cfg_attr") {
            return false;
        }
        if attribute.path().is_ident("cfg") {
            let Ok(predicate) = attribute.parse_args::<syn::Meta>() else {
                return false;
            };
            test_only |= cfg_policy::interim_cfg_is_test_only(&predicate);
        }
        before = &before[..open];
    }
}

#[test]
fn tls_scan_keeps_production_blocks_after_test_only_items() {
    let before = r#"
impl Census {
    #[cfg(any(test, feature = "test-support"))]
    fn count(&self) -> usize { self.count }
}

// Production carrier state.
"#;
    for before in [
        before,
        "// The preceding item used #[cfg(test)].\n",
        "#[allow(dead_code)]\n// Not #[cfg(test)].\n",
    ] {
        assert!(
            !is_test_only_block(before),
            "a preceding item's test cfg must not hide a production TLS block: {before}"
        );
    }
}

#[test]
fn tls_scan_skips_blocks_with_attached_test_attributes() {
    for before in [
        "#[cfg(test)]\n",
        "#[cfg(test)]\n\n// Test fixture.\n",
        "#[cfg(all(test, feature = \"probe\"))]\n",
        "#[cfg(any(test, feature = \"test-support\"))]\n",
        "#[cfg(\n    test\n)]\n#[allow(dead_code)]\n",
        "#[cfg(test)]\n/* Test fixture. */\n",
        "#[cfg(test)]\n/* Multiline\n   test fixture. */\n",
    ] {
        assert!(is_test_only_block(before), "attached test cfg: {before}");
    }
}

#[test]
fn planted_production_alternatives_are_scanned_as_tls() {
    let source = r#"
#[cfg(any(test, debug_assertions))]
thread_local! { static DEBUG_COUNTER: u32 = 0; }
#[cfg(any(test, feature = "vm-profile"))]
std::thread_local! { static PROFILE_COUNTER: u32 = 0; }
#[cfg(any(test, feature = "fuzzing", feature = "test-support"))]
thread_local! { static FUZZ_OVERRIDE: u32 = 0; }
#[cfg_attr(test, cfg(test))]
thread_local! { static UNKNOWN_CFG_ATTR: u32 = 0; }
#[cfg(test)]
#[cfg_attr(test, cfg(test))]
thread_local! { static INNER_UNKNOWN_ATTRIBUTE: u32 = 0; }
#[cfg_attr(test, cfg(test))]
#[cfg(test)]
thread_local! { static OUTER_UNKNOWN_ATTRIBUTE: u32 = 0; }
"#;
    let mut found = Vec::new();
    scan_tls_source(source, "fixture.rs", &mut found);
    assert_eq!(
        found.into_iter().map(|(name, _)| name).collect::<Vec<_>>(),
        [
            "DEBUG_COUNTER",
            "PROFILE_COUNTER",
            "FUZZ_OVERRIDE",
            "UNKNOWN_CFG_ATTR",
            "INNER_UNKNOWN_ATTRIBUTE",
            "OUTER_UNKNOWN_ATTRIBUTE"
        ],
        "a production-capable or unmodelled alternative must not disappear from the census"
    );
}

#[test]
fn planted_required_test_and_allowlisted_alternatives_are_skipped() {
    let source = r#"
#[cfg(test)]
thread_local! { static UNIT_ONLY: u32 = 0; }
#[cfg(all(test, feature = "x"))]
thread_local! { static REQUIRED_TEST: u32 = 0; }
#[cfg(any(test, feature = "test-support"))]
thread_local! { static SUPPORT_ONLY: u32 = 0; }
#[cfg(any(test, feature = "redisplay-test-policy"))]
std::thread_local! { static REDISPLAY_ONLY: u32 = 0; }
"#;
    let mut found = Vec::new();
    scan_tls_source(source, "fixture.rs", &mut found);
    assert!(
        found.is_empty(),
        "only proven test-only alternatives are excluded: {found:?}"
    );
}

fn scanned_tls_names() -> Vec<(String, String)> {
    let crates = workspace_crates_dir();
    let mut found = Vec::new();
    visit(&crates, &crates, &mut found);
    found.sort();
    found
}

fn visit(root: &Path, dir: &Path, found: &mut Vec<(String, String)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit(root, &path, found);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if rel.contains("/tests/") {
            continue;
        }
        scan_tls_source(&text, &rel, found);
    }
}

fn scan_tls_source(text: &str, rel: &str, found: &mut Vec<(String, String)>) {
    let mut search = 0;
    while let Some(pos) = text[search..].find("thread_local!") {
        let start = search + pos;
        let Some(open) = text[start..].find('{') else {
            break;
        };
        let open = start + open;
        let mut depth = 0usize;
        let mut end = open;
        for (offset, ch) in text[open..].char_indices() {
            match ch {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        end = open + offset;
                        break;
                    }
                }
                _ => {}
            }
        }
        // Attributes precede the item's line, including qualified
        // std::thread_local! invocations.
        let line_start = text[..start].rfind('\n').map_or(0, |at| at + 1);
        let before = &text[..line_start];
        if !is_test_only_block(before) {
            let block = &text[open..=end];
            let mut cursor = 0;
            while let Some(sp) = block[cursor..].find("static ") {
                let at = cursor + sp + "static ".len();
                let rest = &block[at..];
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                    .collect();
                let name_len = name.len();
                if name_len != 0 && rest[name_len..].trim_start().starts_with(':') {
                    found.push((name, rel.to_owned()));
                }
                cursor = at + name_len;
            }
        }
        search = end.max(start + 1);
    }
}

#[test]
fn every_production_thread_local_has_a_carrier_tls_class() {
    let scanned = scanned_tls_names();
    assert!(
        !scanned.is_empty(),
        "the scan must find the workspace TLS statics"
    );
    let duplicates = duplicate_classification_keys(TLS_CLASSIFICATION);
    assert!(
        duplicates.is_empty(),
        "duplicate (module, name) carrier TLS keys (the class would depend on row order): \
         {duplicates:?}"
    );
    let missing: Vec<_> = scanned
        .iter()
        .filter(|(name, file)| classify(file, name).is_none())
        .map(|(name, file)| format!("{name} ({file})"))
        .collect();
    assert!(
        missing.is_empty(),
        "unclassified thread_local! statics (add a carrier TLS class): {missing:?}"
    );
    let mut occurrences: std::collections::BTreeMap<(String, String), usize> =
        std::collections::BTreeMap::new();
    for (name, file) in &scanned {
        *occurrences.entry((file.clone(), name.clone())).or_insert(0) += 1;
    }
    let occurrence_violations =
        same_file_occurrence_violations(&occurrences, SAME_FILE_MULTI_OCCURRENCE);
    assert!(
        occurrence_violations.is_empty(),
        "same-file carrier TLS occurrence counts drifted: {occurrence_violations:?}"
    );
}

/// Mutation plant: a table with one duplicated `(module, name)` key and one
/// legitimate same-name/different-module pair. The duplicate must be
/// reported, the cross-module pair must not: a name-keyed table would treat
/// both as the same record and let the second class silently lose.
#[test]
fn planted_duplicate_carrier_tls_key_turns_the_surface_red() {
    let plant = [
        (
            "neovm-value/src/tagged/gc.rs",
            "TAGGED_HEAP",
            TlsClass::OwnerCarried,
        ),
        (
            "neovm-value/src/tagged/gc.rs",
            "TAGGED_HEAP",
            TlsClass::SourceOnly,
        ),
        (
            "neovm-value/src/tagged/gc/concurrent.rs",
            "TAGGED_HEAP",
            TlsClass::SourceOnly,
        ),
    ];
    assert_eq!(
        duplicate_classification_keys(&plant),
        vec!["neovm-value/src/tagged/gc.rs::TAGGED_HEAP".to_owned()],
        "a duplicate (module, name) key must be rejected; the same name under a \
         different module is a distinct record"
    );
}

/// Mutation plant for K2: a same-file key whose occurrence count drifts from
/// its expected count must be reported, as must an unregistered second
/// occurrence; a single occurrence needs no row.
#[test]
fn planted_same_file_occurrence_drift_turns_the_surface_red() {
    let absent_layer = "neovm-eval/src/emacs_core/runtime/eval/absent_layer.rs";
    let mut drifted = std::collections::BTreeMap::new();
    drifted.insert((absent_layer.to_owned(), "HANDLE".to_owned()), 3);
    let violations = same_file_occurrence_violations(&drifted, &[(absent_layer, "HANDLE", 2)]);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("3 scanned occurrence(s), expected 2"),
        "{violations:?}"
    );

    let mut unregistered = std::collections::BTreeMap::new();
    unregistered.insert(
        (
            "neovm-value/src/tagged/gc.rs".to_owned(),
            "TAGGED_HEAP".to_owned(),
        ),
        2,
    );
    let violations = same_file_occurrence_violations(&unregistered, &[]);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("no expected-count row"),
        "{violations:?}"
    );

    let mut single = std::collections::BTreeMap::new();
    single.insert(
        (
            "neovm-value/src/tagged/gc.rs".to_owned(),
            "TAGGED_HEAP".to_owned(),
        ),
        1,
    );
    assert!(same_file_occurrence_violations(&single, &[]).is_empty());

    // A stale expected-count row for a key the scan no longer finds is red.
    let violations = same_file_occurrence_violations(&single, &[(absent_layer, "HANDLE", 2)]);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("names no scanned static"),
        "{violations:?}"
    );
}

/// Module -> deferred carrier-reset step (or `workspace:<registered name>`),
/// used to cross-check every `MustReset` TLS record against the real reset
/// list in `carrier_tls.rs`. A record belongs to exactly one module; the
/// reset step named here must exist in `reset_carrier_tls` (the deferred
/// pass), and a `workspace:` entry must be registered by the editor binary
/// (`register_carrier_tls_reset` in `neomacs/src/main.rs`). A record added,
/// moved, reclassified or left without a reset fails
/// `every_must_reset_record_is_covered_by_a_real_carrier_reset`; removing or
/// renaming a reset step fails it too.
const MUST_RESET_MODULE_COVERAGE: &[(&str, &str)] = &[
    ("neovm-eval/src/emacs_core/runtime/eval/var_fast.rs", "eval"),
    ("neovm-value/src/tagged/mutate.rs", "gc"),
    ("neovm-core/src/emacs_core/display/xdisp/mod.rs", "xdisp"),
    ("neovm-core/src/emacs_core/editing/indent/mod.rs", "indent"),
    ("neovm-core/src/emacs_core/lisp/load/mod.rs", "load"),
    (
        "neovm-core/src/emacs_core/lisp/native/subr/leaf.rs",
        "subr_leaf",
    ),
    (
        "neovm-core/src/emacs_core/lisp/native/builtins/arithmetic.rs",
        "builtins_caches",
    ),
    (
        "neovm-core/src/emacs_core/lisp/print/error_format.rs",
        "error_format",
    ),
    ("neovm-core/src/emacs_core/lisp/print/mod.rs", "print"),
    ("neovm-core/src/emacs_core/runtime/jit/aot.rs", "jit_aot"),
    ("neovm-core/src/emacs_core/runtime/jit/bg.rs", "jit_bg"),
    (
        "neovm-core/src/emacs_core/runtime/jit/cache.rs",
        "jit_cache",
    ),
    (
        "neovm-core/src/emacs_core/runtime/jit/compile.rs",
        "jit_compile",
    ),
    (
        "neovm-core/src/emacs_core/runtime/jit/compile/",
        "jit_compile",
    ),
    (
        "neovm-core/src/emacs_core/runtime/jit/inline.rs",
        "jit_inline",
    ),
    (
        "neovm-core/src/emacs_core/runtime/jit/stats.rs",
        "jit_stats",
    ),
    ("neovm-core/src/emacs_core/runtime/jit/stats/", "jit_stats"),
    (
        "neovm-core/src/emacs_core/runtime/jit/tier2.rs",
        "jit_tier2",
    ),
    ("neovm-core/src/emacs_core/runtime/jit/tier2/", "jit_tier2"),
    (
        "neovm-core/src/emacs_core/runtime/eval/cconv_memo.rs",
        "eval",
    ),
    ("neovm-core/src/emacs_core/runtime/eval/mod.rs", "eval"),
    ("neovm-core/src/emacs_core/runtime/intern/mod.rs", "intern"),
    (
        "neovm-core/src/emacs_core/runtime/pdump/convert.rs",
        "pdump_convert",
    ),
    (
        "neovm-core/src/emacs_core/runtime/pdump/runtime.rs",
        "pdump_runtime",
    ),
    ("neovm-core/src/emacs_core/runtime/symbol/mod.rs", "symbol"),
    (
        "neovm-core/src/emacs_core/system/dynamic_module/mod.rs",
        "dynamic_module",
    ),
    (
        "neovm-core/src/emacs_core/system/perf_trace/mod.rs",
        "perf_trace",
    ),
    (
        "neovm-core/src/emacs_core/text/charset/mod.rs",
        "charset_memo",
    ),
    ("neovm-core/src/emacs_core/text/regex/dfa.rs", "regex_emacs"),
    (
        "neovm-core/src/emacs_core/text/regex/emacs.rs",
        "regex_emacs",
    ),
    ("neovm-core/src/emacs_core/text/regex/mod.rs", "regex"),
    ("neovm-value/src/string_pos_cache.rs", "string_pos_cache"),
    ("neovm-core/src/emacs_core/text/syntax/mod.rs", "syntax"),
    (
        "neovm-core/src/emacs_core/text/syntax/parse_cache.rs",
        "parse_cache",
    ),
    (
        "neovm-core/src/tagged/collection_reads.rs",
        "collection_reads",
    ),
    ("neovm-core/src/tagged/gc/cold_gc.rs", "gc"),
    ("neovm-core/src/tagged/value.rs", "value"),
    (
        "neomacs-display-protocol/src/input_latency.rs",
        "input_latency",
    ),
    ("neomacs-layout-engine/", "workspace:neomacs-layout-engine"),
    ("neomacs-terminfo/", "workspace:neomacs-terminfo"),
    ("neovm-edit/src/emacs_core/editing/indent/mod.rs", "indent"),
    ("neovm-eval/src/emacs_core/lisp/load/mod.rs", "load"),
    (
        "neovm-eval/src/emacs_core/lisp/native/subr/leaf.rs",
        "subr_leaf",
    ),
    (
        "neovm-eval/src/emacs_core/lisp/print/error_format.rs",
        "error_format",
    ),
    ("neovm-eval/src/emacs_core/lisp/print/mod.rs", "print"),
    ("neovm-eval/src/emacs_core/runtime/jit/aot.rs", "jit_aot"),
    ("neovm-eval/src/emacs_core/runtime/jit/bg.rs", "jit_bg"),
    (
        "neovm-eval/src/emacs_core/runtime/jit/cache.rs",
        "jit_cache",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile.rs",
        "jit_compile",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/compile/",
        "jit_compile",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/inline.rs",
        "jit_inline",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/jit/stats.rs",
        "jit_stats",
    ),
    ("neovm-eval/src/emacs_core/runtime/jit/stats/", "jit_stats"),
    (
        "neovm-eval/src/emacs_core/runtime/jit/tier2.rs",
        "jit_tier2",
    ),
    ("neovm-eval/src/emacs_core/runtime/jit/tier2/", "jit_tier2"),
    (
        "neovm-eval/src/emacs_core/runtime/eval/cconv_memo.rs",
        "eval",
    ),
    ("neovm-eval/src/emacs_core/runtime/eval/mod.rs", "eval"),
    (
        "neovm-eval/src/emacs_core/runtime/eval/data/xdisp_column_spec.rs",
        "xdisp",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/pdump/convert.rs",
        "pdump_convert",
    ),
    (
        "neovm-eval/src/emacs_core/runtime/pdump/runtime.rs",
        "pdump_runtime",
    ),
    (
        "neovm-system/src/emacs_core/system/dynamic_module/mod.rs",
        "dynamic_module",
    ),
    (
        "neovm-system/src/emacs_core/system/perf_trace/mod.rs",
        "perf_trace",
    ),
    (
        "neovm-text/src/emacs_core/text/charset/mod.rs",
        "charset_memo",
    ),
    ("neovm-text/src/emacs_core/text/regex/dfa.rs", "regex_emacs"),
    (
        "neovm-text/src/emacs_core/text/regex/emacs.rs",
        "regex_emacs",
    ),
    ("neovm-text/src/emacs_core/text/regex/mod.rs", "regex"),
    ("neovm-text/src/emacs_core/text/syntax/mod.rs", "syntax"),
    (
        "neovm-text/src/emacs_core/text/syntax/parse_cache.rs",
        "parse_cache",
    ),
    (
        "neovm-value/src/tagged/collection_reads.rs",
        "collection_reads",
    ),
    ("neovm-value/src/tagged/gc/cold_gc.rs", "gc"),
    ("neovm-value/src/tagged/value.rs", "value"),
    ("neovm-value/src/symbol/", "symbol"),
    ("neovm-value/src/intern/", "intern"),
];

/// `MustReset` records the source scan cannot see (they come from the binary
/// TLS census or a cfg-gated block the scan skips); each still needs an
/// explicit reset-step claim.
const MUST_RESET_EXTRA_COVERAGE: &[(&str, &str)] = &[];

/// Deferred reset steps whose records the name-keyed classification does not
/// (and should not) call `MustReset`: each entry names a record the step's
/// reset body clears, and the record-level scan below must still find it.
/// Without this the orphan check would demand a MustReset entry for state
/// that is deliberately `OwnerCarried`, or for a static name that is not
/// unique across modules.
const DEFERRED_STEPS_OUTSIDE_THE_TABLE: &[(&str, &str)] = &[
    // `arithmetic::reset_carrier_tls` drops the owner-carried integer-width
    // binding; the next activation rewrites it anyway, but the reuse reset
    // clears it before the prologue.
    ("builtins_caches", "INTEGER_WIDTH"),
    // `parse_cache::reset_carrier_tls` resets the parse-cache counters.
    // `STATS` is a static name several modules use, so the table's single
    // `STATS` entry maps to another module's static.
    ("parse_cache", "STATS"),
];

fn crates_dir() -> std::path::PathBuf {
    workspace_crates_dir()
}

fn simple_string_after<'a>(text: &'a str, mut at: usize) -> Option<&'a str> {
    let bytes = text.as_bytes();
    while at < bytes.len() && bytes[at].is_ascii_whitespace() {
        at += 1;
    }
    if bytes.get(at) != Some(&b'"') {
        return None;
    }
    let rest = &text[at + 1..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

/// The body of the top-level `carrier_tls.rs` function named in `header`, up
/// to and including its closing brace. Braces are counted textually; the two
/// functions this parses contain nothing but direct calls and step names, so
/// no string or comment can unbalance the count.
fn function_body<'a>(text: &'a str, header: &str) -> &'a str {
    let start = text
        .find(header)
        .unwrap_or_else(|| panic!("carrier_tls.rs has no `{header}`"));
    let open = start
        + text[start..]
            .find('{')
            .unwrap_or_else(|| panic!("`{header}` has no body"));
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &text[open..=open + offset];
                }
            }
            _ => {}
        }
    }
    panic!("`{header}` has no closing brace");
}

/// One parsed `reset_step!(...)` invocation.
struct CarrierResetStep {
    /// Literal step name; `None` for the registered-reset loop, whose first
    /// argument is the runtime `name` variable.
    name: Option<String>,
    /// Last identifier of the reset call target (`reset_carrier_tls` and
    /// friends), the root of the reset-side body scan.
    target: String,
}

fn reset_steps(text: &str, header: &str) -> Vec<CarrierResetStep> {
    let body = function_body(text, header);
    let mut steps = Vec::new();
    let mut cursor = 0;
    while let Some(found) = body[cursor..].find("reset_step!") {
        let after = cursor + found + "reset_step!".len();
        let open = after
            + body[after..]
                .find('(')
                .unwrap_or_else(|| panic!("`reset_step!` without an argument list"));
        let mut depth = 0usize;
        let mut close = None;
        for (offset, ch) in body[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(open + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let close = close.expect("unbalanced `reset_step!` arguments");
        let call = body[open + 1..close].trim_start();
        let (name, rest) = if let Some(name) = simple_string_after(call, 0) {
            let quote = call.find('"').expect("quoted step name");
            let after_name = quote + 1 + name.len();
            let comma = call[after_name..]
                .find(',')
                .map(|i| after_name + i)
                .unwrap_or_else(|| panic!("reset step `{name}` has no call argument"));
            (Some(name.to_owned()), &call[comma + 1..])
        } else {
            let comma = call
                .find(',')
                .unwrap_or_else(|| panic!("reset step `{call}` has no call argument"));
            (None, &call[comma + 1..])
        };
        let rest = rest.trim_start();
        let paren = rest
            .find('(')
            .unwrap_or_else(|| panic!("reset step target `{rest}` is not a direct call"));
        assert!(
            !rest[..paren].contains('{') && !rest[..paren].contains('|'),
            "reset step target `{}` is not a plain call path",
            &rest[..paren]
        );
        let mut start = paren;
        while start > 0 {
            let byte = rest.as_bytes()[start - 1];
            if byte.is_ascii_alphanumeric() || byte == b'_' {
                start -= 1;
            } else {
                break;
            }
        }
        assert!(
            start < paren,
            "reset step target `{rest}` has no callable identifier"
        );
        steps.push(CarrierResetStep {
            name,
            target: rest[start..paren].to_owned(),
        });
        cursor = close;
    }
    steps
}

/// Registered workspace resets: registration name -> reset function name.
/// Parses `register_carrier_tls_reset(CarrierTlsReset::new("name",
/// path::reset_fn, ...))` and the `with_fingerprint` form in the editor
/// binary.
fn registered_workspace_resets(text: &str) -> std::collections::BTreeMap<String, String> {
    let mut resets = std::collections::BTreeMap::new();
    let mut cursor = 0;
    while let Some(found) = text[cursor..].find("register_carrier_tls_reset(") {
        let after = cursor + found + "register_carrier_tls_reset(".len();
        let quote = after
            + text[after..]
                .find('"')
                .unwrap_or_else(|| panic!("carrier reset registration without a name"));
        let rest = &text[quote + 1..];
        let end = rest
            .find('"')
            .expect("unterminated carrier reset registration name");
        let name = &rest[..end];
        let after_name = quote + 1 + end + 1;
        let comma = after_name
            + text[after_name..]
                .find(',')
                .expect("carrier reset registration without a reset argument");
        let arg_start = comma + 1;
        let arg_end = arg_start
            + text[arg_start..]
                .find(|c: char| c == ',' || c == ')')
                .expect("unterminated carrier reset registration");
        let path = text[arg_start..arg_end].trim();
        let target = path
            .rsplit("::")
            .next()
            .expect("carrier reset registration path");
        assert!(
            target
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_'),
            "carrier reset registration `{name}` names no reset function: `{path}`"
        );
        resets.insert(name.to_owned(), target.to_owned());
        cursor = after;
    }
    resets
}

fn must_reset_module_step(file: &str) -> Option<&'static str> {
    MUST_RESET_MODULE_COVERAGE
        .iter()
        .find(|(prefix, _)| file.starts_with(prefix))
        .map(|(_, step)| *step)
}

/// Workspace function bodies, keyed by source file and function name. Every
/// same-named function in a file is kept: a file can hold several `clear`
/// bodies and the reset scan must see all of them.
type FunctionBodies =
    std::collections::BTreeMap<String, std::collections::BTreeMap<String, Vec<String>>>;

fn workspace_functions(crates: &Path) -> FunctionBodies {
    let mut found = FunctionBodies::new();
    visit_functions(crates, crates, &mut found);
    found
}

fn visit_functions(root: &Path, dir: &Path, found: &mut FunctionBodies) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            visit_functions(root, &path, found);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .unwrap_or_default();
        if rel.contains("/tests/") || rel.ends_with("_test.rs") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        collect_function_bodies(&rel, &text, found);
    }
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Byte ranges of Rust code: comments, string literals, raw strings, byte
/// strings and char literals are excluded, so a brace inside any of them can
/// neither open nor close a function body. Lifetimes are not char literals
/// and stay in code.
fn code_ranges(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut ranges = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    while i < len {
        let byte = bytes[i];
        let at_token_start = i == 0 || !is_ident_byte(bytes[i - 1]);
        if at_token_start && (byte == b'r' || (byte == b'b' && i + 1 < len && bytes[i + 1] == b'r'))
        {
            let mut j = i + if byte == b'r' { 1 } else { 2 };
            let mut hashes = 0usize;
            while j < len && bytes[j] == b'#' {
                hashes += 1;
                j += 1;
            }
            if j < len && bytes[j] == b'"' {
                if start < i {
                    ranges.push((start, i));
                }
                j += 1;
                while j < len {
                    if bytes[j] == b'"' {
                        let mut k = j + 1;
                        let mut seen = 0usize;
                        while seen < hashes && k < len && bytes[k] == b'#' {
                            seen += 1;
                            k += 1;
                        }
                        if seen == hashes {
                            j = k;
                            break;
                        }
                    }
                    j += 1;
                }
                i = j;
                start = i;
                continue;
            }
        }
        if at_token_start
            && byte == b'b'
            && i + 1 < len
            && (bytes[i + 1] == b'"' || bytes[i + 1] == b'\'')
        {
            // Byte string / byte char: the quote handler below skips it.
            i += 1;
            continue;
        }
        match byte {
            b'/' if i + 1 < len && bytes[i + 1] == b'/' => {
                if start < i {
                    ranges.push((start, i));
                }
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
                start = i;
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                if start < i {
                    ranges.push((start, i));
                }
                let mut depth = 1usize;
                i += 2;
                while i < len && depth > 0 {
                    if i + 1 < len && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if i + 1 < len && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                start = i;
            }
            b'"' => {
                if start < i {
                    ranges.push((start, i));
                }
                i += 1;
                while i < len {
                    if bytes[i] == b'\\' {
                        i += 2;
                    } else if bytes[i] == b'"' {
                        i += 1;
                        break;
                    } else {
                        i += 1;
                    }
                }
                start = i;
            }
            b'\'' => {
                if i + 1 < len && bytes[i + 1] == b'\\' {
                    if start < i {
                        ranges.push((start, i));
                    }
                    i += 2;
                    while i < len && bytes[i] != b'\'' {
                        i += 1;
                    }
                    if i < len {
                        i += 1;
                    }
                    start = i;
                } else {
                    let lead = bytes.get(i + 1).copied().unwrap_or(0);
                    let char_len = if lead < 0x80 {
                        1
                    } else if lead >> 5 == 0b110 {
                        2
                    } else if lead >> 4 == 0b1110 {
                        3
                    } else {
                        4
                    };
                    let after = i + 1 + char_len;
                    if lead != 0 && after < len && bytes[after] == b'\'' {
                        if start < i {
                            ranges.push((start, i));
                        }
                        i = after + 1;
                        start = i;
                    } else {
                        i += 1;
                    }
                }
            }
            _ => i += 1,
        }
    }
    if start < len {
        ranges.push((start, len));
    }
    ranges
}

fn code_contains(ranges: &[(usize, usize)], pos: usize) -> bool {
    ranges.iter().any(|&(a, b)| a <= pos && pos < b)
}

fn first_code_after(ranges: &[(usize, usize)], pos: usize) -> Option<usize> {
    for &(a, b) in ranges {
        if b <= pos {
            continue;
        }
        let at = a.max(pos);
        return (at < b).then_some(at);
    }
    None
}

fn first_code_brace(text: &str, ranges: &[(usize, usize)], pos: usize) -> Option<usize> {
    for &(a, b) in ranges {
        if b <= pos {
            continue;
        }
        let at = a.max(pos);
        if let Some(offset) = text[at..b].find('{') {
            return Some(at + offset);
        }
    }
    None
}

fn code_has_semicolon(text: &str, ranges: &[(usize, usize)], from: usize, to: usize) -> bool {
    let bytes = text.as_bytes();
    for &(a, b) in ranges {
        if b <= from {
            continue;
        }
        if a >= to {
            break;
        }
        let lo = a.max(from);
        let hi = b.min(to);
        if lo < hi && bytes[lo..hi].contains(&b';') {
            return true;
        }
    }
    false
}

fn matching_brace(text: &str, ranges: &[(usize, usize)], open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    for &(a, b) in ranges {
        if b <= open {
            continue;
        }
        let lo = a.max(open);
        if lo >= b {
            continue;
        }
        for (offset, &byte) in bytes[lo..b].iter().enumerate() {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    if depth == 0 {
                        return None;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return Some(lo + offset);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn collect_function_bodies(rel: &str, text: &str, found: &mut FunctionBodies) {
    let bytes = text.as_bytes();
    let ranges = code_ranges(text);
    let mut cursor = 0;
    while let Some(offset) = text[cursor..].find("fn ") {
        let at = cursor + offset;
        if !code_contains(&ranges, at) {
            cursor = at + 1;
            continue;
        }
        if at > 0 && is_ident_byte(bytes[at - 1]) {
            cursor = at + 1;
            continue;
        }
        let name_start = at + "fn ".len();
        let mut name_end = name_start;
        while name_end < bytes.len() && is_ident_byte(bytes[name_end]) {
            name_end += 1;
        }
        if name_end == name_start {
            cursor = at + 1;
            continue;
        }
        let name = &text[name_start..name_end];
        let Some(first) = first_code_after(&ranges, name_end) else {
            break;
        };
        if !(bytes[first] == b'(' || bytes[first] == b'<') {
            cursor = at + 1;
            continue;
        }
        let Some(open) = first_code_brace(text, &ranges, name_end) else {
            break;
        };
        // A declaration (`fn name(...);`, e.g. a trait method) has a `;` in
        // the signature; do not attribute the following item's body to it.
        if code_has_semicolon(text, &ranges, name_end, open) {
            cursor = at + 1;
            continue;
        }
        let (body, next) = match matching_brace(text, &ranges, open) {
            Some(close) => (text[open..=close].to_owned(), close + 1),
            // Unterminated construct: keep the rest as the body (the record
            // mention then still sees anything the function may clear) and
            // keep scanning from the next byte.
            None => (text[open..].to_owned(), open + 1),
        };
        found
            .entry(rel.to_owned())
            .or_default()
            .entry(name.to_owned())
            .or_default()
            .push(body);
        cursor = next;
    }
}

/// Unqualified callee identifiers in a function body (`clear()` yes,
/// `module::clear()` and `value.clear()` no).
fn unqualified_calls(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut calls = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let mut after = i;
            while after < bytes.len() && bytes[after].is_ascii_whitespace() {
                after += 1;
            }
            let qualified = start >= 2 && bytes[start - 1] == b':' && bytes[start - 2] == b':';
            let method = start >= 1 && bytes[start - 1] == b'.';
            if after < bytes.len() && bytes[after] == b'(' && !qualified && !method {
                calls.push(&body[start..i]);
            }
        } else {
            i += 1;
        }
    }
    calls
}

fn mentions_identifier(text: &str, needle: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if &text[start..i] == needle {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

/// Functions a deferred reset step reaches: every workspace function whose
/// name is a reset-step target, plus the same-file free functions those
/// bodies call (e.g. `jit::cache::reset_carrier_tls` calls the module's
/// `clear`, which is where the cached jit state actually lives).
fn reset_reachable_functions(
    functions: &FunctionBodies,
    roots: &std::collections::BTreeSet<String>,
) -> std::collections::BTreeSet<(String, String)> {
    let mut reachable = std::collections::BTreeSet::new();
    let mut work = Vec::new();
    for (file, by_name) in functions {
        for root in roots {
            if by_name.contains_key(root) && reachable.insert((file.clone(), root.clone())) {
                work.push((file.clone(), root.clone()));
            }
        }
    }
    while let Some((file, name)) = work.pop() {
        let bodies = &functions[&file][&name];
        for body in bodies {
            for callee in unqualified_calls(body) {
                if functions[&file].contains_key(callee)
                    && reachable.insert((file.clone(), callee.to_owned()))
                {
                    work.push((file.clone(), callee.to_owned()));
                }
            }
        }
    }
    reachable
}

#[test]
fn every_must_reset_record_is_covered_by_a_real_carrier_reset() {
    let crates = crates_dir();
    let carrier_tls = std::fs::read_to_string(production_file(
        "emacs_core/runtime/eval/green/carrier_tls.rs",
    ))
    .expect("read carrier_tls.rs");
    let main_rs = std::fs::read_to_string(crates.join("neomacs/src/main.rs"))
        .expect("read neomacs/src/main.rs");

    let park_steps = reset_steps(&carrier_tls, "pub(crate) fn park_carrier_tls()");
    let deferred_steps = reset_steps(&carrier_tls, "pub(crate) fn reset_carrier_tls()");
    let park: std::collections::BTreeSet<String> = park_steps
        .iter()
        .filter_map(|step| step.name.clone())
        .collect();
    let deferred: std::collections::BTreeSet<String> = deferred_steps
        .iter()
        .filter_map(|step| step.name.clone())
        .collect();
    let registered = registered_workspace_resets(&main_rs);
    assert!(!park.is_empty(), "no park reset steps parsed");
    assert!(!deferred.is_empty(), "no deferred reset steps parsed");
    assert!(
        !registered.is_empty(),
        "no registered workspace resets parsed"
    );
    let both: Vec<_> = park.intersection(&deferred).cloned().collect();
    assert!(
        both.is_empty(),
        "reset step names used by both passes (park/deferred must be disjoint): {both:?}"
    );

    let mut must_reset_names = std::collections::BTreeSet::new();
    for entry in TLS_CLASSIFICATION.iter() {
        if matches!(entry.2, TlsClass::MustReset) {
            must_reset_names.insert((entry.1).to_owned());
        }
    }

    let scanned = scanned_tls_names();
    let mut covered = std::collections::BTreeSet::new();
    let mut claimed_steps = std::collections::BTreeSet::new();
    let mut claimed_workspace = std::collections::BTreeSet::new();

    for (name, file) in &scanned {
        if !matches!(classify(file, name), Some(TlsClass::MustReset)) {
            continue;
        }
        covered.insert(name.clone());
        let step = must_reset_module_step(file).unwrap_or_else(|| {
            panic!(
                "MustReset TLS record `{name}` in {file} has no module -> reset coverage; add \
                 the module to MUST_RESET_MODULE_COVERAGE and point it at the reset that clears \
                 the record"
            )
        });
        if let Some(workspace) = step.strip_prefix("workspace:") {
            assert!(
                registered.contains_key(workspace),
                "MustReset record `{name}` in {file} is covered by workspace reset `{workspace}`, \
                 but neomacs/src/main.rs registers no such reset"
            );
            claimed_workspace.insert(workspace.to_owned());
        } else {
            assert!(
                deferred.contains(step),
                "MustReset record `{name}` in {file} maps to reset step `{step}`, which is not in \
                 reset_carrier_tls(); the table and carrier_tls.rs have drifted"
            );
            claimed_steps.insert(step.to_owned());
        }
    }

    for (name, step) in MUST_RESET_EXTRA_COVERAGE {
        assert!(
            must_reset_names.contains(*name),
            "MUST_RESET_EXTRA_COVERAGE claims `{name}`, not a MustReset table entry"
        );
        covered.insert((*name).to_owned());
        if let Some(workspace) = step.strip_prefix("workspace:") {
            assert!(
                registered.contains_key(workspace),
                "extra coverage for `{name}` names workspace reset `{workspace}`, which is not \
                 registered in neomacs/src/main.rs"
            );
            claimed_workspace.insert(workspace.to_owned());
        } else {
            assert!(
                deferred.contains(*step),
                "extra coverage for `{name}` names reset step `{step}`, which is not in \
                 reset_carrier_tls()"
            );
            claimed_steps.insert((*step).to_owned());
        }
    }

    let missing: Vec<_> = must_reset_names.difference(&covered).collect();
    assert!(
        missing.is_empty(),
        "MustReset table entries with no reset coverage: {missing:?}; every MustReset record \
         must be cleared by a real reset (add it to MUST_RESET_MODULE_COVERAGE only when the \
         reset exists)"
    );

    // Record-level cross-check. The module table above binds every record to
    // a step name, but a step name alone cannot show that the record is
    // actually cleared: `REDISPLAY_RUNTIME` once sat in a module with a
    // registered workspace reset that never touched it. Here every MustReset
    // record must be named inside a function the deferred reset list reaches
    // (the reset step's target, or a same-file free function that target
    // calls, e.g. `jit::cache::reset_carrier_tls` -> `clear`).
    let mut roots: std::collections::BTreeSet<String> = deferred_steps
        .iter()
        .filter(|step| step.name.is_some())
        .map(|step| step.target.clone())
        .collect();
    roots.extend(registered.values().cloned());
    assert!(!roots.is_empty(), "no reset-side roots parsed");
    let functions = workspace_functions(&crates);
    assert!(
        !functions.is_empty(),
        "the reset scan must find workspace function bodies"
    );
    let reachable = reset_reachable_functions(&functions, &roots);
    assert!(
        !reachable.is_empty(),
        "no reset-side function bodies found for {roots:?}"
    );
    let reset_mentions = |name: &str| {
        reachable.iter().any(|(file, function)| {
            functions[file][function]
                .iter()
                .any(|body| mentions_identifier(body, name))
        })
    };

    // A deferred step may clear state the name-keyed table classes outside
    // MustReset (an OwnerCarried binding, or a static name shared by several
    // modules); those steps are declared with the record their reset body
    // names, and the declaration is checked against the body scan.
    let mut excused_steps = std::collections::BTreeSet::new();
    for (step, record) in DEFERRED_STEPS_OUTSIDE_THE_TABLE {
        assert!(
            deferred.contains(*step),
            "excused deferred step `{step}` is not in reset_carrier_tls()"
        );
        assert!(
            reset_mentions(record),
            "excused deferred step `{step}` claims to clear `{record}`, but no reachable reset \
             body names that record"
        );
        excused_steps.insert((*step).to_owned());
    }

    let orphan_steps: Vec<_> = deferred
        .iter()
        .filter(|step| !claimed_steps.contains(*step) && !excused_steps.contains(*step))
        .collect();
    assert!(
        orphan_steps.is_empty(),
        "deferred reset steps that no MustReset record and no declaration claims: \
         {orphan_steps:?}; reclassify the record the step clears, or declare it in \
         DEFERRED_STEPS_OUTSIDE_THE_TABLE with the record it names"
    );
    let orphan_workspace: Vec<_> = registered
        .keys()
        .filter(|name| !claimed_workspace.contains(*name))
        .collect();
    assert!(
        orphan_workspace.is_empty(),
        "registered workspace resets that no MustReset record claims: {orphan_workspace:?}"
    );

    let mut unreset: Vec<&String> = Vec::new();
    for name in &must_reset_names {
        if !reset_mentions(name) {
            unreset.push(name);
        }
    }
    assert!(
        unreset.is_empty(),
        "MustReset records that no deferred reset step clears: {unreset:?}; the table says they \
         must be reset, but no function reached from reset_carrier_tls() (or from a registered \
         workspace reset) names them. Add the reset, or classify the record CarrierIntrinsic / \
         OwnerCarried if nothing clears it"
    );
}
