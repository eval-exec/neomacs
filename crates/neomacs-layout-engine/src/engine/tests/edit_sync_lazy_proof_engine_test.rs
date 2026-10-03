//! Actual eager text/property probes, not a test-local proof model. Every arm
//! uses the existing real producer/reference-render fixture under one Context.
use super::*;
use crate::incremental_layout::lazy_proof_test_support::{self as probes, Counts, Guard};

fn observed(
    lazy: bool,
    prove_first: bool,
    mode: EditSyncMode,
    below: bool,
    text: &str,
    point: usize,
    forms: &[&str],
) -> (Vec<String>, ProveFirstCounts, Vec<LayoutStats>, Counts) {
    let _guard = Guard::set(lazy);
    assert_eq!(probes::forced(), Some(lazy));
    let (observations, producer, stats) =
        run_with_policy(prove_first, mode, below, text, point, forms);
    (observations, producer, stats, probes::counts())
}

#[test]
fn accepted_sync_skips_eager_source_proof_after_complete_output_validation() {
    let text = source();
    let forms = ["(insert \"x\")"];
    let off = observed(
        false,
        false,
        EditSyncMode::Sync,
        true,
        &text,
        line_end(&text, 8),
        &forms,
    );
    let on = observed(
        true,
        false,
        EditSyncMode::Sync,
        true,
        &text,
        line_end(&text, 8),
        &forms,
    );
    assert_eq!(on.0, off.0, "mode-line/fontifier and positional observers");
    assert!(off.2[0].edit_windows == 1 && on.2[0].edit_windows == 1);
    assert!(
        off.3.sync_admissions > 0 && on.3.sync_admissions > 0,
        "both arms must admit actual Sync: off={:?} on={:?}",
        off.3,
        on.3
    );
    assert!(
        off.3.source_proof_calls > 0 && off.3.char_queries > 0 && off.3.property_queries > 0,
        "actual eager proof control: {:?}",
        off.3
    );
    // The sealed 6726426503 test-only predecessor must reach this assertion and fail here,
    // after all complete geometry/paint/source/observer comparisons pass.
    assert_eq!(
        on.3.source_proof_calls, 0,
        "admitted Sync must skip source proof: {:?}",
        on.3
    );
    assert_eq!(on.3.char_queries, 0);
    assert_eq!(on.3.property_queries, 0);
    assert!(on.3.lazy_entries > 0);
}

#[test]
fn lazy_proof_off_prove_first_prove_and_below_disabled_keep_eager_policy() {
    let text = source();
    let forms = ["(insert \"x\")"];
    for (pf, mode, below) in [
        (true, EditSyncMode::Sync, true),
        (false, EditSyncMode::Prove, true),
        (false, EditSyncMode::Sync, false),
    ] {
        let off = observed(false, pf, mode, below, &text, line_end(&text, 8), &forms);
        let on = observed(true, pf, mode, below, &text, line_end(&text, 8), &forms);
        assert_eq!(on.0, off.0);
        assert_eq!(on.1, off.1, "preferred producer unchanged");
        assert_eq!(
            on.3, off.3,
            "ineligible knob ON must keep literal eager probes"
        );
        assert_eq!(on.3.lazy_entries, 0);
        if below {
            assert!(on.3.source_proof_calls > 0);
        } else {
            assert_eq!(on.3.source_proof_calls, 0);
        }
        assert_eq!(on.2[0].relaid_body_rows, off.2[0].relaid_body_rows);
        assert_eq!(on.2[0].reused_rows, off.2[0].reused_rows);
    }
    let off = observed(
        false,
        false,
        EditSyncMode::Sync,
        true,
        &text,
        line_end(&text, 8),
        &forms,
    );
    assert_eq!(off.3.lazy_entries, 0);
    assert!(off.3.source_proof_calls > 0);
}

#[test]
fn lazy_sync_line_start_newline_join_tab_unicode_and_width_match_full_rendering() {
    let text = source();
    let line_start = text.split_inclusive('\n').take(8).map(str::len).sum();
    let cases = [
        (line_start, "(insert \"x\")"),
        (line_end(&text, 8) - 3, "(insert \"\\n\")"),
        (line_end(&text, 8), "(delete-region (point) (1+ (point)))"),
        (line_end(&text, 8) - 3, "(insert \"\\t\")"),
        (line_end(&text, 8) - 3, "(insert \"中\")"),
        (line_end(&text, 8) - 3, "(insert (make-string 100 ?w))"),
    ];
    for (point, form) in cases {
        let off = observed(
            false,
            false,
            EditSyncMode::Sync,
            true,
            &text,
            point,
            &[form],
        );
        let on = observed(true, false, EditSyncMode::Sync, true, &text, point, &[form]);
        assert_eq!(on.0, off.0, "{form}");
        assert_eq!(
            on.2[0].relaid_body_rows, off.2[0].relaid_body_rows,
            "{form}"
        );
        assert_eq!(on.2[0].reused_rows, off.2[0].reused_rows, "{form}");
    }
}

#[test]
fn lazy_sync_structure_and_unmarking_fontifiers_keep_complete_source_observers() {
    let text = source();
    let cases = [
        "(put-text-property (- (point) 4) (point) 'display \"<REPLACED>\")",
        "(put-text-property (- (point) 4) (point) 'invisible t)",
        "(put-text-property (- (point) 4) (point) 'line-prefix \"P:\")",
        "(progn (setq fontification-functions (list (lambda (_p) (setq prove-first-font-count (1+ prove-first-font-count))))) (delete-region (point) (1+ (point))))",
    ];
    for form in cases {
        let off = observed(
            false,
            false,
            EditSyncMode::Sync,
            true,
            &text,
            line_end(&text, 8),
            &[form],
        );
        let on = observed(
            true,
            false,
            EditSyncMode::Sync,
            true,
            &text,
            line_end(&text, 8),
            &[form],
        );
        assert_eq!(on.0, off.0, "callback/positional observers: {form}");
        assert_eq!(on.2[0].edit_windows, off.2[0].edit_windows);
        if form.contains("fontification-functions") {
            assert!(
                on.0[0]
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .parse::<i64>()
                    .unwrap()
                    > 0,
                "fontifier must run: {:?}",
                on.0
            );
        }
    }
}

#[test]
fn lazy_proof_numeric_override_restores_nested_and_unwound_scopes() {
    let initial = probes::forced();
    let initial_counts = probes::counts();
    {
        let _outer = Guard::set(false);
        probes::note_source_proof();
        let outer_counts = probes::counts();
        let unwind = std::panic::catch_unwind(|| {
            let _inner = Guard::set(true);
            probes::note_char_query();
            assert_eq!(probes::forced(), Some(true));
            panic!("numeric scope unwind control");
        });
        assert!(unwind.is_err());
        assert_eq!(probes::forced(), Some(false));
        assert_eq!(probes::counts(), outer_counts);
    }
    assert_eq!(probes::forced(), initial);
    assert_eq!(probes::counts(), initial_counts);
}

#[cfg(test)]
#[path = "edit_sync_lazy_proof_edge_engine_test.rs"]
mod edge_controls;
