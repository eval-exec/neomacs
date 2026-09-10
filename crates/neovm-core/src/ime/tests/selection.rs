use crate::Value;

#[test]
fn ime_selection_rejects_field_changes_without_text_edits() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    for change in [
        "(put-text-property 1 3 'field 'protected)",
        "(overlay-put (make-overlay 1 3) 'field 'protected)",
    ] {
        let mut eval = crate::Context::new();
        eval.eval_str(r##"(insert "abc")"##).unwrap();
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
        assert_eq!(eval.eval_str("(point)").unwrap(), Value::fixnum(4));
    }
}

#[test]
fn ime_selection_rejects_a_context_that_became_private_after_capture() {
    use neovm_host_abi::ime::{ImeSelection, ImeSelectionOutcome};
    let mut eval = crate::Context::new();
    eval.eval_str(r##"(insert "secret")"##).unwrap();
    let snapshot = eval.ime_surrounding_text().unwrap();
    eval.eval_str("(setq overriding-text-conversion-style 'password)")
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
    assert_eq!(eval.eval_str("(point)").unwrap(), Value::fixnum(7));
}

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
