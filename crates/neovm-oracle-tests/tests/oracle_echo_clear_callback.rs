//! Echo-clear callback behavior preserved while avoiding no-op invalidation.
//! GNU Emacs 31.1, xdisp.c:13635-13661: always invoke callable clear handlers
//! under inhibit-quit, honor dont-clear-message, and preserve their effects.
use crate::common;

#[test]
fn echo_clear_callbacks_keep_empty_message_and_mutation_semantics() {
    common::assert_oracle_parity_expect(
        r#"(let ((calls 0) (quits nil) (set-message-function nil)
      (clear-message-function nil) (echo-area-clear-hook nil)
      (message-log-max nil) (inhibit-message nil))
  (setq clear-message-function
        (lambda () (setq calls (1+ calls)) (push inhibit-quit quits) nil))
  (message nil)
  (message "")
  (let ((empty-calls calls))
    (with-temp-buffer
      (insert "before")
      (setq clear-message-function
            (lambda () (setq calls (1+ calls))
              (insert "-changed") 'dont-clear-message))
      (message nil)
      (list empty-calls calls (nreverse quits) (buffer-string)))))"#,
        expect_test::expect![[r#""OK (2 3 (t t) \"before-changed\")""#]],
    );
}
