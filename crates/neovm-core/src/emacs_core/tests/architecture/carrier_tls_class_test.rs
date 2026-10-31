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
    ("CCL_VARIABLES", TlsClass::MustReset),
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
    ("_TLS_MODULE_BASE_", TlsClass::MustReset),
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
