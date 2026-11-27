//! Timing-assertion inventory ratchet for `crates/neomacs-gui-tests`.
//!
//! Complements `src/wall_clock_guard_test.rs` (W1–W5):
//! - `wall_clock_guard_test.rs` floors wait caps (`HangCap`, `NegativeSoak`),
//!   denies unallowlisted clock identifiers, and pins fixed sleeps (`SLEEP_PINS`).
//! - This ratchet governs **what tests assert**, enforcing exact pins across:
//!   1. **Tier A1 (latency / cadence / clock-uncertainty assertions):** calibrated
//!      and hardware-only measurements (`continuous_120hz_*`, `x11_presentation`).
//!   2. **Tier A2 (promptness / exit-within-budget assertions):** pinned at `0`
//!      after `issue_417_startup` was converted to `HangCap` (`a92a33ed1a`), so
//!      a non-monotonic or short exit-budget assertion cannot return silently.
//!   3. **Tier B (indirect load-sensitive / capture-latency / rate-coupled rows):**
//!      rows whose assertions can fail from host load without calling a Rust clock
//!      directly (missed by W1–W5 because coupling lives in capture spacing or
//!      Elisp fixture timers).
//!   4. **Audit doc sync:** every pinned and converted-by-guard row must be
//!      documented in `docs/gui-timing-assertions-audit.md`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use proc_macro2::TokenTree;
use syn::visit::{self, Visit};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TimingAssertionPin {
    tier_a_latency_and_cadence: usize,
    tier_a_promptness_budget: usize,
    tier_b_load_sensitive: usize,
}

