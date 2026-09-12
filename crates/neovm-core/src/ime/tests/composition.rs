use crate::Value;

#[test]
fn ime_session_uses_target_encoding_for_point_and_owned_replacement_range() {
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.eval_str("(progn (set-buffer-multibyte nil) (insert \"XYZ\") (goto-char 1))")
        .unwrap();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "é".into(),
        },
    )
    .unwrap();
    assert_eq!(eval.eval_str("(point)").unwrap(), Value::fixnum(2));
    assert_eq!(
        eval.eval_str("(aref (buffer-string) 0)").unwrap(),
        Value::fixnum(233)
    );
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 2,
            after_bytes: 0,
            text: String::new(),
        },
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("XYZ")
    );
}

#[test]
fn ime_session_rejects_edits_after_point_or_buffer_changes() {
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.eval_str("(insert \"safe\")").unwrap();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.eval_str("(goto-char 1)").unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 4,
            after_bytes: 0,
            text: "wrong".into(),
        },
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("safe")
    );
    eval.handle_ime_operation(ImeSessionId(2), ImeOperation::Begin)
        .unwrap();
    eval.eval_str("(set-buffer (get-buffer-create \"other-ime\"))")
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(2),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "wrong".into(),
        },
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("")
    );
}

#[test]
fn ime_session_replaces_owned_unicode_text_and_rejects_stale_revisions() {
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.eval_str("(insert \"prefix:\")").unwrap();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "你好😀".into(),
        },
    )
    .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 7,
            after_bytes: 0,
            text: "好!".into(),
        },
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("prefix:你好!")
    );
    eval.eval_str("(save-excursion (goto-char 1) (insert \"changed\"))")
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 4,
            after_bytes: 0,
            text: "wrong".into(),
        },
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("changedprefix:你好!")
    );
}

#[test]
fn ime_session_read_only_and_reentrant_hooks_cannot_redirect_replacement() {
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.eval_str("(setq buffer-read-only t)").unwrap();
    assert!(
        eval.handle_ime_operation(
            ImeSessionId(1),
            ImeOperation::Replace {
                before_bytes: 0,
                after_bytes: 0,
                text: "wrong".into(),
            }
        )
        .is_err()
    );
    eval.eval_str("(setq buffer-read-only nil)").unwrap();
    eval.handle_ime_operation(ImeSessionId(2), ImeOperation::Begin)
        .unwrap();
    eval.eval_str("(setq before-change-functions (list (lambda (beg end) (set-buffer (get-buffer-create \"hook-target\")))))").unwrap();
    eval.handle_ime_operation(
        ImeSessionId(2),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "wrong".into(),
        },
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("")
    );
    assert_eq!(
        eval.eval_str("(progn (set-buffer \"*scratch*\") (buffer-string))")
            .unwrap()
            .as_utf8_str(),
        Some("")
    );
}

#[test]
fn ime_session_cannot_delete_before_its_anchor_or_reopen_a_retired_identity() {
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.eval_str("(insert \"safe\")").unwrap();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 4,
            after_bytes: 0,
            text: "wrong".into(),
        },
    )
    .unwrap();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "wrong".into(),
        },
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("safe")
    );
}
#[test]
fn ime_commit_inherits_adjacent_text_properties_like_gnu() {
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.eval_str(r##"(progn (insert "x") (put-text-property 1 2 'face 'bold))"##)
        .unwrap();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "y".into(),
        },
    )
    .unwrap();
    assert!(
        eval.eval_str("(get-text-property 2 'face)")
            .unwrap()
            .is_symbol_named("bold")
    );
}

#[test]
fn an_unsupported_after_deletion_does_not_retire_the_session() {
    // `after_bytes != 0` (deleting to the right of the anchor) is not
    // supported, but nothing about the session ended. The re-anchored path
    // already restored for this same condition; leaving the unchanged-anchor
    // path retiring instead meant one such request silently killed every
    // later commit.
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 3,
            text: "ignored".into(),
        },
    )
    .unwrap();

    // The session must still be live: a plain insertion has to land.
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "after".into(),
        },
    )
    .unwrap();

    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("after"),
        "an unsupported after-deletion must not disable input for the session"
    );
}

#[test]
fn a_read_only_buffer_rejects_the_edit_without_retiring_the_session() {
    // One IME keystroke into a read-only buffer must not disable input
    // entirely: the buffer refused the edit, the composition did not end.
    use neovm_host_abi::ime::{ImeOperation, ImeSessionId};
    let mut eval = crate::Context::new();
    eval.handle_ime_operation(ImeSessionId(1), ImeOperation::Begin)
        .unwrap();
    eval.eval_str("(setq buffer-read-only t)").unwrap();

    let rejected = eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "blocked".into(),
        },
    );
    assert!(rejected.is_err(), "a read-only buffer must signal");

    eval.eval_str("(setq buffer-read-only nil)").unwrap();
    eval.handle_ime_operation(
        ImeSessionId(1),
        ImeOperation::Replace {
            before_bytes: 0,
            after_bytes: 0,
            text: "allowed".into(),
        },
    )
    .unwrap();

    assert_eq!(
        eval.eval_str("(buffer-string)").unwrap().as_utf8_str(),
        Some("allowed"),
        "input must resume once the buffer is writable again"
    );
}
