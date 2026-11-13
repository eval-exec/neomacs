//! GNU keyboard.c safe_run_hook_funcall installs a Qt handler per function.
//! The handler blocks outer handler-bind and the ordinary debugger, while
//! debug-on-signal deliberately remains observable. These expected values
//! were printed by the attested pinned GNU oracle (safe-hook-barrier/probes.log).

use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn safe_hook_barrier_blocks_outer_handler_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity_expect(
        r#"(progn
(setq probe-events nil)
(defun probe-bad () (error "boom"))
(defun probe-next () (setq probe-events (cons 'next probe-events)))
(setq auto-save-hook (list #'probe-bad #'probe-next))
(handler-bind ((error (lambda (_condition)
                       (setq probe-events (cons 'outer-handler-bind probe-events)))))
  (do-auto-save t))
(list (nreverse probe-events) auto-save-hook))"#,
        expect_test::expect![[r#""OK ((next) (probe-next))""#]],
    );
}

#[test]
fn safe_hook_removal_watcher_error_escapes_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity_expect(
        r#"(progn
(setq probe-events nil)
(defun probe-bad ()
  (setq probe-events (cons 'bad probe-events))
  (error "boom"))
(defun probe-next () (setq probe-events (cons 'next probe-events)))
(defun probe-removal-watcher (_symbol _new operation _where)
  (when (eq operation 'set)
    (setq probe-events (cons 'removal-watcher probe-events))
    (error "remove-boom")))
(setq auto-save-hook (list #'probe-bad #'probe-next))
(add-variable-watcher 'auto-save-hook #'probe-removal-watcher)
(setq probe-result
      (condition-case err
          (handler-bind ((error (lambda (_condition)
                                 (setq probe-events (cons 'outer-handler-bind probe-events)))))
            (do-auto-save t)
            'returned)
        (error (list 'escaped (car err) (cdr err)))))
(remove-variable-watcher 'auto-save-hook #'probe-removal-watcher)
(list probe-result (nreverse probe-events) auto-save-hook))"#,
        expect_test::expect![[
            r#""OK ((escaped error (\"remove-boom\")) (bad removal-watcher outer-handler-bind) (probe-bad probe-next))""#
        ]],
    );
}
