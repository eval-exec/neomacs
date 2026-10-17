//! GNU Emacs 31.1 prefix-help selection and ordinary command dispatch.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

/// Neither Lisp reader executes prefix help or drops its final help event.
#[test]
fn prefix_help_readers_preserve_help_events_without_executing_commands() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
          (defvar issue576-help-ran nil)
          (defun issue576-help () (interactive) (setq issue576-help-ran t))
          (let ((overriding-terminal-local-map (make-sparse-keymap))
                (help-char 8) (help-event-list '(63 f1 issue576-help-event)) out)
            (define-key overriding-terminal-local-map [3] (make-sparse-keymap))
            (dolist (reader '(read-key-sequence read-key-sequence-vector))
              (dolist (command '(issue576-help nil))
                (dolist (event '(8 63 f1 issue576-help-event))
                  (let ((prefix-help-command command)
                        (unread-command-events (list 3 event)))
                    (setq issue576-help-ran nil)
                    (push (list reader command event
                                (append (funcall reader nil) nil)
                                issue576-help-ran) out)))))
            (nreverse out)))"#,
        expect_test::expect![[
            r#""OK ((read-key-sequence issue576-help 8 (3 8) nil) (read-key-sequence issue576-help 63 (3 63) nil) (read-key-sequence issue576-help f1 (3 f1) nil) (read-key-sequence issue576-help issue576-help-event (3 issue576-help-event) nil) (read-key-sequence nil 8 (3 8) nil) (read-key-sequence nil 63 (3 63) nil) (read-key-sequence nil f1 (3 f1) nil) (read-key-sequence nil issue576-help-event (3 issue576-help-event) nil) (read-key-sequence-vector issue576-help 8 (3 8) nil) (read-key-sequence-vector issue576-help 63 (3 63) nil) (read-key-sequence-vector issue576-help f1 (3 f1) nil) (read-key-sequence-vector issue576-help issue576-help-event (3 issue576-help-event) nil) (read-key-sequence-vector nil 8 (3 8) nil) (read-key-sequence-vector nil 63 (3 63) nil) (read-key-sequence-vector nil f1 (3 f1) nil) (read-key-sequence-vector nil issue576-help-event (3 issue576-help-event) nil))""#
        ]],
    );
}

/// Prefix help observes normal command identity and pre/body/post ordering.
#[test]
fn prefix_help_command_runs_between_hooks_and_preserves_following_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
          (defvar issue576-trace nil)
          (defun issue576-record (stage)
            (when (eq this-command 'issue576-help)
              (push (list stage this-command real-this-command
                          (append (this-command-keys) nil)) issue576-trace)))
          (defun issue576-help () (interactive) (issue576-record 'body))
          (defun issue576-after () (interactive) (push 'after issue576-trace))
          (let ((overriding-terminal-local-map (make-sparse-keymap))
                (prefix-help-command 'issue576-help) (help-char 8)
                (pre-command-hook (list (lambda () (issue576-record 'pre))))
                (post-command-hook (list (lambda () (issue576-record 'post)))))
            (define-key overriding-terminal-local-map [3] (make-sparse-keymap))
            (define-key overriding-terminal-local-map [122] 'issue576-after)
            (execute-kbd-macro [3 8 122])
            (nreverse issue576-trace)))"#,
        expect_test::expect![[
            r#""OK ((pre issue576-help issue576-help (3 8)) (body issue576-help issue576-help (3 8)) (post issue576-help issue576-help (3 8)) after)""#
        ]],
    );
}

/// A binding for the full sequence wins over the prefix-help fallback.
#[test]
fn explicit_prefix_help_key_binding_takes_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
          (defvar issue576-selected nil)
          (defun issue576-help () (interactive) (setq issue576-selected 'help))
          (defun issue576-bound () (interactive) (setq issue576-selected 'bound))
          (let ((overriding-terminal-local-map (make-sparse-keymap))
                (prefix-help-command 'issue576-help) (help-char 8))
            (define-key overriding-terminal-local-map [3] (make-sparse-keymap))
            (define-key overriding-terminal-local-map [3 8] 'issue576-bound)
            (execute-kbd-macro [3 8])
            issue576-selected))"#,
        expect_test::expect![[r#""OK bound""#]],
    );
}

/// Hooks see a shifted prefix as translated even when its next event is help.
#[test]
fn shifted_prefix_help_finalizes_translation_before_command_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(progn
          (defvar issue576-trace nil)
          (defun issue576-record (stage)
            (when (eq this-command 'issue576-help)
              (push (list stage (append (this-command-keys) nil)
                          this-command-keys-shift-translated) issue576-trace)))
          (defun issue576-help () (interactive) (issue576-record 'body))
          (let ((overriding-terminal-local-map (make-sparse-keymap))
                (prefix-help-command 'issue576-help) (help-char 8)
                (translate-upper-case-key-bindings t)
                (pre-command-hook (list (lambda () (issue576-record 'pre))))
                (post-command-hook (list (lambda () (issue576-record 'post)))))
            (define-key overriding-terminal-local-map [f5] (make-sparse-keymap))
            (execute-kbd-macro [S-f5 8])
            (nreverse issue576-trace)))"#,
        expect_test::expect![[r#""OK ((pre (f5 8) t) (body (f5 8) t) (post (f5 8) t))""#]],
    );
}
