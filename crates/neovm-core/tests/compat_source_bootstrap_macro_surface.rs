use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::format_eval_result;

#[test]
fn compat_source_bootstrap_macro_surface_is_minimal() {
    let mut eval = Context::new();
    eval.set_lexical_binding(true);

    let result = eval.eval_str(
        r#"(let* ((pcase-macroexpander
                   (intern "`--pcase-macroexpander"))
                  (symbols (list 'eval-and-compile
                                 'defvar-local
                                 'track-mouse
                                 'with-current-buffer
                                 'with-temp-buffer
                                 'with-output-to-string
                                 'with-syntax-table
                                 'with-mutex
                                 'pcase
                                 'pcase-defmacro
                                 pcase-macroexpander)))
  (mapcar
   (lambda (sym)
     (list sym
           (fboundp sym)
           ;; `macrop' is itself part of the surface this test measures: GNU
           ;; has no C version, only `(defun macrop (object) ...)' at
           ;; lisp/subr.el:4793 (DIVERGENCES.md 148).  Ask it the way that
           ;; `defun' does, over `indirect-function', which IS a C subr
           ;; (src/data.c:2557).
           (let ((def (indirect-function sym)))
             (if (consp def) (eq 'macro (car def))))))
   symbols))"#,
    );
    let rendered = format_eval_result(&result);
    assert_eq!(
        rendered,
        "OK ((eval-and-compile nil nil) (defvar-local nil nil) (track-mouse nil nil) (with-current-buffer nil nil) (with-temp-buffer nil nil) (with-output-to-string nil nil) (with-syntax-table nil nil) (with-mutex nil nil) (pcase nil nil) (pcase-defmacro nil nil) (\\`--pcase-macroexpander nil nil))"
    );
}
