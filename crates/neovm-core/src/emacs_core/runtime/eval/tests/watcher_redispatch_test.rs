//! GNU stores a variable only after its watchers ran, re-dispatching on the
//! redirect the watcher left (`set_internal`, `src/data.c:1678-1795`).
//! Every expectation below is GNU Emacs 31.1's answer to the same form
//! (`emacs --batch -Q`, captured for P6.1).

use super::*;

fn eval_printed(source: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    match ctx.eval_str(source) {
        Ok(value) => crate::emacs_core::print::print_value(&value),
        Err(flow) => format!("ERR {flow:?}"),
    }
}

/// GNU `do_specbind` on a trapped plain cell calls `set_internal (...,
/// SET_INTERNAL_BIND)`, which runs the watcher and only then switches on the
/// redirect (`src/eval.c:3613-3621`, `src/data.c:1678-1718`): the watcher's
/// `make-local-variable` makes the `let` bind the new buffer-local binding.
/// This used to write the plain value over the BLV pointer and crash.
#[test]
fn let_make_local_rebinds_the_new_buffer_local() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-a 0) (add-variable-watcher 'p61-a (lambda (&rest args) (if (eq (nth 2 args) 'let) (make-local-variable 'p61-a)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (let ((p61-a 1)) p61-a)) (kill-buffer p61-buf)))))"
        ),
        "1"
    );
}

/// The binding entry stays `SPECPDL_LET` and the symbol is no longer plain at
/// unwind, so `do_one_unbind` restores the DEFAULT through
/// `set_default_internal` (`src/eval.c:3846-3867`): the buffer-local binding
/// keeps the let value.
#[test]
fn let_make_local_leaves_the_local_after_unwind() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-b 0) (add-variable-watcher 'p61-b (lambda (&rest args) (if (eq (nth 2 args) 'let) (make-local-variable 'p61-b)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (list (let ((p61-b 1)) p61-b) p61-b (default-value 'p61-b))) (kill-buffer p61-buf)))))"
        ),
        "(1 1 0)"
    );
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

/// A dynamic lambda argument is the same `specbind` (`funcall_lambda`,
/// `src/eval.c`).
#[test]
fn lambda_argument_binding_redispatches() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-e 0) (add-variable-watcher 'p61-e (lambda (&rest args) (if (eq (nth 2 args) 'let) (make-local-variable 'p61-e)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (list (funcall '(lambda (p61-e) p61-e) 1) p61-e (default-value 'p61-e))) (kill-buffer p61-buf)))))"
        ),
        "(1 1 0)"
    );
}

/// `set_default_internal` on a plain cell is `set_internal (symbol, value,
/// Qnil, bindflag)` (`src/data.c:2063-2066`), which re-dispatches after its
/// watcher: the current buffer's new binding takes the value, the default keeps
/// 0.
#[test]
fn set_default_redispatches_to_the_new_local() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-f 0) (add-variable-watcher 'p61-f (lambda (&rest args) (if (eq (nth 2 args) 'set) (make-local-variable 'p61-f)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (set-default 'p61-f 1) (list p61-f (default-value 'p61-f))) (kill-buffer p61-buf)))))"
        ),
        "(1 0)"
    );
}

/// `do_one_unbind` on a trapped plain cell is `set_internal (...,
/// SET_INTERNAL_UNBIND)` (`src/eval.c:3846-3856`): the watcher's new binding
/// takes the restored value.
#[test]
fn unlet_redispatches_to_the_new_local() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-g 0) (add-variable-watcher 'p61-g (lambda (&rest args) (if (eq (nth 2 args) 'unlet) (make-local-variable 'p61-g)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (let ((p61-g 1)) 'in) (list p61-g (default-value 'p61-g))) (kill-buffer p61-buf)))))"
        ),
        "(0 1)"
    );
}

/// `Fmakunbound` is `Fset (symbol, Qunbound)` (`src/data.c:767-789`): after the
/// watcher, the new binding is voided and the default kept.
#[test]
fn makunbound_redispatches_to_the_new_local() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-h 0) (add-variable-watcher 'p61-h (lambda (&rest args) (if (eq (nth 2 args) 'makunbound) (make-local-variable 'p61-h)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (makunbound 'p61-h) (list (boundp 'p61-h) (default-value 'p61-h))) (kill-buffer p61-buf)))))"
        ),
        "(nil 0)"
    );
}

/// The `set` path already re-dispatched after its watcher; kept as the
/// reference shape.
#[test]
fn setq_redispatches_to_the_new_local() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-i 0) (add-variable-watcher 'p61-i (lambda (&rest args) (if (eq (nth 2 args) 'set) (make-local-variable 'p61-i)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (setq p61-i 1) (list p61-i (default-value 'p61-i))) (kill-buffer p61-buf)))))"
        ),
        "(1 0)"
    );
}

/// GNU `set_internal` stores `Qunbound` into `blv->defcell` for a buffer
/// without its own binding (`src/data.c:1714-1762`); the variable stays buffer-
/// local, so another buffer's binding survives.
#[test]
fn makunbound_keeps_other_buffers_bindings() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-j 1) (let ((other (get-buffer-create \" *p61-other*\"))) (save-current-buffer (set-buffer other) (set (make-local-variable 'p61-j) 2)) (makunbound 'p61-j) (prog1 (list (save-current-buffer (set-buffer other) p61-j) (default-boundp 'p61-j)) (kill-buffer other))))"
        ),
        "(2 nil)"
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

/// A `set` watcher that makes the variable automatically buffer-local turns
/// `set-default`'s plain store into `set_internal`'s `SET` store, which creates
/// the current buffer's binding (`src/data.c:1742-1756`).
#[test]
fn set_default_of_an_auto_local_redispatches() {
    assert_eq!(
        eval_printed(
            "(progn (defvar p61-q 0) (add-variable-watcher 'p61-q (lambda (&rest args) (if (eq (nth 2 args) 'set) (make-variable-buffer-local 'p61-q)))) (let ((p61-buf (get-buffer-create \" *p61-temp*\"))) (save-current-buffer (set-buffer p61-buf) (unwind-protect (progn (set-default 'p61-q 1) (list p61-q (default-value 'p61-q) (local-variable-p 'p61-q))) (kill-buffer p61-buf)))))"
        ),
        "(1 0 t)"
    );
}
