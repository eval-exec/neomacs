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
