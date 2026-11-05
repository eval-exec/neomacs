//! U2.8: `skip-syntax-forward`/`-backward` parse their class string from its
//! bytes (GNU `skip_syntaxes`) instead of decoding it first; both parses
//! must name the same classes, and the builtins must answer alike either
//! way.

use crate::emacs_core::eval::set_builtin_frontend_for_test;

use neovm_text::emacs_core::syntax::*;

#[test]
fn skip_syntax_answers_alike_with_the_knob_on_and_off() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let ((out nil))
  (dolist (mode '(fundamental-mode emacs-lisp-mode))
    (with-temp-buffer
      (insert "  foo_bar (baz) \"q\" ; c\nÀé 漢字-x ?\\( 'y")
      (funcall mode)
      (dolist (spec '("w" "^w" "w_" " " "^ " "-" "_w" "()" "^()" "\"" "." "<>" "" "^"
                      "wé" "@"))
        (dolist (pos '(1 3 8 12 20 25 30 40))
          (dolist (limit '(nil 1 15 35 100))
            (dolist (fn '(skip-syntax-forward skip-syntax-backward))
              (goto-char (min pos (point-max)))
              (push (list mode spec pos limit fn
                          (condition-case err (funcall fn spec limit)
                            (error (list 'signal err)))
                          (point))
                    out)))))))
  (nreverse out))
"#;
    let run = |frontend: bool| {
        set_builtin_frontend_for_test(Some(frontend));
        let mut eval = crate::test_utils::runtime_startup_context();
        let result = eval.eval_str(form).expect("the skip matrix evaluates");
        set_builtin_frontend_for_test(None);
        crate::emacs_core::print::print_value(&result)
    };
    assert_eq!(run(true), run(false));
}

#[test]
fn parse_partial_sexp_answers_alike_with_the_knob_on_and_off() {
    crate::test_utils::init_test_tracing();
    let form = r#"
(let ((out nil))
  (with-temp-buffer
    (insert "(defun f (x) \"s(tr\" ; c(\n  (let ((y 'z)) [x y] ?\\( #'car))\n(a (b (c")
    (emacs-lisp-mode)
    (dolist (from '(1 2 8 15 23 30 44))
      (dolist (to '(1 10 20 40 60 70))
        (when (<= from to)
          (dolist (target '(nil 0 1 -1))
            (dolist (stop '(nil t))
              (dolist (commentstop '(nil t syntax-table))
                (goto-char 1)
                (let ((state (condition-case err
                                 (parse-partial-sexp from (min to (point-max)) target stop nil commentstop)
                               (error (list 'signal err)))))
                  (push (list from to target stop commentstop state (point)
                              (and (consp state) (not (eq (car state) 'signal))
                                   (condition-case err
                                       (parse-partial-sexp (point) (point-max) nil nil state)
                                     (error (list 'signal err)))))
                        out)))))))))
  (nreverse out))
"#;
    let run = |frontend: bool| {
        set_builtin_frontend_for_test(Some(frontend));
        let mut eval = crate::test_utils::runtime_startup_context();
        let result = eval.eval_str(form).expect("the pps matrix evaluates");
        set_builtin_frontend_for_test(None);
        crate::emacs_core::print::print_value(&result)
    };
    assert_eq!(run(true), run(false));
}
