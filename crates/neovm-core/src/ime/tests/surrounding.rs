#[test]
fn ime_surrounding_snapshot_does_not_export_password_input() {
    for privacy in [
        "(setq overriding-text-conversion-style 'password)",
        "(setq text-conversion-style 'password)",
        "(setq read-hide-char ?*)",
    ] {
        let mut eval = crate::Context::new();
        eval.eval_str(r##"(insert "secret")"##).unwrap();
        eval.eval_str(privacy).unwrap();
        assert!(eval.ime_surrounding_text().is_none(), "{privacy}");
    }
}

#[test]
fn ime_surrounding_snapshot_is_confined_to_the_input_field() {
    let mut eval = crate::Context::new();
    eval.eval_str(
        r##"(progn
        (insert "prompt: abc history")
        (put-text-property 1 9 'field 'prompt)
        (put-text-property 9 12 'field 'input)
        (goto-char 10))"##,
    )
    .unwrap();
    let snapshot = eval.ime_surrounding_text().unwrap();
    assert_eq!(snapshot.text(), "abc");
    assert_eq!(snapshot.cursor(), 1);
}

#[test]
fn ime_gui_export_waits_for_an_acknowledged_live_presentation() {
    use crate::window::{WindowDisplaySnapshot, geometry::PresentationId};
    let mut eval = crate::Context::new();
    let buffer = eval.buffers.current_buffer_id().unwrap();
    eval.eval_str(r##"(insert "visible")"##).unwrap();
    let frame_id = eval.frames.create_frame("ime", 800, 600, buffer);
    let window_id = eval.frames.get(frame_id).unwrap().selected_window;
    eval.frames
        .get_mut(frame_id)
        .unwrap()
        .set_window_system(Some(crate::Value::symbol("neo")));
    assert!(eval.ime_surrounding_text().is_none());
    let freshness = eval
        .window_display_snapshot_freshness(frame_id, window_id, buffer)
        .unwrap();
    eval.frames
        .get_mut(frame_id)
        .unwrap()
        .prepare_live_window_presentation(
            PresentationId::new(1),
            vec![WindowDisplaySnapshot {
                window_id,
                layout_freshness: Some(freshness),
                ..WindowDisplaySnapshot::default()
            }],
        )
        .unwrap();
    assert!(
        eval.ime_surrounding_text().is_none(),
        "prepared is not displayed"
    );
    eval.frames
        .get_mut(frame_id)
        .unwrap()
        .activate_display_presentation(PresentationId::new(1))
        .unwrap();
    assert_eq!(eval.ime_surrounding_text().unwrap().text(), "visible");
    eval.eval_str(r##"(set-buffer (get-buffer-create "not displayed"))"##)
        .unwrap();
    assert!(eval.ime_surrounding_text().is_none());
}

#[test]
fn ime_gui_export_rejects_a_selection_not_yet_acknowledged_by_the_renderer() {
    use crate::window::{
        SplitDirection, SplitPlacement, WindowDisplaySnapshot, geometry::PresentationId,
    };
    let mut eval = crate::Context::new();
    let buffer = eval.buffers.current_buffer_id().unwrap();
    let frame_id = eval.frames.create_frame("ime", 800, 600, buffer);
    let first = eval.frames.get(frame_id).unwrap().selected_window;
    let second = eval
        .frames
        .split_window(
            frame_id,
            first,
            SplitDirection::Horizontal,
            buffer,
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();
    eval.frames
        .get_mut(frame_id)
        .unwrap()
        .set_window_system(Some(crate::Value::symbol("neo")));
    let snapshots = [first, second]
        .into_iter()
        .map(|window_id| WindowDisplaySnapshot {
            window_id,
            layout_freshness: eval.window_display_snapshot_freshness(frame_id, window_id, buffer),
            ..WindowDisplaySnapshot::default()
        })
        .collect();
    let frame = eval.frames.get_mut(frame_id).unwrap();
    frame
        .prepare_live_window_presentation(PresentationId::new(1), snapshots)
        .unwrap();
    frame
        .activate_display_presentation(PresentationId::new(1))
        .unwrap();
    assert!(eval.ime_surrounding_text().is_some());
    eval.frames.get_mut(frame_id).unwrap().select_window(second);
    assert!(
        eval.ime_surrounding_text().is_none(),
        "both windows were drawn, but only the first was selected"
    );
}

#[test]
fn ime_surrounding_snapshot_is_bounded_and_does_not_truncate_selection() {
    use neovm_host_abi::ime::ImeTextSnapshot;
    let mut eval = crate::Context::new();
    eval.eval_str("(progn (insert (make-string 10000 97)) (goto-char 5000))")
        .unwrap();
    let snapshot = eval.ime_surrounding_text().unwrap();
    assert_eq!(snapshot.text().len(), ImeTextSnapshot::MAX_BYTES);
    assert_eq!(snapshot.cursor(), 1999);
    assert_eq!(snapshot.anchor(), 1999);
    eval.eval_str("(progn (set-marker (mark-marker) 1) (setq mark-active t))")
        .unwrap();
    assert!(eval.ime_surrounding_text().is_none());
}

#[test]
fn ime_surrounding_snapshot_exports_unibyte_text_without_lossy_offsets() {
    let mut eval = crate::Context::new();
    eval.eval_str("(progn (set-buffer-multibyte nil) (insert \"éX\") (goto-char 2))")
        .unwrap();
    let snapshot = eval.ime_surrounding_text().unwrap();
    assert_eq!(snapshot.text(), "éX");
    assert_eq!(snapshot.cursor(), 2);
}

#[test]
fn ime_surrounding_snapshot_never_exports_lossy_emacs_characters() {
    let mut eval = crate::Context::new();
    eval.eval_str("(progn (insert-char #x3fff80) (insert \"abc\"))")
        .unwrap();
    assert_eq!(eval.ime_surrounding_text().unwrap().text(), "abc");
    eval.eval_str("(progn (set-marker (mark-marker) 1) (setq mark-active t))")
        .unwrap();
    assert!(eval.ime_surrounding_text().is_none());
}

#[test]
fn ime_surrounding_snapshot_is_retired_when_input_session_ends() {
    use neovm_host_abi::ime::{ImeOperation, ImeSelection, ImeSelectionOutcome, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    let snapshot = eval.ime_surrounding_text().unwrap();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::End)
        .unwrap();
    assert_eq!(
        eval.ime_set_selection(ImeSelection {
            snapshot: snapshot.id(),
            cursor: 0,
            anchor: 0,
        })
        .unwrap(),
        ImeSelectionOutcome::StaleSnapshot
    );
}

#[test]
fn ime_surrounding_snapshot_preserves_unicode_selection_and_narrowing() {
    let mut eval = crate::Context::new();
    eval.eval_str(
        "(progn (insert \"hidden A😀éZ hidden\") (narrow-to-region 8 12)
                (goto-char 11) (set-marker (mark-marker) 9) (setq mark-active t))",
    )
    .unwrap();
    let snapshot = eval
        .ime_surrounding_text()
        .expect("accessible Unicode text");
    assert_eq!(snapshot.text(), "A😀éZ");
    assert_eq!(snapshot.cursor(), 7);
    assert_eq!(snapshot.anchor(), 1);
}
