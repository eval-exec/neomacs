//! Which operator environment variables a benchmark editor inherits:
//! the explicit knob list plus the `NEOVM_JIT_` prefix. Kept out of the
//! harness engine so new runtime knobs do not grow `harness.rs`.

const BENCHMARK_PASSTHROUGH_ENVIRONMENT: &[&str] = &[
    "PATH",
    "LD_LIBRARY_PATH",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    // Preserve Vulkan loader discovery on hosts whose drivers live outside
    // the default search paths (e.g. NixOS). Dropping these can silently
    // benchmark software rendering instead of the session's GPU driver.
    "VK_DRIVER_FILES",
    "VK_ICD_FILENAMES",
    "VK_ADD_DRIVER_FILES",
    "GST_PLUGIN_SYSTEM_PATH_1_0",
    "GST_PLUGIN_SCANNER_1_0",
    "SYSTEMROOT",
    "WINDIR",
    // The master JIT switch has no trailing underscore, so it is not covered
    // by the diagnostic prefix below. Record and forward interpreter controls.
    "NEOVM_JIT",
    // Same-binary AOT admission and re-tier experiments need these exact
    // runtime knobs. Producer and alternate-store controls remain isolated.
    "NEOVM_AOT",
    "NEOVM_AOT_RETIER",
    "NEOVM_AOT_PREWARM",
    // Measurement knobs the fixtures read. The environment is cleared before
    // the editor runs, so a knob absent from this list is silently ignored --
    // the experiment then compares a binary against itself and reads as "no
    // effect" rather than as a mistake.
    "NEOMACS_PERF_RELEASE_STARTUP_GC_CEILING",
    // Per-frame layout telemetry (relaid rows, fast-path classification), for
    // attributing an input-latency tail to relayout rather than guessing.
    "NEOMACS_LAYOUT_STATS_FILE",
    // Per-keystroke latency decomposition: wall, CPU, and collection work.
    // Percentiles cannot say whether a slow keystroke was computing or
    // waiting, and those call for opposite work.
    "NEOMACS_PERF_LATENCY_TRACE_FILE",
    // GC pacing sweep: the live-proportional term as a percentage of the live
    // heap. 0 leaves GNU's `gc-cons-threshold`/`gc-cons-percentage` contract
    // exactly; the built-in default is 50.
    "NEOVM_GC_LIVE_GROWTH_PERCENT",
    // OSR can reject a hot loop before compilation starts, so the JIT's
    // compilation census alone cannot explain why that loop stays interpreted.
    // Capture the opt-in rejection trace in the editor's stderr artifact.
    "NEOMACS_OSR_DEBUG",
    // P3.5 redisplay knobs (same-binary A/B; explicit off retains baseline arms).
    // Buffer-text snapshots: `copy` or `share` (default, copy-on-write).
    "NEOMACS_TEXT_SNAPSHOT",
    // TTY silent frames: `off` (default) or `on`.
    "NEOMACS_TTY_SILENT",
    // TTY damage-proportional repaint: `off`, `verify` or `on` (default), and
    // the per-frame verify report.
    "NEOMACS_TTY_DAMAGE",
    "NEOMACS_TTY_DAMAGE_REPORT_FILE",
    // What names a window row in a TTY painter key: `address` or
    // `appearance` (default; position-only copies keep their row).
    "NEOMACS_TTY_ROW_IDENTITY",
    // Chrome string positions from chrome rows: `frame` or `rows` (default).
    "NEOMACS_PRESENT_CHROME_POS",
    // Text hit positions built while composing (`eager`) or on the
    // first pointer query (`lazy`, default).
    "NEOMACS_PRESENT_HIT",
    // C6 compact row point storage and per-window row hits: off, on (default), verify.
    "NEOMACS_PRESENT_POINT_ROWS",
    // Certified row-stream concatenation: off, on (default); overlaps keep heap merge.
    "NEOMACS_POINT_ROW_ITER",
    // Deferred numeric presentation positions: on (default), off; same-binary arms.
    "NEOMACS_PRESENT_GEOMETRY_LAZY",
    // Source newline counting: off, on (default), verify; indexed counts unchanged.
    "NEOMACS_LAYOUT_LINE_COUNT",
    // The mini-window stands still when what it shows is unchanged: `on`.
    "NEOMACS_LAYOUT_MINI_STILL",
    // The visible automatic-composition scan's ASCII fast path and memo: `on`.
    "NEOMACS_COMPOSITION_FASTPATH",
    // P3.5 U3.7. An edit frame's mode line by GNU's optimization-1 guard:
    // `legacy` (default) or `gnu`.
    "NEOMACS_MODE_LINE_GATE",
    // Edit replays synchronize with the rows below the edit (GNU
    // try_window_id): `prove` (default) or `sync`.
    "NEOMACS_LAYOUT_EDIT_SYNC",
    // A window whose start moved back reuses its old rows: `on`.
    "NEOMACS_LAYOUT_SCROLL_BACK",
    // posn-at-point & co. read only the text the window shows: `on`.
    "NEOMACS_POSN_BOUNDED_TEXT",
    // `(redisplay t)` skips the layout when nothing visible changed: `on`.
    "NEOMACS_REDISPLAY_IDLE_SKIP",
    // An evaluated mode line that renders the same reuses its row: `on|verify`.
    "NEOMACS_CHROME_MEMO",
    // U2.8 builtin front-end diets: `=off` restores the general search,
    // syntax and text-property paths for a same-binary board A/B.
    "NEOVM_BUILTIN_FRONTEND",
    // P1.4 Stage A cached variable tiers: `=0` restores the general
    // read/set/bind/unbind paths.
    "NEOVM_VAR_CACHE",
    // CL2 performance-cliff controls; record the exact editor environment in
    // input-provenance.json for same-binary comparisons.
    "NEOVM_COMPARE_STRINGS_POS_CACHE",
    "NEOVM_REGEX_SHORT_LITERAL",
    "NEOVM_EMACS_MULE_PREPARED",
    "NEOVM_OVERLAY_LOCAL_MOVE",
    // P1.2 / P1.0 §3.10: Tier-0 `Bcall` of leaf builtins (`=on`).
    "NEOVM_VM_LEAF",
    // P3.3 regex knobs: alternation anchors (`=on`) and the existence DFA
    // (`=on`/`=verify`), plus its exit statistics.
    "NEOVM_REGEX_ANCHOR_ALT",
    "NEOVM_REGEX_DFA",
    "NEOVM_REGEX_DFA_STATS",
    // Folded two-character suffix search and GNU sort predicate capture.
    "NEOVM_REGEX_SUFFIX_LITERAL",
    "NEOVM_SORT_CAPTURE",
    // P4.1 Stage 0 cconv memo (`off`/`stats`/`on`/`verify`) and the native
    // no-lexvars closure path (`on`).
    "NEOVM_CCONV_MEMO",
    "NEOVM_CCONV_FAST",
    // P3.4 text line index: `on` (default), `off` or `verify`, its size
    // thresholds, and the exit report of builds, copies and served queries.
    "NEOVM_TEXT_LINE_INDEX",
    "NEOVM_TEXT_LINE_INDEX_MIN_BYTES",
    "NEOVM_TEXT_LINE_INDEX_CHUNK",
    "NEOVM_TEXT_LINE_INDEX_QUERY_BYTES",
    "NEOVM_TEXT_LINE_INDEX_QUERY_LINES",
    "NEOVM_TEXT_LINE_INDEX_BUILD_LINES",
    "NEOVM_TEXT_LINE_INDEX_STATS",
    // U0.7: `parse-partial-sexp` runs `syntax-propertize` like GNU (on);
    // `=0` never propertizes, to attribute the parity fix's cost.
    "NEOVM_PPS_PROPERTIZE",
    // P3.4 syntax cache: L1 (`1` default, `0`, `verify`), L2 (`0` default,
    // `1`, `verify`), the shared geometry and counters file.
    "NEOVM_SYNTAX_PARSE_CACHE",
    "NEOVM_SYNTAX_PARSE_CACHE_L2",
    "NEOVM_SYNTAX_PARSE_CACHE_CHUNK",
    "NEOVM_SYNTAX_PARSE_CACHE_MIN_SPAN",
    "NEOVM_SYNTAX_PARSE_CACHE_STATS",
    // Collector knobs (same-binary A/B; each defaults to the old path): the
    // chunk map (`=1`), falsifier F-G's vector deferral (`=defer`), and the
    // generation census (`=1`), whose records go to the census file.
    "NEOVM_GC_CHUNK_MAP",
    "NEOVM_GC_VEC_SCAN",
    "NEOVM_GC_CENSUS",
    "NEOVM_GC_CENSUS_REMSET",
    "NEOVM_GC_CENSUS_FILE",
];

/// Operator-set JIT diagnostic knobs (`NEOVM_JIT_PROFILE`, `NEOVM_JIT_THRESHOLD`,
/// `NEOVM_JIT_COMPILE_STATS`, ...) reach the editor too: a census of what the
/// JIT compiles or rejects under a real scenario needs them, and an unset knob
/// forwards nothing, so a plain benchmark run is unchanged.
const BENCHMARK_PASSTHROUGH_PREFIX: &str = "NEOVM_JIT_";

pub(crate) fn benchmark_passthrough_environment() -> Vec<(String, std::ffi::OsString)> {
    passthrough_from(std::env::vars_os())
}

/// [`benchmark_passthrough_environment`] over an explicit environment (testable).
pub(crate) fn passthrough_from(
    vars: impl IntoIterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(String, std::ffi::OsString)> {
    vars.into_iter()
        .filter_map(|(name, value)| {
            let name = name.into_string().ok()?;
            let forwarded = BENCHMARK_PASSTHROUGH_ENVIRONMENT.contains(&name.as_str())
                || name.starts_with(BENCHMARK_PASSTHROUGH_PREFIX);
            forwarded.then_some((name, value))
        })
        .collect()
}
