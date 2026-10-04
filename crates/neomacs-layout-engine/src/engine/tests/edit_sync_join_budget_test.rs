//! A join moves unchanged rows upward, which the Sync installer cannot
//! complete without GNU's second bottom walk. It must retain the safe prefix
//! and walk once, including in a buffer with default-face remapping.
use super::*;

fn joining(remapped: bool, budget: bool) -> (LayoutStats, u64, usize) {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(budget);
    let text = tabbed_source(100);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 24));
    if remapped {
        frame
            .eval
            .buffer_manager_mut()
            .current_buffer_mut()
            .unwrap()
            .set_buffer_local(
                "face-remapping-alist",
                Value::list(vec![Value::list(vec![
                    Value::symbol("default"),
                    Value::list(vec![
                        Value::keyword("height"),
                        Value::make_float(0.8),
                        Value::keyword("background"),
                        Value::string("#245678"),
                    ]),
                    Value::symbol("default"),
                ])]),
            );
        // Establish the retained prefix under the remap, rather than asking
        // this join test to prove an unrelated remap-change invalidation.
        frame.engine = LayoutEngine::new();
        frame
            .engine
            .layout_frame_rust(&mut frame.eval, frame.frame_id);
    }
    crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
    let stats = frame.step("(delete-region (point) (1+ (point)))");
    let retries = crate::buffer_source::window_source::sync_source_budget_retries_for_test();
    let relaid = frame.main_relaid();
    // No shifted/stale retained state may leak into the next typing frame.
    frame.step("(insert \"x\")");
    (stats, retries, relaid)
}

fn join_once(remapped: bool) {
    let (off, off_retries, off_rows) = joining(remapped, false);
    let (on, on_retries, on_rows) = joining(remapped, true);
    assert_eq!(off_retries, 0);
    assert_eq!(
        on_retries, 0,
        "a proved removed newline cannot use the horizon"
    );
    assert_eq!(off.edit_windows, 1, "{off:?}");
    assert_eq!(on.edit_windows, 1, "{on:?}");
    assert_eq!(on.full_windows, off.full_windows);
    assert_eq!(on.relaid_body_rows, off.relaid_body_rows);
    assert_eq!(on.reused_rows, off.reused_rows);
    assert_eq!(on.relaid_chrome_rows, off.relaid_chrome_rows);
    assert_eq!(on.reused_chrome_rows, off.reused_chrome_rows);
    assert_eq!(on_rows, off_rows);
    assert!(on.reused_rows >= 20, "rows above the join survive: {on:?}");
}

#[test]
fn source_budget_plain_join_walks_once_and_preserves_prefix() {
    join_once(false);
}

#[test]
fn source_budget_text_scale_join_walks_once_and_preserves_prefix() {
    join_once(true);
}

#[test]
fn source_budget_invisible_newline_without_a_delete_still_retries_exactly() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    let text = tabbed_source(100);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    // Property-only invisibility preserves every source newline while
    // collapsing two display rows into one. It cannot satisfy the pure-
    // deletion witness, so the negative-dy horizon must still retry exactly.
    crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
    crate::buffer_source::window_source::reset_sync_source_budget_horizon_reads_for_test();
    frame.step("(put-text-property (point) (1+ (point)) 'invisible t)");
    assert!(
        crate::buffer_source::window_source::sync_source_budget_horizon_reads_for_test() > 0,
        "unchanged source newlines remain eligible for a capped attempt"
    );
    assert_eq!(
        crate::buffer_source::window_source::sync_source_budget_retries_for_test(),
        1,
        "the hidden display boundary must exercise a genuine horizon retry"
    );
}

#[test]
fn source_budget_display_table_join_keeps_unknown_retry_and_fresh_output() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    let text = tabbed_source(100);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    let table = Value::make_char_table(Value::symbol("display-table"), Value::NIL, 6);
    neovm_core::emacs_core::chartable::ct_set_single(
        &table,
        '\n' as i64,
        Value::vector(vec![Value::fixnum('→' as i64), Value::fixnum('\n' as i64)]),
    );
    frame
        .eval
        .buffer_manager_mut()
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("buffer-display-table", table);
    frame.engine = LayoutEngine::new();
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    assert!(
        backend_trace_text_area_text(&selected_window_layout_trace(
            &frame.eval,
            &frame.engine,
            frame.frame_id,
        ))
        .contains('→'),
        "effective display table must be consumed before the join"
    );
    crate::buffer_source::window_source::reset_sync_source_budget_horizon_reads_for_test();
    crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
    frame.step("(delete-region (point) (1+ (point)))");
    assert!(crate::buffer_source::window_source::sync_source_budget_horizon_reads_for_test() > 0);
    assert_eq!(
        crate::buffer_source::window_source::sync_source_budget_retries_for_test(),
        1,
        "display-table row ends cannot prove raw source newline identity"
    );
}

#[test]
fn source_budget_string_owned_row_end_keeps_unknown_retry_and_fresh_output() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    let text = tabbed_source(100);
    let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
    frame
        .eval
        .eval_str(
            "(let ((overlay (make-overlay (line-beginning-position) (line-beginning-position)))) \
               (overlay-put overlay 'before-string \"PRE \"))",
        )
        .unwrap();
    frame.engine = LayoutEngine::new();
    frame
        .engine
        .layout_frame_rust(&mut frame.eval, frame.frame_id);
    let selected = frame
        .eval
        .frame_manager()
        .get(frame.frame_id)
        .unwrap()
        .selected_window;
    assert!(
        frame
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .window_matrices
            .iter()
            .find(|entry| entry.window_id.get() == selected.0 as i64)
            .unwrap()
            .matrix
            .rows
            .iter()
            .any(|row| !row.string_sources().is_empty()),
        "fixture must materialize a string-owned body row"
    );
    crate::buffer_source::window_source::reset_sync_source_budget_horizon_reads_for_test();
    crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
    frame.step("(delete-region (point) (1+ (point)))");
    assert!(crate::buffer_source::window_source::sync_source_budget_horizon_reads_for_test() > 0);
    assert_eq!(
        crate::buffer_source::window_source::sync_source_budget_retries_for_test(),
        1,
        "string-owned row ends keep the exact unknown-damage retry"
    );
}

#[test]
fn source_budget_wrapped_row_delete_keeps_full_layout_and_no_horizon() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    let _budget = SourceBudgetGuard::set(true);
    let text = format!("{}\n{}", "w".repeat(150), tabbed_source(80));
    let mut frame = SyncFrame::new(&text, 75);
    assert!(
        frame
            .engine
            .last_frame_display_state
            .as_ref()
            .unwrap()
            .window_matrices
            .iter()
            .any(|entry| entry.matrix.rows.iter().any(|row| row.continued)),
        "fixture must include a genuine wrapped body row"
    );
    crate::buffer_source::window_source::reset_sync_source_budget_horizon_reads_for_test();
    crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
    let stats = frame.step("(delete-region (1- (point)) (point))");
    assert_eq!(
        stats.edit_windows, 0,
        "wrapped rows retain original refusal"
    );
    assert_eq!(
        crate::buffer_source::window_source::sync_source_budget_horizon_reads_for_test(),
        0
    );
    assert_eq!(
        crate::buffer_source::window_source::sync_source_budget_retries_for_test(),
        0
    );
}
