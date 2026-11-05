//! The two BOOT-FULL key-description oracles, split out of eval's
//! builtins/tests/keymaps_test.rs (EVAL-UNIT-RUNTIME: FULL tests re-home; the
//! other two tests in that file are CONTEXT-FREE and stay).
//!
//! Census v3: both chains are assert/route -> runtime_startup_eval_all().

#[test]
fn key_description_accepts_position_bearing_mouse_events() {
    crate::test_utils::init_test_tracing();
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(let ((event '(C-mouse-movement
                          (nil 332 (801 . 219) 0 nil 332 (0 . 12)
                           nil (0 . 0) (1679 . 18)))))
             (list (key-description (list event))
                   (key-description (vector event))
                   (single-key-description event)
                   (single-key-description event t)))"#,
    );
    assert_eq!(
        result,
        [
            r#"OK ("C-<mouse-movement>" "C-<mouse-movement>" "C-<mouse-movement>" "C-mouse-movement")"#
        ]
    );
}

#[test]
fn single_key_description_distinguishes_event_payloads_ranges_and_lucid_lists() {
    // Literal results measured with GNU Emacs's public builtin.
    let result = crate::test_utils::runtime_startup_eval_all(
        r#"(mapcar (lambda (event)
                     (condition-case err (single-key-description event)
                       (error (car err))))
                   (list '(mouse-1 (nil 1 (2 . 3)))
                         '(drag-mouse-1 (nil 1 (2 . 3)) (nil 2 (4 . 5)))
                         (cons 'mouse-movement 42)
                         '((mouse-1) (nil))
                         '(meta shift up)
                         (cons ?a ?z)))"#,
    );
    assert_eq!(
        result,
        [r#"OK ("<mouse-1>" "<drag-mouse-1>" "<mouse-movement>" error "M-S-<up>" "a..z")"#]
    );
}
