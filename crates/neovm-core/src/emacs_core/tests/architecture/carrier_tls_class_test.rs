//! Architecture guard: every production `thread_local!` static has a
//! carrier TLS class (peer review condition 4).
//!
//! The authoritative binary census is the 226 TLS symbols in the shipped
//! profiling binary; this scan covers source statics, including the ones a
//! feature/cfg combination keeps out of that binary. A new `thread_local!`
//! without an entry here fails the test.

use std::path::Path;

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
const TLS_CLASSIFICATION: &[(&str, TlsClass)] = &[
    ("ACTIVE", TlsClass::MustReset),
    ("ACTIVE_ARRAY_KINDS", TlsClass::MustReset),
    ("ACTIVE_BUILD", TlsClass::MustReset),
    ("ACTIVE_CALL_HEAVY", TlsClass::MustReset),
    ("ACTIVE_CALL_SOURCE", TlsClass::MustReset),
    ("ACTIVE_CALL_TARGETS_READ", TlsClass::MustReset),
    ("ACTIVE_ENVS", TlsClass::MustReset),
    ("ACTIVE_FEEDBACK_HOLDS", TlsClass::MustReset),
    ("ACTIVE_FUSED", TlsClass::MustReset),
    ("ACTIVE_LABEL", TlsClass::MustReset),
    ("ACTIVE_NO_INLINE_CALL_SITES", TlsClass::MustReset),
    ("ACTIVE_NUMERIC_FEEDBACK", TlsClass::MustReset),
    ("ACTIVE_REGALLOC", TlsClass::MustReset),
    ("ACTIVE_REGION", TlsClass::MustReset),
    ("ALLOCATION_HINT", TlsClass::MustReset),
    ("ATOM_NAME_CACHE", TlsClass::CarrierIntrinsic),
    ("BUF", TlsClass::CarrierIntrinsic),
    ("BUFFER", TlsClass::MustReset),
    ("BUSES", TlsClass::CarrierIntrinsic),
    ("BYPASS_PROFIT_GATE", TlsClass::MustReset),
    ("BYTECODE_BRANCH_POLL_COUNT", TlsClass::SourceOnly),
    ("BYTECODE_DATA_ACCESS_COUNT", TlsClass::SourceOnly),
    ("CACHE", TlsClass::OwnerCarried),
    ("CACHED", TlsClass::CarrierIntrinsic),
    ("CACHE_COLLECTION_EPOCH", TlsClass::OwnerCarried),
    ("CACHE_HEAP", TlsClass::OwnerCarried),
    ("CALL_KIND_COUNTS", TlsClass::SourceOnly),
    ("CALL_SITES", TlsClass::SourceOnly),
    ("CCL_REGISTRY", TlsClass::OwnerCarried),
    // CarrierIntrinsic: `with_ccl_obarray` publishes the view pointer in a
    // scoped slot and a `Drop` guard restores the previous (null) value
    // before the frame ends (`ccl/mod.rs:139-167`), including on unwind, so
    // the record is null between jobs and never carries retired state.
    ("CCL_VARIABLES", TlsClass::CarrierIntrinsic),
    ("CHARSET_MAP_MEMO", TlsClass::MustReset),
    ("CHARSET_REGISTRY", TlsClass::OwnerCarried),
    ("CHAR_TABLE_ASCII_FOLD", TlsClass::MustReset),
    (
        "CHAR_TABLE_LAYOUT_MUTATION_EPOCH",
        TlsClass::CarrierIntrinsic,
    ),
    ("CHROME_GENERATION_RECORD", TlsClass::MustReset),
    ("CLOSE_COUNT", TlsClass::CarrierIntrinsic),
    ("COLUMN_SPEC_CONSUMED", TlsClass::MustReset),
    ("COMPACTION_IN_PROGRESS", TlsClass::CarrierIntrinsic),
    ("COMPILED", TlsClass::MustReset),
    ("COMPILED_HEAP", TlsClass::MustReset),
    ("COMPILED_OBARRAY", TlsClass::MustReset),
    ("COMPOSITION_REGISTRY", TlsClass::CarrierIntrinsic),
    ("CONCURRENT_HASH_MUTATOR_ID", TlsClass::MustReset),
    ("CONSUMED", TlsClass::MustReset),
    ("CONTEXT", TlsClass::CarrierIntrinsic),
    ("CORE_PDUMP_HOOKS", TlsClass::CarrierIntrinsic),
    ("CURRENT", TlsClass::MustReset),
    ("CURRENT_LEAF_BASES", TlsClass::MustReset),
    ("CURRENT_PARKER", TlsClass::CarrierIntrinsic),
    ("CURRENT_PASS", TlsClass::CarrierIntrinsic),
    ("DB_HANDLES", TlsClass::CarrierIntrinsic),
    ("DEFERRED", TlsClass::MustReset),
    ("DEFER_SCOPE", TlsClass::MustReset),
    ("DEOPT_TAIL", TlsClass::MustReset),
    ("DESTROYED_FIRST", TlsClass::SourceOnly),
    ("DROPPED_LEAF_TOTALS", TlsClass::MustReset),
    ("DTORS", TlsClass::CarrierIntrinsic),
    ("EAGER_EXPANSION_FAILURE", TlsClass::MustReset),
    ("ENTRY_COUNTS", TlsClass::SourceOnly),
    ("ENV", TlsClass::MustReset),
    ("EPOCH", TlsClass::MustReset),
    ("EPOCH_DISCARDS", TlsClass::MustReset),
    ("FACE_ATTR_STATE", TlsClass::OwnerCarried),
    ("FAULT", TlsClass::SourceOnly),
    ("FILE_NOTIFY_STATE", TlsClass::OwnerCarried),
    ("FILTERING", TlsClass::CarrierIntrinsic),
    ("FINGERPRINT_RECORD", TlsClass::MustReset),
    ("FLONUM_CENSUS", TlsClass::MustReset),
    ("FN_CELL_UNCHANGED", TlsClass::MustReset),
    ("FN_EPOCH_BUMPS", TlsClass::MustReset),
    ("FN_REDEFINED", TlsClass::MustReset),
    ("FOO", TlsClass::CarrierIntrinsic),
    ("FORMAT_OBJECT_STACK", TlsClass::MustReset),
    ("GATE_THREAD", TlsClass::SourceOnly),
    ("GLOBAL_REFS", TlsClass::OwnerCarried),
    ("GLOBAL_SUBR_LOOKUP_COUNT", TlsClass::SourceOnly),
    ("GLOBAL_SUBR_TABLE", TlsClass::MustReset),
    ("GUARD_COUNT", TlsClass::MustReset),
    ("HANDLE", TlsClass::CarrierIntrinsic),
    ("HASH_TABLE_TEST_ALIASES", TlsClass::OwnerCarried),
    ("HEADER_LINE_EVAL_COUNT", TlsClass::MustReset),
    ("HOTPATH_STATE", TlsClass::MustReset),
    ("ID", TlsClass::CarrierIntrinsic),
    ("IMM_POOL", TlsClass::MustReset),
    ("INLINE_CENSUS", TlsClass::MustReset),
    ("INLINE_DEPS", TlsClass::MustReset),
    ("INLINE_EVICTED_LEAVES", TlsClass::MustReset),
    ("INLINE_EVICTIONS", TlsClass::MustReset),
    ("INLINE_HEAP_SITES", TlsClass::MustReset),
    ("INLINE_SUBR_TABLE", TlsClass::MustReset),
    ("INTEGER_WIDTH", TlsClass::OwnerCarried),
    ("INTERN_STR_CACHE", TlsClass::CarrierIntrinsic),
    ("IN_FLIGHT_ROOTS", TlsClass::OwnerCarried),
    ("IN_HEAP_MUT_CLOSURE", TlsClass::SourceOnly),
    ("LAMBDA_LIST", TlsClass::MustReset),
    ("LAST_CLIF_SIZE", TlsClass::MustReset),
    ("LAST_INSTANT", TlsClass::CarrierIntrinsic),
    ("LAST_IR_STATS", TlsClass::MustReset),
    ("LAST_KNOWN_COLUMN", TlsClass::MustReset),
    ("LEAF_ACTIVE", TlsClass::SourceOnly),
    ("LEAF_BY_SUBR", TlsClass::MustReset),
    ("LEDGER", TlsClass::MustReset),
    ("LISP_REGEX_PATTERN_CACHE", TlsClass::MustReset),
    ("LITERAL_TRT_CACHE", TlsClass::MustReset),
    ("LOADED_UNITS", TlsClass::MustReset),
    ("LOCAL_WAKER", TlsClass::CarrierIntrinsic),
    ("LOCK_HELD", TlsClass::CarrierIntrinsic),
    ("LOOP_MARK", TlsClass::MustReset),
    ("LOSSAGE_SIZE", TlsClass::CarrierIntrinsic),
    ("MATCHER_OVERFLOW", TlsClass::MustReset),
    ("MATCH_SCRATCH", TlsClass::MustReset),
    ("MATERIALIZE_CALL_COUNT", TlsClass::SourceOnly),
    ("MENU_BAR_ITEMS_CACHE", TlsClass::MustReset),
    ("MIR_BAIL_REASONS", TlsClass::MustReset),
    ("MODE_LINE_EVAL_COUNT", TlsClass::MustReset),
    ("MODE_OVERRIDE", TlsClass::SourceOnly),
    ("MODULE_CTX", TlsClass::MustReset),
    ("NAME_CANONICAL_SYMBOL_CACHE", TlsClass::CarrierIntrinsic),
    ("NEOMACS_CLIPBOARD_TEXT", TlsClass::CarrierIntrinsic),
    ("NEOMACS_MONITORS", TlsClass::CarrierIntrinsic),
    ("NEOMACS_PRIMARY_SELECTION_TEXT", TlsClass::CarrierIntrinsic),
    ("OBSERVATION_STATE_ACCESSES", TlsClass::SourceOnly),
    ("OBSERVED", TlsClass::CarrierIntrinsic),
    ("OBSERVED_FUNCTION_WRITES", TlsClass::MustReset),
    ("OBSERVED_OWNER_PUBLICATIONS", TlsClass::SourceOnly),
    ("OP_COUNTS", TlsClass::SourceOnly),
    ("OP_PAIR_COUNTS", TlsClass::SourceOnly),
    ("OSR_CACHE", TlsClass::MustReset),
    ("OSR_PENDING", TlsClass::MustReset),
    ("OVERRIDE", TlsClass::SourceOnly),
    ("OWNED", TlsClass::SourceOnly),
    ("PASS_TIME", TlsClass::CarrierIntrinsic),
    ("PDUMP_LOAD_NAME_REMAP", TlsClass::MustReset),
    ("PDUMP_LOAD_SYM_IDENTITY", TlsClass::MustReset),
    ("PDUMP_LOAD_SYM_REMAP", TlsClass::MustReset),
    ("PDUMP_RUNTIME_STATE", TlsClass::MustReset),
    ("PENDING", TlsClass::MustReset),
    ("PENDING_CALLEE", TlsClass::MustReset),
    ("PENDING_COUNT", TlsClass::MustReset),
    ("PENDING_FLOW", TlsClass::MustReset),
    ("PENDING_IDS", TlsClass::MustReset),
    ("PENDING_ROOT_SWEEP_FLOOR", TlsClass::MustReset),
    ("PENDING_SHIM_PANIC", TlsClass::MustReset),
    ("PER_HASHER_NONDETERMINISM", TlsClass::CarrierIntrinsic),
    ("PHASE_CLOCK", TlsClass::MustReset),
    ("PIKE_FALLBACK", TlsClass::MustReset),
    ("PIKE_SCRATCH", TlsClass::MustReset),
    ("PRELOAD_UNIT", TlsClass::MustReset),
    ("PREV_OP", TlsClass::SourceOnly),
    ("PREWARM_HASHES", TlsClass::MustReset),
    ("PREWARM_STUBS", TlsClass::MustReset),
    ("PREWARM_STUBS_PENDING", TlsClass::MustReset),
    ("PRINT_BYTES_OBJECT_STACK", TlsClass::MustReset),
    ("PRINT_CALL_DEPTH", TlsClass::MustReset),
    ("PRINT_NUMBER_INDEX", TlsClass::MustReset),
    ("PRINT_OBJECT_STACK", TlsClass::MustReset),
    ("PROCESS_NAME_REGISTRY", TlsClass::CarrierIntrinsic),
    ("PROFILER", TlsClass::CarrierIntrinsic),
    ("QUIT_REQUESTED_TLS", TlsClass::OwnerCarried),
    ("RANDOM_STATE", TlsClass::SourceOnly),
    ("RECENT_CLEARED_CELLS", TlsClass::SourceOnly),
    ("RECENT_CLEARED_MASKS", TlsClass::SourceOnly),
    ("RECENT_READS", TlsClass::MustReset),
    ("REDEFINED_PINS", TlsClass::MustReset),
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
    ("REDISPLAY_RUNTIME", TlsClass::CarrierIntrinsic),
    ("REGEX_CACHE_COLLECTION_EPOCH", TlsClass::CarrierIntrinsic),
    ("REGEX_CACHE_HEAP", TlsClass::CarrierIntrinsic),
    ("REGISTERED", TlsClass::CarrierIntrinsic),
    ("REGISTRATION", TlsClass::CarrierIntrinsic),
    // The green guard's debug-only emission counter (jit/compile/lowering.rs):
    // a cfg(debug_assertions) `Cell<u64>` that holds no Value.
    ("RELEASE_REVALIDATION_EMITTED", TlsClass::SourceOnly),
    ("RESULT_SETS", TlsClass::CarrierIntrinsic),
    ("RETAG_COUNT", TlsClass::MustReset),
    ("REVISION", TlsClass::CarrierIntrinsic),
    ("RNG", TlsClass::CarrierIntrinsic),
    ("ROOTWIN_CARRY", TlsClass::MustReset),
    ("SCOPE", TlsClass::SourceOnly),
    ("SCRATCH_GC_ROOTS", TlsClass::MustReset),
    ("SCRATCH_SPACE", TlsClass::CarrierIntrinsic),
    ("SEARCH_PATTERN_CACHE", TlsClass::MustReset),
    ("SEED_SKIP_OBARRAY_SYMBOL_CELLS", TlsClass::MustReset),
    ("SELF_SOURCE", TlsClass::MustReset),
    ("SESSION", TlsClass::SourceOnly),
    ("SHARED_JIT", TlsClass::MustReset),
    ("SINK", TlsClass::MustReset),
    ("SPEC_REVALIDATIONS", TlsClass::MustReset),
    ("STACK_CURSOR_LIVE", TlsClass::SourceOnly),
    ("STACK_LIMIT", TlsClass::CarrierIntrinsic),
    ("STANDARD_BYTE", TlsClass::CarrierIntrinsic),
    ("STANDARD_CASE_TABLE_HEAP", TlsClass::OwnerCarried),
    ("STANDARD_CASE_TABLE_OBJECT", TlsClass::OwnerCarried),
    ("STANDARD_CATEGORY_TABLE_HEAP", TlsClass::OwnerCarried),
    ("STANDARD_CATEGORY_TABLE_OBJECT", TlsClass::OwnerCarried),
    ("STANDARD_SYNTAX_TABLE_HEAP", TlsClass::OwnerCarried),
    ("STANDARD_SYNTAX_TABLE_OBJECT", TlsClass::OwnerCarried),
    ("STATE", TlsClass::CarrierIntrinsic),
    ("STATIC_SUBR_OBJECTS", TlsClass::MustReset),
    ("STATS", TlsClass::MustReset),
    ("SUBR_BY_SYM", TlsClass::MustReset),
    ("SUBR_COUNTS", TlsClass::SourceOnly),
    ("SURVIVOR", TlsClass::SourceOnly),
    ("SYMBOL_CACHE", TlsClass::CarrierIntrinsic),
    ("SYNTAX_CODE_OBJECTS", TlsClass::OwnerCarried),
    ("SYNTAX_CODE_OBJECTS_HEAP", TlsClass::OwnerCarried),
    ("SYNTAX_FLAT_ASCII_CACHE", TlsClass::MustReset),
    ("SYNTAX_FLAT_ASCII_ENTRIES", TlsClass::MustReset),
    ("SYNTAX_FLAT_ASCII_ENTRY_CACHE", TlsClass::MustReset),
    ("SYNTAX_FLAT_ASCII_ENTRY_ENTRIES", TlsClass::MustReset),
    ("T2_STATS", TlsClass::MustReset),
    ("TAB_LINE_EVAL_COUNT", TlsClass::MustReset),
    ("TAGGED_HEAP", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_BARRIER_WINDOW", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_CONCURRENT_ACTIVE", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_CONCURRENT_HASH_ACTIVE", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_CONS_BARRIER_WINDOW", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_DUMP_SPAN", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_ID", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_PARTITION_ACTIVE", TlsClass::SourceOnly),
    ("TAGGED_HEAP_SATB_CACHE", TlsClass::OwnerCarried),
    ("TAGGED_HEAP_WRITE_TRACKING_MODE", TlsClass::OwnerCarried),
    ("TERMINAL_LISP_STATE", TlsClass::OwnerCarried),
    ("TERMINAL_MANAGER", TlsClass::OwnerCarried),
    ("TEST_FALLBACK_TAGGED_HEAP", TlsClass::SourceOnly),
    ("TEXT", TlsClass::CarrierIntrinsic),
    ("THREAD", TlsClass::CarrierIntrinsic),
    ("THREAD_CACHE_EPOCH", TlsClass::CarrierIntrinsic),
    ("THREAD_DATA", TlsClass::CarrierIntrinsic),
    ("THREAD_GUARD", TlsClass::CarrierIntrinsic),
    ("THREAD_ID", TlsClass::CarrierIntrinsic),
    ("THREAD_LOCAL_ALLOCATION_COUNTS", TlsClass::CarrierIntrinsic),
    ("TIME_ZONE_RULE", TlsClass::CarrierIntrinsic),
    ("TOOL_BAR_ITEMS_CACHE", TlsClass::MustReset),
    ("UNBIND_SITES", TlsClass::MustReset),
    ("UNBOUNDED_BODY", TlsClass::MustReset),
    ("UNTAG_COUNT", TlsClass::MustReset),
    ("VARREF_ALIAS", TlsClass::SourceOnly),
    ("VARREF_COUNTS", TlsClass::SourceOnly),
    ("VERIFY_MARKED_OBJECTS", TlsClass::SourceOnly),
    ("VISIBLE_SYMBOL_NAME_CACHE", TlsClass::MustReset),
    ("W32_VALID_CODEPAGES", TlsClass::SourceOnly),
    ("WINDOW_CONFIGURATION_SNAPSHOTS", TlsClass::OwnerCarried),
    (
        "WINDOW_PARAMETERS_LAYOUT_GENERATION",
        TlsClass::CarrierIntrinsic,
    ),
    ("WORKER_THREAD_STATE", TlsClass::CarrierIntrinsic),
    // CarrierIntrinsic: this is the linker-synthesized ELF TLS module base
    // (binary census only; no `thread_local!` in the workspace), the address
    // the OS thread's TLS block is reached through. It is valid for the
    // thread's whole lifetime, carries no Lisp state and cannot be reset by
    // source code.
    ("_TLS_MODULE_BASE_", TlsClass::CarrierIntrinsic),
    ("_rjem_je_tsd_tls", TlsClass::CarrierIntrinsic),
    ("{closure#0}", TlsClass::CarrierIntrinsic),
    ("{closure#1}", TlsClass::CarrierIntrinsic),
];

fn classify(name: &str) -> Option<TlsClass> {
    TLS_CLASSIFICATION
        .iter()
        .find(|(classified, _)| *classified == name)
        .map(|(_, class)| *class)
}

fn is_test_only_block(before: &str) -> bool {
    before.contains("#[cfg(test)]")
        || before.contains("cfg(all(test")
        || before.contains("cfg(any(test")
}

fn scanned_tls_names() -> Vec<(String, String)> {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory");
    let mut found = Vec::new();
    visit(crates, crates, &mut found);
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
    let missing: Vec<_> = scanned
        .iter()
        .filter(|(name, _)| classify(name).is_none())
        .map(|(name, file)| format!("{name} ({file})"))
        .collect();
    assert!(
        missing.is_empty(),
        "unclassified thread_local! statics (add a carrier TLS class): {missing:?}"
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
    (
        "neovm-core/src/emacs_core/text/string_pos_cache/mod.rs",
        "string_pos_cache",
    ),
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
];

/// `MustReset` records the source scan cannot see (they come from the binary
/// TLS census or a cfg-gated block the scan skips); each still needs an
/// explicit reset-step claim.
const MUST_RESET_EXTRA_COVERAGE: &[(&str, &str)] = &[("IMM_POOL", "jit_compile")];

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
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates directory")
        .to_path_buf()
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
    let carrier_tls = std::fs::read_to_string(
        crates.join("neovm-core/src/emacs_core/runtime/eval/green/carrier_tls.rs"),
    )
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
        if matches!(entry.1, TlsClass::MustReset) {
            must_reset_names.insert((entry.0).to_owned());
        }
    }

    let scanned = scanned_tls_names();
    let mut covered = std::collections::BTreeSet::new();
    let mut claimed_steps = std::collections::BTreeSet::new();
    let mut claimed_workspace = std::collections::BTreeSet::new();

    for (name, file) in &scanned {
        if !matches!(classify(name), Some(TlsClass::MustReset)) {
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