/// Exact equality pins (same shrink-only / reviewed-diff shape as `SLEEP_PINS`).
///
/// - A new timing assertion without a classified allowlist entry and bumped pin fails.
/// - Reintroducing any Tier A2 promptness/exit-budget assertion fails (`0` pinned).
/// - Converting or removing a pinned row without lowering its pin fails.
const TIMING_ASSERTION_PINS: TimingAssertionPin = TimingAssertionPin {
    tier_a_latency_and_cadence: 3,
    tier_a_promptness_budget: 0,
    tier_b_load_sensitive: 4,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Disposition {
    CalibratedByDesign,
    HardwareOnly,
    ConvertedByGuard,
    OpenTierB,
}

#[derive(Debug, Clone, Copy)]
struct AuditedRow {
    file: &'static str,
    test_fn: &'static str,
    disposition: Disposition,
    /// Substring expected in `file` proving the row's load-sensitive coupling
    /// still exists (used on Tier B rows so a conversion forces a pin shrink).
    coupling_marker: Option<&'static str>,
}

/// Active Tier A1 rows (calibrated measurements & hardware-only timing gates).
const TIER_A_LATENCY_ROWS: &[AuditedRow] = &[
    AuditedRow {
        file: "tests/native_scrolling.rs",
        test_fn: "continuous_120hz_scroll_in_large_plain_buffer",
        disposition: Disposition::CalibratedByDesign,
        coupling_marker: None,
    },
    AuditedRow {
        file: "tests/native_scrolling.rs",
        test_fn: "continuous_120hz_scroll_in_large_rich_buffer",
        disposition: Disposition::CalibratedByDesign,
        coupling_marker: None,
    },
    AuditedRow {
        file: "tests/x11_presentation.rs",
        test_fn: "x11_rich_scroll_inputs_have_native_output_receipts_across_resize",
        disposition: Disposition::HardwareOnly,
        coupling_marker: None,
    },
];

/// Tier A2 rows already converted to `HangCap`; retained in the audit doc and
/// verified free of promptness-budget assertions.
const TIER_A_CONVERTED_ROWS: &[AuditedRow] = &[AuditedRow {
    file: "tests/issue_417_startup.rs",
    test_fn: "invalid_initial_frame_exits_promptly_without_a_gui_frame",
    disposition: Disposition::ConvertedByGuard,
    coupling_marker: None,
}];

/// Active Tier B rows (indirectly coupled to wall-clock timers, capture
/// cadence, or frame rate without calling a Rust clock inside `assert!`).
const TIER_B_LOAD_SENSITIVE_ROWS: &[AuditedRow] = &[
    AuditedRow {
        file: "tests/child_frame_animation.rs",
        test_fn: "child_frame_fades_progress_across_presented_frames",
        disposition: Disposition::OpenTierB,
        coupling_marker: Some("fading_strip[0] > full_strip + 0.05"),
    },
    AuditedRow {
        file: "tests/agent_shell_source.rs",
        test_fn: "melpa_agent_shell_resume_paints_without_false_source_mutation_like_gnu",
        disposition: Disposition::OpenTierB,
        coupling_marker: Some("Some(\"1.5\" | \"3\" | \"3.0\")"),
    },
    AuditedRow {
        file: "tests/real_gui_smoke/line_navigation.rs",
        test_fn: "timer_line_navigation_reuses_scene_during_cursor_motion",
        disposition: Disposition::OpenTierB,
        coupling_marker: Some("count(\"composite_only_frames\") > commits"),
    },
    AuditedRow {
        file: "tests/presentation_pacing_receipts.rs",
        test_fn: "wayland_gpu_only_cursor_motion_presents_paced_receipts_without_unpaced_storm",
        disposition: Disposition::OpenTierB,
        coupling_marker: Some("p2_composite_only >= 3"),
    },
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AssertionAllowance {
    file: &'static str,
    scope: &'static str,
    count: usize,
}

/// Exact `(file, fn_scope, assertion_count)` allowlist for live Tier A1
/// latency / cadence / clock-uncertainty assertions found by AST inspection.
const TIER_A_LATENCY_SITES: &[AssertionAllowance] = &[
    AssertionAllowance {
        file: "tests/native_scrolling/continuous.rs",
        scope: "run",
        count: 1,
    },
    AssertionAllowance {
        file: "tests/x11_presentation.rs",
        scope: "x11_rich_scroll_inputs_have_native_output_receipts_across_resize",
        count: 4,
    },
];

/// Exact `(file, fn_scope, assertion_count)` allowlist for Tier A2 promptness /
/// exit-within-budget assertions. Pinned empty (`0`): never reintroduce.
const TIER_A_PROMPTNESS_SITES: &[AssertionAllowance] = &[];

/// Exact `(file, fn_scope, assertion_count)` allowlist for Tier B load-sensitive
/// assertion expressions detected in `tests/**/*.rs`.
const TIER_B_SIGNAL_SITES: &[AssertionAllowance] = &[
    AssertionAllowance {
        file: "tests/child_frame_animation.rs",
        scope: "child_frame_fades_progress_across_presented_frames",
        count: 9,
    },
    AssertionAllowance {
        file: "tests/real_gui_smoke/line_navigation.rs",
        scope: "run_navigation",
        count: 1,
    },
];

/// Offline pure-data unit test files and guard self-tests excluded from live
/// GUI timing-assertion site counts.
const OFFLINE_BUDGET_UNIT_FILES: &[&str] = &[
    "src/wall_clock_guard_test.rs",
    "tests/native_scrolling/budget_tests.rs",
    "tests/native_scrolling/budget_regression_tests.rs",
    "tests/native_scrolling/latency.rs",
    "tests/timing_assertion_ratchet.rs",
];

const LATENCY_ASSERT_IDENTS: &[&str] = &[
    "WithinBudget",
    "ExceededBudget",
    "meets_response_budget",
    "assess",
    "cold_page_ns",
    "p95_ns",
    "max_ns",
    "p50_ns",
    "p99_ns",
    "p95_ms",
    "max_ms",
    "latency_ms",
    "latency_ns",
    "frame_time",
    "frames_per_sec",
    "uncertainty_ns",
    "timestamp_uncertainty_ns",
];

const LATENCY_ASSERT_LITERALS: &[&str] = &[
    "cold_page_ns",
    "p95_ns",
    "max_ns",
    "p50_ns",
    "p99_ns",
    ":uncertainty-ns",
    "timestamp_uncertainty_ns",
    "exceeds 250 ms",
    "exceeds p95",
];

const PROMPTNESS_ASSERT_IDENTS: &[&str] = &[
    "elapsed",
    "duration_since",
    "checked_duration_since",
    "saturating_duration_since",
    "as_millis",
    "as_micros",
    "as_nanos",
    "as_secs",
    "as_secs_f32",
    "as_secs_f64",
    "EXIT_BUDGET",
    "PROMPT_BUDGET",
    "LATENCY_BUDGET",
    "SPEED_BUDGET",
];

const TIER_B_ASSERT_IDENTS: &[&str] = &[
    "opening_strip",
    "fading_strip",
    "resized_strip",
    "fading",
    "cursor_motion",
];

const TIER_B_ASSERT_LITERALS: &[&str] = &["composite_only_frames"];

#[derive(Debug, Default)]
struct FileScanResult {
    latency_sites: BTreeMap<String, usize>,
    promptness_sites: BTreeMap<String, usize>,
    tier_b_sites: BTreeMap<String, usize>,
    test_fns: BTreeSet<String>,
}

struct AssertionScanner {
    current_scope: String,
    in_cfg_test_mod: bool,
    result: FileScanResult,
}

impl AssertionScanner {
    fn new() -> Self {
        Self {
            current_scope: "<items>".to_owned(),
            in_cfg_test_mod: false,
            result: FileScanResult::default(),
        }
    }

    fn inspect_macro(&mut self, mac: &syn::Macro) {
        if self.in_cfg_test_mod {
            return;
        }
        let Some(name) = mac.path.segments.last().map(|seg| seg.ident.to_string()) else {
            return;
        };
        if !matches!(name.as_str(), "assert" | "assert_eq" | "assert_ne") {
            return;
        }
        let mut idents = Vec::new();
        let mut literals = Vec::new();
        collect_tokens(mac.tokens.clone(), &mut idents, &mut literals);

        let is_latency = idents
            .iter()
            .any(|id| LATENCY_ASSERT_IDENTS.contains(&id.as_str()))
            || literals
                .iter()
                .any(|lit| LATENCY_ASSERT_LITERALS.iter().any(|pat| lit.contains(pat)));
        if is_latency {
            *self
                .result
                .latency_sites
                .entry(self.current_scope.clone())
                .or_insert(0) += 1;
        }

        let is_promptness = idents
            .iter()
            .any(|id| PROMPTNESS_ASSERT_IDENTS.contains(&id.as_str()));
        if is_promptness {
            *self
                .result
                .promptness_sites
                .entry(self.current_scope.clone())
                .or_insert(0) += 1;
        }

        let is_tier_b = idents
            .iter()
            .any(|id| TIER_B_ASSERT_IDENTS.contains(&id.as_str()))
            || literals
                .iter()
                .any(|lit| TIER_B_ASSERT_LITERALS.iter().any(|pat| lit.contains(pat)));
        if is_tier_b {
            *self
                .result
                .tier_b_sites
                .entry(self.current_scope.clone())
                .or_insert(0) += 1;
        }
    }
}

fn collect_tokens(
    stream: proc_macro2::TokenStream,
    idents: &mut Vec<String>,
    literals: &mut Vec<String>,
) {
    for tree in stream {
        match tree {
            TokenTree::Group(group) => collect_tokens(group.stream(), idents, literals),
            TokenTree::Ident(ident) => idents.push(ident.to_string()),
            TokenTree::Literal(literal) => literals.push(literal.to_string()),
            TokenTree::Punct(_) => {}
        }
    }
}

fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path()
            .segments
            .last()
            .is_some_and(|seg| seg.ident == "test")
    })
}

