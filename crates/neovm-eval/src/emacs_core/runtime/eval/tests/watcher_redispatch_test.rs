//! GNU stores a variable only after its watchers ran, re-dispatching on the
//! redirect the watcher left (`set_internal`, `src/data.c:1678-1795`).
//! Every expectation below is GNU Emacs 31.1's answer to the same form
//! (`emacs --batch -Q`, captured for P6.1).

use super::*;

fn eval_printed(source: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    match ctx.eval_str(source) {
        Ok(value) => crate::emacs_core::print::print_value(&value),
        Err(flow) => format!("ERR {flow:?}"),
    }
}

/// `make-variable-buffer-local` creates no binding in the current buffer and a
/// `let` never auto-creates one (`bindflag != SET`), so the bind lands in the
/// default cell.
#[test]
fn let_make_variable_buffer_local_binds_the_default() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-c 0) (add-variable-watcher 'p61-c (lambda (&rest args) (if (eq (nth 2 args) 'let) (make-variable-buffer-local 'p61-c)))) (list (let ((p61-c 1)) p61-c) p61-c (default-value 'p61-c)))"
        ),
        "(1 0 0)"
    );
}

/// The audit's `(list (let ...) 'out)` crash form.
#[test]
fn let_body_value_survives_a_localizing_watcher() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-d 0) (add-variable-watcher 'p61-d (lambda (&rest args) (if (eq (nth 2 args) 'let) (make-local-variable 'p61-d)))) (list (let ((p61-d 1)) 'in) 'out))"
        ),
        "(in out)"
    );
}

/// `Fmakunbound` on a `SYMBOL_VARALIAS` cell turns it back into a void plain
/// cell; the target keeps its value (`src/data.c:779-786`).
#[test]
fn makunbound_of_an_alias_undoes_the_alias() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-k 5) (defvaralias 'p61-k-alias 'p61-k) (makunbound 'p61-k-alias) (list (boundp 'p61-k) (boundp 'p61-k-alias) (eq (indirect-variable 'p61-k-alias) 'p61-k-alias)))"
        ),
        "(t nil t)"
    );
}

/// That alias arm stores directly, without `set_internal`, so a watcher on the
/// target is not called.
#[test]
fn makunbound_of_a_watched_alias_does_not_notify() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-l 5) (defvar p61-l-log nil) (defvaralias 'p61-l-alias 'p61-l) (add-variable-watcher 'p61-l (lambda (&rest args) (setq p61-l-log (cons (nth 2 args) p61-l-log)))) (makunbound 'p61-l-alias) (list (boundp 'p61-l) p61-l-log))"
        ),
        "(t nil)"
    );
}

/// `Fmakunbound` is `Fset (symbol, Qunbound)`; `set_internal` notifies the
/// watchers first and only then refuses to void a forwarded variable
/// (`src/data.c:1697-1706`, `:1802-1806`).
#[test]
fn makunbound_of_a_builtin_notifies_before_refusing() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-m-log nil) (add-variable-watcher 'gc-cons-threshold (lambda (&rest args) (setq p61-m-log (cons (nth 2 args) p61-m-log)))) (list (condition-case err (makunbound 'gc-cons-threshold) (error (car err))) p61-m-log (integerp gc-cons-threshold)))"
        ),
        "(error (makunbound) t)"
    );
}

/// The same order for a per-buffer slot (`BUFFER_OBJFWD`).
#[test]
fn makunbound_of_a_buffer_slot_notifies_before_refusing() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-n-log nil) (add-variable-watcher 'fill-column (lambda (&rest args) (setq p61-n-log (cons (nth 2 args) p61-n-log)))) (list (condition-case err (makunbound 'fill-column) (error (car err))) p61-n-log (integerp fill-column)))"
        ),
        "(error (makunbound) t)"
    );
}

/// An `unlet` watcher that aliases the variable sends the restored value to the
/// alias target: `set_internal` follows `SYMBOL_VARALIAS` after notifying
/// (`src/data.c:1712-1714`).
#[test]
fn unlet_through_a_watcher_alias_restores_the_target() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-o 0) (defvar p61-o-target 10) (add-variable-watcher 'p61-o (lambda (&rest args) (if (eq (nth 2 args) 'unlet) (defvaralias 'p61-o 'p61-o-target)))) (list (let ((p61-o 1)) p61-o) p61-o-target (eq (indirect-variable 'p61-o) 'p61-o-target)))"
        ),
        "(1 0 t)"
    );
}

/// The `let` entry is on the specpdl before the watcher runs, so `defvaralias`
/// refuses a let-bound variable (`src/eval.c:703-711`) and the unwind restores
/// the old value.
#[test]
fn let_watcher_cannot_alias_a_let_bound_variable() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-p 0) (defvar p61-p-target 10) (add-variable-watcher 'p61-p (lambda (&rest args) (if (eq (nth 2 args) 'let) (defvaralias 'p61-p 'p61-p-target)))) (list (condition-case err (let ((p61-p 1)) p61-p) (error (car err))) p61-p p61-p-target))"
        ),
        "(error 0 10)"
    );
}
