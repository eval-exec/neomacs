//! Cursor ownership at the real leaf-loop acceptance boundary.
use super::*;
use neovm_core::emacs_core::error::EvalResult;
use neovm_core::window::{FrameId, WindowCursorPos, WindowId};
use neovm_eval::emacs_core::subr::SubrSpec;

/// Chrome can observe the previous completed output row even after the next
/// body begins emitting provisional progress. This records one semantic fact,
/// rather than reconstructing the checkpoint's field-by-field implementation.
fn committed_cursor_row(eval: &mut Context) -> EvalResult {
    let frame = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame");
    let display = frame
        .find_window(frame.selected_window)
        .and_then(|window| window.display())
        .expect("selected graphical window");
    Ok(Value::fixnum(display.last_cursor_vpos))
}

/// Controlled dispatch probe: canonical row emitters are speculative and do
/// not themselves change live cursor state. A native chrome callback publishes
/// through the real WindowOutputUpdate API so omitting the caller's restore
/// becomes observable at the next actual chrome-relayout attempt. This is not
/// a claim that normal chrome or body production publishes a live cursor.
fn observe_and_publish_provisional_cursor(eval: &mut Context) -> EvalResult {
    let previous = committed_cursor_row(eval)?;
    let frame = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame");
    let frame_id = frame.id;
    let window_id = frame.selected_window;
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .window_output_update(window_id)
        .expect("controlled provisional output")
        .finalize_live_update(Some(NEW_OWNER_CURSOR), None);
    Ok(previous)
}

fn fixture() -> (Context, LayoutEngine, FrameId, WindowId) {
    let text = "body line for cursor ownership\n".repeat(20);
    let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 640, 320);
    realize_test_gui_frame(&mut eval, frame);
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("fixture buffer")
        .set_buffer_local("mode-line-format", Value::string("ML"));
    eval.register_subr(SubrSpec::fixed0(
        "issue830-committed-cursor-row",
        committed_cursor_row,
    ));
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    engine.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &engine, frame);
    assert!(
        eval.frame_manager()
            .get(frame)
            .and_then(|frame| frame.find_window(window))
            .and_then(|window| window.display())
            .and_then(|display| display.phys_cursor.as_ref())
            .is_some(),
        "fixture must have a genuinely rendered accepted cursor"
    );
    (eval, engine, frame, window)
}

#[test]
fn rejected_leaf_restores_the_previously_accepted_cursor() {
    let (mut eval, _engine, frame, window) = fixture();
    let cursor_info = |eval: &mut Context| {
        eval.eval_str("(format \"%S\" (window-cursor-info))")
            .expect("public cursor query")
            .as_str_owned()
            .expect("printed cursor")
    };
    let accepted = cursor_info(&mut eval);
    assert_ne!(accepted, "nil", "the accepted cursor must be visible");
    let accepted_row = committed_cursor_row(&mut eval)
        .expect("accepted cursor row")
        .as_fixnum()
        .expect("numeric row");
    // Ownership-API unit control: genuine accepted rendering above, then
    // provisional output through the same update API the body/tail uses.
    // Actual composition rejection is covered by composition_retry_publication;
    // actual leaf-loop restore dispatch is covered by the chrome-relayout row.
    let checkpoint =
        super::super::leaf_cursor_checkpoint::LeafCursorCheckpoint::capture(&eval, frame, window)
            .expect("capture accepted cursor");
    {
        let mut update = eval
            .frame_manager_mut()
            .get_mut(frame)
            .expect("frame")
            .window_output_update(window)
            .expect("provisional output owner");
        update.begin_update();
        update.output_cursor_to_coords(
            NEW_OWNER_CURSOR.row,
            NEW_OWNER_CURSOR.col,
            NEW_OWNER_CURSOR.y,
            NEW_OWNER_CURSOR.x,
        );
        update.finalize_live_update(Some(NEW_OWNER_CURSOR), None);
    }
    assert_eq!(
        cursor_info(&mut eval),
        "nil",
        "provisional output must differ"
    );
    assert_ne!(
        committed_cursor_row(&mut eval)
            .expect("provisional row")
            .as_fixnum(),
        Some(accepted_row),
        "the provisional tail must change committed row bookkeeping"
    );
    checkpoint.restore(&mut eval, frame, window);
    assert_eq!(cursor_info(&mut eval), accepted);
    assert_eq!(
        committed_cursor_row(&mut eval)
            .expect("restored row")
            .as_fixnum(),
        Some(accepted_row)
    );
}

