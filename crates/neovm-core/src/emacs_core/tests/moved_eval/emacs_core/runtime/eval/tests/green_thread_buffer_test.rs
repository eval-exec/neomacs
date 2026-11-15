//! Green thread-buffer disposition rows re-homed from neovm-eval's
//! `green_switch_test.rs`.
//!
//! `kill-buffer` is a core-owned subr, so a row that kills the buffer a
//! worker is seated in can only execute where the production hook set is
//! installed. The scheduler semantics under test (thread-buffer-killed
//! signaling, `silently`/`t` dispositions, per-thread buffer capture) are
//! eval's; the assertions are the ones the eval tests had.

use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::value::Value;

fn eval(ev: &mut Context, src: &str) -> String {
    match ev.eval_str(src) {
        Ok(value) => print_value(&value),
        Err(neovm_value::error::EvalError::Signal { symbol, data, .. }) => format!(
            "ERR {} {}",
            neovm_value::intern::resolve_sym(symbol),
            print_value(&Value::list(data))
        ),
        Err(error) => format!("ERR {error:?}"),
    }
}

fn green() -> Context {
    crate::test_utils::init_test_tracing();
    #[cfg(feature = "jit")]
    crate::emacs_core::jit::force_jit_off_for_test(true);
    crate::test_utils::new_green_context()
}

#[test]
fn killing_a_threads_buffer_moves_it_and_signals_it() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgb-log nil)
               (let* ((b (setq tgb-b (get-buffer-create \"tgb-x\")))
                      (th (save-current-buffer
                            (set-buffer b)
                            (make-thread
                             (lambda ()
                               (condition-case e (progn (thread-yield) (setq tgb-log (cons 'resumed tgb-log)))
                                 (error (setq tgb-log (cons (list e (eq (current-buffer) tgb-b)
                                                                (buffer-live-p (current-buffer)))
                                                          tgb-log)))))))))
                 (thread-yield)
                 (setq tgb-log (cons (list 'kill (kill-buffer b) (buffer-live-p b)) tgb-log))
                 (setq tgb-log (cons (condition-case e (thread-join th) (error (list 'join e))) tgb-log))
                 (nreverse tgb-log)))"
        ),
        "((kill t nil) ((thread-buffer-killed) nil t) (join (thread-buffer-killed)))"
    );
}

#[test]
fn a_thread_buffer_disposition_of_silently_or_t() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgq-log nil)
               (let* ((s (setq tgq-s (get-buffer-create \"tgq-s\")))
                      (k (get-buffer-create \"tgq-k\"))
                      (quiet (save-current-buffer
                               (set-buffer s)
                               (make-thread (lambda ()
                                              (thread-yield)
                                              (setq tgq-log (cons (list 'quiet (eq (current-buffer) tgq-s) (buffer-live-p (current-buffer)))
                                                                tgq-log)))
                                            nil 'silently)))
                      (keeper (save-current-buffer
                                (set-buffer k)
                                (make-thread (lambda ()
                                               (thread-yield)
                                               (setq tgq-log (cons (list 'keeper (buffer-name)) tgq-log)))
                                             nil t))))
                 (thread-yield)
                 (setq tgq-log (cons (list (kill-buffer s) (kill-buffer k) (buffer-live-p k)) tgq-log))
                 (thread-join quiet)
                 (thread-join keeper)
                 (nreverse tgq-log)))"
        ),
        "((t nil t) (quiet nil t) (keeper \"tgq-k\"))"
    );
}

#[test]
fn a_thread_starting_leaves_the_previous_threads_buffer_alone() {
    let mut ev = green();
    assert_eq!(
        eval(
            &mut ev,
            "(progn
               (setq tgr-a (get-buffer-create \"tgr-a\") tgr-b (get-buffer-create \"tgr-b\"))
               (let* ((first (save-current-buffer
                               (set-buffer tgr-a)
                               (make-thread (lambda () (thread-yield) (buffer-name)) nil t)))
                      (second (save-current-buffer
                               (set-buffer tgr-b)
                               (make-thread (lambda () (thread-yield) (buffer-name))))))
                 (thread-yield)
                 (list (kill-buffer tgr-a) (buffer-live-p tgr-a)
                       (thread-join first) (thread-join second))))"
        ),
        "(nil t \"tgr-a\" \"tgr-b\")"
    );
}
