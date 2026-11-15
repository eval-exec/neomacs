//! VM syntax-motion test re-homed from neovm-eval's VM test module.
//!
//! The fixture drives `forward-comment`, a core-owned syntax command (its R0
//! placement keeps syntax scanning with the command layer). Eval's unit
//! runtime carries the upper entry as an upper-layer refusal, so the test
//! belongs to the runtime that owns the primitive. Core's `new_context`
//! constructs that same surface with the canonical hook-installation proof.
//! The Lisp form and expected string are unchanged from the eval copy.

fn vm_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_syntax_motion_builtins_use_shared_point_and_syntax_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(progn
                 (set-syntax-table (copy-syntax-table (standard-syntax-table)))
                 (modify-syntax-entry ?\; "<")
                 (modify-syntax-entry ?\n ">")
                 (modify-syntax-entry ?' ". p")
                 (erase-buffer)
                 (insert "  ;c\n''foo bar")
                 (list
                  (progn (goto-char 1) (list (forward-comment 1) (point)))
                  (progn (goto-char 8) (backward-prefix-chars) (point))
                  (progn (goto-char 8) (forward-word) (point))
                  (progn (goto-char 1) (list (skip-syntax-forward " ") (point)))
                  (progn (goto-char 11) (list (skip-syntax-backward "w") (point)))))"#
        ),
        "OK ((t 6) 6 11 (2 3) (-3 8))"
    );
}
