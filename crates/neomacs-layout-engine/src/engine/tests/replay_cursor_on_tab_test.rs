//! A cursor re-decorated on a row a replay REUSED must be sized like the
//! walk sizes it: on a TAB (a stretch glyph) it is one column wide unless
//! `x-stretch-cursor` is set (GNU `get_phys_cursor_geometry`). The replay
//! took the stretch's full width, so the cursor spanned the whole tab until
//! the next full layout. The edit sync reuses the rows below an edit most
//! often, so the case runs under it: point on the TAB that starts line 17,
//! a face change on line 5.

use super::*;

#[test]
fn a_cursor_put_back_on_a_reused_tab_is_one_column_wide() {
    crate::incremental_layout::edit_sync::set_edit_sync_mode_for_test(Some(
        crate::incremental_layout::edit_sync::EditSyncMode::Sync,
    ));
    let text: String = (0..60)
        .map(|i| {
            if i % 2 == 0 {
                format!("(defun f{i} (a b)\n")
            } else {
                format!("\t(+ a b {i}))\n")
            }
        })
        .collect();
    let line17: usize = text
        .split_inclusive('\n')
        .take(17)
        .map(|line| line.chars().count())
        .sum();
    let (mut eval, frame_id, _buf, _window) = incr_editing_frame(&text, 800, 600);
    eval.eval_str("(setq bidi-paragraph-direction 'left-to-right)")
        .expect("bidi");
    eval.eval_str(&format!("(goto-char {})", line17 + 1))
        .expect("goto");
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    eval.eval_str("(put-text-property 80 84 'face 'bold)")
        .expect("face");
    engine.layout_frame_rust(&mut eval, frame_id);
    let incremental = selected_window_layout_trace(&eval, &engine, frame_id);
    let mut fresh = LayoutEngine::new();
    fresh.layout_frame_rust(&mut eval, frame_id);
    let reference = selected_window_layout_trace(&eval, &fresh, frame_id);
    crate::incremental_layout::edit_sync::set_edit_sync_mode_for_test(None);
    assert_eq!(incremental.phys_cursor, reference.phys_cursor);
    assert_eq!(incremental, reference);
}
