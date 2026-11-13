fn eval_printed(source: &str) -> String {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    match ctx.eval_str(source) {
        Ok(value) => crate::emacs_core::print::print_value(&value),
        Err(flow) => format!("ERR {flow:?}"),
    }
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
