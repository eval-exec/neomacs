//! Existing real renderer/complete reference consumers under the new numeric
//! policy. Only thread-owned numeric test overrides live across each attempt.
use super::*;
use crate::incremental_layout::edit_sync::dense_index_test_support::{
    self as dense, Guard as DenseGuard,
};

fn observed_dense(
    on: bool,
) -> (
    FrameObservation,
    String,
    LayoutStats,
    sync_probes::Counts,
    dense::Counts,
) {
    let _guard = DenseGuard::set(on);
    let observed = observed_sync(true);
    (
        observed.0,
        observed.1,
        observed.2,
        observed.4,
        dense::counts(),
    )
}

#[test]
fn accepted_sync_elides_numeric_hashes_after_complete_output_and_callbacks() {
    let off = observed_dense(false);
    let on = observed_dense(true);
    assert_eq!(
        on.0, off.0,
        "complete OFF/ON frame, faces, points, source/pointer metadata and geometry"
    );
    assert_eq!(on.1, off.1, "mode-line/fontifier and positional observers");
    assert_eq!(off.2.edit_windows, 1);
    assert_eq!(on.2.edit_windows, 1);
    assert!(off.2.reused_rows > 0 && on.2.reused_rows > 0);
    assert!(
        off.3.sync_admissions > 0 && on.3.sync_admissions > 0,
        "actual accepted Sync must reach both producers"
    );
    assert!(
        off.4.plan_insertions > 0 && off.4.install_insertions > 0,
        "actual original hash construction sites: {:?}",
        off.4
    );
    // Counter RED is meaningful only after every full/reference and callback
    // assertion above passes; compilation/setup/earlier assertion failures are
    // infrastructure/fixture failures, never claimed semantic or cost REDs.
    assert_eq!(
        on.4.plan_insertions + on.4.install_insertions,
        0,
        "accepted Sync constructed numeric hash indexes: {:?}",
        on.4
    );
    assert!(on.4.dense_plans > 0 && on.4.dense_installs > 0);
}

#[test]
fn dense_index_keeps_newline_wrap_callbacks_and_source_horizon_outputs() {
    let text = source();
    let cases = [
        (line_end(&text, 8) - 3, "(insert \"\\n\")"),
        (line_end(&text, 8), "(delete-region (point) (1+ (point)))"),
        (line_end(&text, 8) - 3, "(insert (make-string 100 ?w))"),
        (
            line_end(&text, 8),
            "(put-text-property (- (point) 4) (point) 'display \"<REPLACED>\")",
        ),
        (
            line_end(&text, 8),
            "(put-text-property (- (point) 4) (point) 'invisible t)",
        ),
        (
            line_end(&text, 8),
            "(put-text-property (- (point) 4) (point) 'line-prefix \"P:\")",
        ),
        (
            line_end(&text, 8),
            "(progn (setq fontification-functions (list (lambda (_p) (setq prove-first-font-count (1+ prove-first-font-count))))) (delete-region (point) (1+ (point))))",
        ),
    ];
    for (point, form) in cases {
        let run_dense = |on| {
            let _dense = DenseGuard::set(on);
            let _lazy = SyncGuard::set(true);
            let _budget = SourceBudgetGuard::set(true);
            let _keys = Guard::set(true);
            use crate::buffer_source::window_source::{
                reset_sync_source_budget_horizon_reads_for_test,
                reset_sync_source_budget_retries_for_test,
                sync_source_budget_horizon_reads_for_test, sync_source_budget_retries_for_test,
            };
            reset_sync_source_budget_retries_for_test();
            reset_sync_source_budget_horizon_reads_for_test();
            let observed = run_with_policy(false, EditSyncMode::Sync, true, &text, point, &[form]);
            (
                observed,
                sync_source_budget_retries_for_test(),
                sync_source_budget_horizon_reads_for_test(),
            )
        };
        let (off, off_retries, off_reads) = run_dense(false);
        let (on, on_retries, on_reads) = run_dense(true);
        assert_eq!(on.0, off.0, "callbacks/source/positional observers: {form}");
        assert_eq!(on.1, off.1, "producer decisions: {form}");
        assert_eq!(on.2[0].edit_windows, off.2[0].edit_windows, "{form}");
        assert_eq!(
            on.2[0].relaid_body_rows, off.2[0].relaid_body_rows,
            "{form}"
        );
        assert_eq!(on.2[0].reused_rows, off.2[0].reused_rows, "{form}");
        assert_eq!(on_retries, off_retries, "full horizon retries: {form}");
        assert_eq!(on_reads, off_reads, "actual horizon source copies: {form}");
        if form.contains("fontification-functions") {
            assert!(
                on.0[0]
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .parse::<i64>()
                    .unwrap()
                    > 0,
                "actual fontification callback must execute"
            );
        }
    }
}

#[cfg(test)]
#[path = "edit_sync_dense_index_default_startup_test.rs"]
mod default_startup_tests;
