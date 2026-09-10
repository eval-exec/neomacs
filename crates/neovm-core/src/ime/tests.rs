use crate::Value;

#[test]
fn ime_surrounding_selection_rejects_partial_characters_and_changed_contexts() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    for offset in [2, 3, 4, 99] {
        let mut eval = crate::Context::new();
        eval.eval_str("(insert \"A😀Z\")").unwrap();
        let snapshot = eval.ime_surrounding_text().unwrap();
        assert_eq!(
            eval.ime_set_selection(ImeSelection {
                snapshot: snapshot.id(),
                cursor: offset,
                anchor: 0,
            })
            .unwrap(),
            ImeSelectionOutcome::InvalidSelection
        );
        assert_eq!(eval.eval_str("(point)").unwrap(), Value::fixnum(4));
    }
    for change in [
        "(goto-char 1)",
        "(progn (insert \"!\") (delete-region 4 5))",
        "(narrow-to-region 2 4)",
        "(progn (set-marker (mark-marker) 1) (setq mark-active t))",
        "(set-buffer (get-buffer-create \"other\"))",
    ] {
        let mut eval = crate::Context::new();
        eval.eval_str("(insert \"abc\")").unwrap();
        let snapshot = eval.ime_surrounding_text().unwrap();
        eval.eval_str(change).unwrap();
        assert_eq!(
            eval.ime_set_selection(ImeSelection {
                snapshot: snapshot.id(),
                cursor: 0,
                anchor: 0,
            })
            .unwrap(),
            ImeSelectionOutcome::StaleSnapshot,
            "{change}"
        );
    }
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
fn ime_surrounding_selection_cannot_redirect_mark_after_lisp_changes_context() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    let mut eval = crate::Context::new();
    eval.eval_str("(progn (insert \"abc\") (fset 'goto-char (lambda (_pos) (set-buffer (get-buffer-create \"redirected\")))))").unwrap();
    let snapshot = eval.ime_surrounding_text().unwrap();
    assert_eq!(
        eval.ime_set_selection(ImeSelection {
            snapshot: snapshot.id(),
            cursor: 1,
            anchor: 0,
        })
        .unwrap(),
        ImeSelectionOutcome::StaleSnapshot
    );
    assert!(
        eval.buffers
            .current_buffer()
            .unwrap()
            .mark_emacs_byte_pos()
            .is_none()
    );
}

#[test]
fn ime_surrounding_selection_uses_snapshot_offsets_and_rejects_replay() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str("(progn (erase-buffer) (insert \"A😀éZ\") (set-marker (mark-marker) 1) (setq transient-mark-mode t mark-active t))").unwrap();
    let snapshot = eval.ime_surrounding_text().unwrap();
    let selection = ImeSelection {
        snapshot: snapshot.id(),
        cursor: 5,
        anchor: 1,
    };
    assert_eq!(
        eval.ime_set_selection(selection).unwrap(),
        ImeSelectionOutcome::Applied
    );
    assert_eq!(eval.eval_str("(point)").unwrap(), Value::fixnum(3));
    assert_eq!(eval.eval_str("(mark t)").unwrap(), Value::fixnum(2));
    assert_eq!(
        eval.ime_set_selection(selection).unwrap(),
        ImeSelectionOutcome::StaleSnapshot
    );
    let snapshot = eval.ime_surrounding_text().unwrap();
    assert_eq!(
        eval.ime_set_selection(ImeSelection {
            snapshot: snapshot.id(),
            cursor: 5,
            anchor: 5,
        })
        .unwrap(),
        ImeSelectionOutcome::Applied
    );
    assert!(eval.eval_str("mark-active").unwrap().is_nil());
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
