use super::*;

/// GNU `eval_sub` raises `max_lisp_eval_depth` to 100 before signalling
/// (`src/eval.c:2604-2608`), and because that variable is a `DEFVAR_INT`
/// the raise is a write to the cell Lisp reads afterwards, not a local
/// adjustment.  The bytecode evaluator keeps a cached copy of the limit
/// (`Vm::ctx.max_depth`); raising only the cache leaves `max-lisp-eval-depth`
/// at the old value, which is what O5 caught (99 where GNU reports 100).
///
/// Each form recurses through non-tail calls until the depth check trips,
/// catches the signal, and reads the variable through the dynamic binding
/// that was in force while the check ran.
#[test]
fn depth_floor_is_visible_after_interpreted_overflow() {
    let mut ev = runtime_startup_context();
    let result = ev.eval_str(
        r#"(progn
             (defalias 'fd-ib (lambda (n) (fd-iping (1+ n))))
             (defalias 'fd-iping (lambda (n) (fd-ib n)))
             (let ((max-lisp-eval-depth 99))
               (condition-case nil (fd-ib 0) (error nil))
               max-lisp-eval-depth))"#,
    );
    assert_eq!(format_eval_result(&result), "OK 100");
}

#[test]
fn depth_floor_is_visible_after_bytecode_overflow() {
    let mut ev = runtime_startup_context();
    let result = ev.eval_str(
        r#"(progn
             (defalias 'fd-gb (byte-compile (lambda (n) (fd-gping (1+ n)))))
             (defalias 'fd-gping (byte-compile (lambda (n) (fd-gb n))))
             (setq max-lisp-eval-depth 99)
             (condition-case nil (fd-gb 0) (error nil))
             (prog1 max-lisp-eval-depth
               (setq max-lisp-eval-depth 1600)))"#,
    );
    // GNU answers 100 here as well; the restore to 1600 shows the floor
    // write went through the variable rather than a transient cache.
    assert_eq!(format_eval_result(&result), "OK 100");
}