fn is_cfg_test_mod(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|attr| {
        attr.path().is_ident("cfg")
            && attr
                .meta
                .require_list()
                .ok()
                .is_some_and(|list| list.tokens.to_string().contains("test"))
    })
}

impl<'ast> Visit<'ast> for AssertionScanner {
    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        let prev = self.in_cfg_test_mod;
        if is_cfg_test_mod(&item.attrs) {
            self.in_cfg_test_mod = true;
        }
        visit::visit_item_mod(self, item);
        self.in_cfg_test_mod = prev;
    }

    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        let name = item.sig.ident.to_string();
        if has_test_attr(&item.attrs) {
            self.result.test_fns.insert(name.clone());
        }
        let prev = std::mem::replace(&mut self.current_scope, name);
        visit::visit_item_fn(self, item);
        self.current_scope = prev;
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        let name = item.sig.ident.to_string();
        let prev = std::mem::replace(&mut self.current_scope, name);
        visit::visit_impl_item_fn(self, item);
        self.current_scope = prev;
    }

    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        self.inspect_macro(mac);
        visit::visit_macro(self, mac);
    }
}

fn scan_rust_source(source: &str) -> Result<FileScanResult, String> {
    let parsed = syn::parse_file(source).map_err(|err| format!("syn parse failed: {err}"))?;
    let mut scanner = AssertionScanner::new();
    scanner.visit_file(&parsed);
    Ok(scanner.result)
}

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("read {}: {err}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

fn check_site_allowlist(
    label: &str,
    observed: &BTreeMap<(String, String), usize>,
    allowlist: &[AssertionAllowance],
) -> Vec<String> {
    let mut expected = BTreeMap::new();
    for entry in allowlist {
        let prev = expected.insert((entry.file.to_owned(), entry.scope.to_owned()), entry.count);
        assert!(
            prev.is_none(),
            "duplicate {label} allowlist entry: {entry:?}"
        );
    }
    let mut errors = Vec::new();
    for (key, &count) in observed {
        match expected.get(key) {
            None => errors.push(format!(
                "[{label}] unlisted timing assertion(s) in {}::{} (count={count})",
                key.0, key.1
            )),
            Some(&allowed) if allowed != count => errors.push(format!(
                "[{label}] count drift in {}::{}: observed={count}, allowlisted={allowed}",
                key.0, key.1
            )),
            Some(_) => {}
        }
    }
    for (key, &allowed) in &expected {
        if !observed.contains_key(key) {
            errors.push(format!(
                "[{label}] stale allowlist entry {}::{} (expected count={allowed}, observed=0); shrink the allowlist and pin",
                key.0, key.1
            ));
        }
    }
    errors
}

fn verify_pins(
    pin: TimingAssertionPin,
    latency_rows: usize,
    promptness_sites: usize,
    tier_b_rows: usize,
) -> Result<(), String> {
    if latency_rows != pin.tier_a_latency_and_cadence {
        return Err(format!(
            "Tier A1 latency/cadence row pin mismatch: observed={latency_rows}, pinned={}",
            pin.tier_a_latency_and_cadence
        ));
    }
    if promptness_sites != pin.tier_a_promptness_budget {
        return Err(format!(
            "Tier A2 promptness/exit-budget pin mismatch: observed={promptness_sites}, pinned={} (must stay 0)",
            pin.tier_a_promptness_budget
        ));
    }
    if tier_b_rows != pin.tier_b_load_sensitive {
        return Err(format!(
            "Tier B load-sensitive row pin mismatch: observed={tier_b_rows}, pinned={}",
            pin.tier_b_load_sensitive
        ));
    }
    Ok(())
}

#[test]
fn gui_timing_assertions_match_pinned_inventory_and_audit_doc() {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for directory in ["tests", "src"] {
        collect_rs_files(&crate_root.join(directory), &mut files);
    }
    assert!(
        !files.is_empty(),
        "must discover GUI test and harness files"
    );

    let mut all_latency = BTreeMap::new();
    let mut all_promptness = BTreeMap::new();
    let mut all_tier_b = BTreeMap::new();
    let mut test_fns_by_file: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut source_by_file: BTreeMap<String, String> = BTreeMap::new();

    for path in files {
        let rel = path
            .strip_prefix(&crate_root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let source = fs::read_to_string(&path).unwrap_or_else(|err| panic!("read {rel}: {err}"));
        let scanned = scan_rust_source(&source).unwrap_or_else(|err| panic!("{rel}: {err}"));
        test_fns_by_file.insert(rel.clone(), scanned.test_fns);
        source_by_file.insert(rel.clone(), source);

        if OFFLINE_BUDGET_UNIT_FILES.contains(&rel.as_str()) {
            continue;
        }
        for (scope, count) in scanned.latency_sites {
            all_latency.insert((rel.clone(), scope), count);
        }
        for (scope, count) in scanned.promptness_sites {
            all_promptness.insert((rel.clone(), scope), count);
        }
        for (scope, count) in scanned.tier_b_sites {
            all_tier_b.insert((rel.clone(), scope), count);
        }
    }

    let mut violations = Vec::new();
    violations.extend(check_site_allowlist(
        "Tier-A1-Latency",
        &all_latency,
        TIER_A_LATENCY_SITES,
    ));
    violations.extend(check_site_allowlist(
        "Tier-A2-Promptness",
        &all_promptness,
        TIER_A_PROMPTNESS_SITES,
    ));
    violations.extend(check_site_allowlist(
        "Tier-B-LoadSensitive",
        &all_tier_b,
        TIER_B_SIGNAL_SITES,
    ));

    // Verify every audited row exists in its file, matches its disposition
    // marker, and is documented in docs/gui-timing-assertions-audit.md.
    let audit_doc_path = crate_root
        .join("../..")
        .join("docs/gui-timing-assertions-audit.md");
    let audit_doc = fs::read_to_string(&audit_doc_path)
        .unwrap_or_else(|err| panic!("read {}: {err}", audit_doc_path.display()));

    for row in TIER_A_LATENCY_ROWS
        .iter()
        .chain(TIER_A_CONVERTED_ROWS)
        .chain(TIER_B_LOAD_SENSITIVE_ROWS)
    {
        let found = test_fns_by_file
            .get(row.file)
            .is_some_and(|fns| fns.contains(row.test_fn));
        if !found {
            violations.push(format!(
                "audited row {}::{} ({:?}) no longer exists; update inventory and pins",
                row.file, row.test_fn, row.disposition
            ));
        }
        if let Some(marker) = row.coupling_marker {
            let still_present = source_by_file
                .get(row.file)
                .is_some_and(|src| src.contains(marker));
            if !still_present {
                violations.push(format!(
                    "Tier B row {}::{} no longer contains coupling marker {marker:?}; shrink TIMING_ASSERTION_PINS.tier_b_load_sensitive and update audit doc",
                    row.file, row.test_fn
                ));
            }
        }
        if !audit_doc.contains(row.test_fn) {
            violations.push(format!(
                "audited row {}::{} missing from docs/gui-timing-assertions-audit.md",
                row.file, row.test_fn
            ));
        }
    }

    if let Err(err) = verify_pins(
        TIMING_ASSERTION_PINS,
        TIER_A_LATENCY_ROWS.len(),
        all_promptness.values().sum(),
        TIER_B_LOAD_SENSITIVE_ROWS.len(),
    ) {
        violations.push(err);
    }

    assert!(
        violations.is_empty(),
        "GUI timing-assertion ratchet violations:\n- {}",
        violations.join("\n- ")
    );
}

#[test]
fn ratchet_rejects_reintroduced_promptness_assertion_like_old_issue_417() {
    let old_issue_417 = r#"
        fn run_invalid_initial_frame_with_prompt_exit() {
            assert!(published.elapsed().unwrap() < EXIT_BUDGET, "did not terminate");
            assert!(finished.duration_since(published).unwrap() <= EXIT_BUDGET, "not prompt");
        }
    "#;
    let scanned = scan_rust_source(old_issue_417).expect("parse synthetic promptness test");
    assert_eq!(
        scanned
            .promptness_sites
            .get("run_invalid_initial_frame_with_prompt_exit")
            .copied(),
        Some(2),
        "both old issue_417 promptness assertions must be caught by Tier A2"
    );
    let mut observed = BTreeMap::new();
    observed.insert(
        (
            "tests/issue_417_startup.rs".to_owned(),
            "run_invalid_initial_frame_with_prompt_exit".to_owned(),
        ),
        2,
    );
    let errors = check_site_allowlist("Tier-A2-Promptness", &observed, TIER_A_PROMPTNESS_SITES);
    assert_eq!(errors.len(), 1, "unlisted promptness assertion must fail");
    assert!(
        errors[0].contains("unlisted timing assertion(s)"),
        "{errors:?}"
    );
}

#[test]
fn ratchet_rejects_unlisted_latency_or_uncertainty_assertion() {
    let sample = r#"
        #[test]
        fn sneaks_in_latency_bound() {
            assert!(stats["p95_ns"].as_u64().unwrap() <= 50_000_000);
            assert!(receipt_number(&r, ":uncertainty-ns ").unwrap() <= 1_000_000);
        }
    "#;
    let scanned = scan_rust_source(sample).expect("parse synthetic latency test");
    assert_eq!(
        scanned
            .latency_sites
            .get("sneaks_in_latency_bound")
            .copied(),
        Some(2),
        "both latency and clock-uncertainty assertions must be caught by Tier A1"
    );
}

#[test]
fn ratchet_rejects_pin_drift_in_either_direction() {
    assert!(verify_pins(TIMING_ASSERTION_PINS, 3, 0, 4).is_ok());
    assert!(verify_pins(TIMING_ASSERTION_PINS, 4, 0, 4).is_err());
    assert!(verify_pins(TIMING_ASSERTION_PINS, 2, 0, 4).is_err());
    assert!(verify_pins(TIMING_ASSERTION_PINS, 3, 1, 4).is_err());
    assert!(verify_pins(TIMING_ASSERTION_PINS, 3, 0, 5).is_err());
    assert!(verify_pins(TIMING_ASSERTION_PINS, 3, 0, 3).is_err());
}
