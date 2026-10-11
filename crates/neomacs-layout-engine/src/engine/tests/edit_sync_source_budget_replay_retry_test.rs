//! A proved joined line must preserve the admitted edit prefix without
//! arming an artificial source horizon that cannot synchronize upward.
use super::*;

#[test]
fn source_budget_joined_line_retry_preserves_edit_classification_and_prefix() {
    let _sync = SyncGuard::set(EditSyncMode::Sync);
    fn run(enabled: bool) -> (LayoutStats, u64) {
        let _budget = SourceBudgetGuard::set(enabled);
        crate::buffer_source::window_source::reset_sync_source_budget_retries_for_test();
        let text = tabbed_source(80);
        let mut frame = SyncFrame::new(&text, end_of_line(&text, 15));
        // step() compares complete glyph/placement/point/window-end state to
        // a full renderer; this also pins classification and retained prefix.
        let stats = frame.step("(delete-region (point) (1+ (point)))");
        (
            stats,
            crate::buffer_source::window_source::sync_source_budget_retries_for_test(),
        )
    }
    let (off, off_retries) = run(false);
    let (on, on_retries) = run(true);
    assert_eq!(off_retries, 0);
    assert_eq!(
        on_retries, 0,
        "a proved join must skip the artificial horizon"
    );
    assert_eq!(off.edit_windows, 1, "{off:?}");
    assert_eq!(on.edit_windows, 1, "{on:?}");
    assert_eq!(on.full_windows, off.full_windows);
    assert_eq!(
        on.reused_rows, off.reused_rows,
        "retained prefix must survive"
    );
    assert_eq!(on.relaid_body_rows, off.relaid_body_rows);
    assert_eq!(on.relaid_chrome_rows, off.relaid_chrome_rows);
    assert_eq!(on.reused_chrome_rows, off.reused_chrome_rows);
}