#[test]
#[tracing_test::traced_test]
fn completed_chrome_relayout_restores_cursor_before_the_next_attempt() {
    let (mut eval, mut engine, frame, window) = fixture();
    let accepted_row = committed_cursor_row(&mut eval)
        .expect("accepted cursor row")
        .as_fixnum()
        .expect("numeric row");
    assert_ne!(
        accepted_row, NEW_OWNER_CURSOR.row,
        "the dispatch probe must differ from the previously accepted cursor"
    );
    eval.register_subr(SubrSpec::fixed0(
        "issue830-observe-and-publish-provisional-cursor",
        observe_and_publish_provisional_cursor,
    ));
    let old_height = eval
        .frame_manager()
        .get(frame)
        .and_then(|frame| frame.redisplay_snapshot(window))
        .expect("accepted geometry")
        .mode_line_height;
    eval.eval_str(
        "(progn
           (setq issue830-rows-seen nil)
           (goto-char (point-min))
           (forward-line 3)
           (setq mode-line-format
                 (list '(:eval (progn
                                 (setq issue830-rows-seen
                                       (cons (issue830-observe-and-publish-provisional-cursor)
                                             issue830-rows-seen))
                                 \"ML\"))
                       (propertize \"T\" 'display '(height 2.0)))))",
    )
    .expect("install intrinsically tall chrome");
    let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("stable tall chrome must converge");
    };
    assert!(
        logs_contain("window chrome metrics changed; rejecting speculative layout"),
        "the real Completed::NeedsRelayout branch must reject the measured leaf"
    );
    let rows = list_to_vec(&eval.eval_str("issue830-rows-seen").expect("observations"))
        .expect("proper observation list");
    assert!(
        rows.len() >= 2,
        "chrome must have caused an actual relayout"
    );
    for row in rows {
        assert_eq!(
            row.as_fixnum(),
            Some(accepted_row),
            "the next chrome evaluation must not inherit the controlled live cursor from a rejected leaf"
        );
    }
    let selected = output
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == window.0 as i64)
        .expect("selected matrix");
    let new_height = selected
        .matrix
        .rows
        .iter()
        .find(|row| row.enabled && row.role == GlyphRowRole::ModeLine)
        .expect("measured mode line")
        .height_px;
    assert!(
        new_height > old_height as f32,
        "intrinsic chrome height must grow"
    );
    let final_row = committed_cursor_row(&mut eval)
        .expect("new committed row")
        .as_fixnum()
        .expect("numeric row");
    assert_ne!(
        final_row, accepted_row,
        "the accepted new point must actually move the cursor to another row"
    );
    assert_ne!(
        final_row, NEW_OWNER_CURSOR.row,
        "the accepted snapshot must replace the controlled live cursor"
    );
}

const NEW_OWNER_CURSOR: WindowCursorPos = WindowCursorPos {
    x: 8,
    y: 32,
    row: 2,
    col: 1,
};

#[test]
fn rejected_leaf_does_not_restore_over_a_switched_buffers_output() {
    let (mut eval, _engine, frame, window) = fixture();
    // This is an ownership-API unit control. The chrome-relayout row exercises
    // real leaf-loop classification. A full redisplay is allowed to converge
    // after a buffer switch and overwrite the cursor with new rendered output.
    let checkpoint =
        super::super::leaf_cursor_checkpoint::LeafCursorCheckpoint::capture(&eval, frame, window)
            .expect("capture the old buffer's accepted cursor");
    eval.eval_str(
        "(progn
           (get-buffer-create \"issue830-new-owner\")
           (set-window-buffer (selected-window) \"issue830-new-owner\"))",
    )
    .expect("switch the real live window to its new buffer owner");
    eval.frame_manager_mut()
        .get_mut(frame)
        .expect("frame")
        .window_output_update(window)
        .expect("new buffer's output owner")
        .finalize_live_update(Some(NEW_OWNER_CURSOR), None);
    checkpoint.restore(&mut eval, frame, window);
    let live = eval
        .frame_manager()
        .get(frame)
        .and_then(|frame| frame.find_window(window))
        .expect("same live window");
    let buffer = live.buffer_id().expect("new buffer owner");
    assert_eq!(
        eval.buffer_manager()
            .get(buffer)
            .expect("buffer")
            .name_runtime_string_owned(),
        "issue830-new-owner"
    );
    let display = live.display().expect("window display");
    assert_eq!(display.cursor, Some(NEW_OWNER_CURSOR));
    assert_eq!(display.last_cursor_vpos, NEW_OWNER_CURSOR.row);
}
