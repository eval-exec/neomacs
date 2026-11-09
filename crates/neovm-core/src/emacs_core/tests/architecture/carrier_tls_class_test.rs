//! Architecture guard: every production `thread_local!` static has a
//! carrier TLS class (peer review condition 4).
//!
//! The authoritative binary census is the 226 TLS symbols in the shipped
//! profiling binary; this scan covers source statics, including the ones a
//! feature/cfg combination keeps out of that binary. A new `thread_local!`
//! without an entry here fails the test.

use std::path::Path;

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
/// Sentinel module for a classified record that the production source
/// scan does not find: a shipped-binary census record (linker TLS,
/// macro-generated `{closure}` slots) or a cfg(test)-gated block the
/// scan deliberately skips.
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
    (BINARY_CENSUS, "CHAR_TABLE_ASCII_FOLD", TlsClass::MustReset),
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
    (BINARY_CENSUS, "DB_HANDLES", TlsClass::CarrierIntrinsic),
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
    (BINARY_CENSUS, "IMM_POOL", TlsClass::MustReset),
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
    (BINARY_CENSUS, "IN_HEAP_MUT_CLOSURE", TlsClass::SourceOnly),
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
    (BINARY_CENSUS, "RESULT_SETS", TlsClass::CarrierIntrinsic),
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
        "SYNTAX_FLAT_ASCII_ENTRIES",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "SYNTAX_FLAT_ASCII_ENTRY_CACHE",
        TlsClass::MustReset,
    ),
    (
        "neovm-text/src/emacs_core/text/syntax/mod.rs",
        "SYNTAX_FLAT_ASCII_ENTRY_ENTRIES",
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

fn is_test_only_block(before: &str) -> bool {
    before.contains("#[cfg(test)]")
        || before.contains("cfg(all(test")
        || before.contains("cfg(any(test")
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
            let before = &text[start.saturating_sub(200)..start];
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
                        found.push((name, rel.clone()));
                    }
                    cursor = at + name_len;
                }
            }
            search = end.max(start + 1);
        }
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
const MUST_RESET_EXTRA_COVERAGE: &[(&str, &str)] = &[
    ("IMM_POOL", "jit_compile"),
    // The record sits directly below a cfg(test)/test-support const, so the
    // source scanner's test-only window skips its thread_local! block; the
    // regex reset step clears it (regex/emacs.rs reset_carrier_tls).
    ("CHAR_TABLE_ASCII_FOLD", "regex_emacs"),
];

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
