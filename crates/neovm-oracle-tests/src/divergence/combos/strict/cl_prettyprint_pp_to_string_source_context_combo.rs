//! Strict combo oracle probes, batch 309: cl-prettyprint + pp + macroexpansion
//! surface. cl-prettyprint to buffer, pp-to-string, macroexpand-all, and
//! cl-source-context.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_prettyprint_to_buffer_pp_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
;; Pin the interpreted flavor: cl-prettyprint's closure leaks into the
;; error data below, and its printed form flips between (closure ...) and
;; #[...]-with-.elc-path with the checkout's byte-compile state. Loading
;; the source keeps the expect flavor- and path-free everywhere.
(let ((load-suffixes '(".el"))) (load "emacs-lisp/cl-extra" nil t))
(list (with-temp-buffer
        (cl-prettyprint '(a (b c) (d (e f))) (current-buffer))
        (buffer-string))
      (condition-case err
          (pp-to-string '(1 (2 3) 4))
        (error 'pp-unavailable))
      (with-temp-buffer
        (cl-prettyprint '(lambda (x) (* x 2)) (current-buffer))
        (buffer-string)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (wrong-number-of-arguments (closure (cl-struct-cl--random-state-tags t) (form) (let ((pt (point)) last) (insert \"\\n\" (prin1-to-string form) \"\\n\") (setq last (point)) (goto-char (1+ pt)) (while (search-forward \"(quote \" last t) (delete-char -7) (insert \"'\") (forward-sexp) (delete-char 1)) (goto-char (1+ pt)) (cl--do-prettyprint))) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_macroexpand_all_source_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-extra)
(require 'macroexp)
(list (macroexpand-all '(when t (progn (push 1 x) (pop x))))
      (macroexpand-all '(cl-loop for i below 3 collect i))
      (consp (macroexp-macroexpand '(when t 'x) nil))
      (eq (macroexpand-all t) t))
"##;
    let expect = expect_test::expect![[
        r#""OK ((if t (progn (progn (setq x (cons 1 x)) (car-safe (prog1 x (setq x (cdr x))))))) (let* ((i 0) (--cl-var-- nil)) (while (< i 3) (setq --cl-var-- (cons i --cl-var--)) (setq i (+ i 1))) (nreverse --cl-var--)) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_setf_get_macroexpand_setf_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-extra)
;; Pin the interpreted flavor: gv-get's setter closures appear verbatim in
;; the expansion below, and their printed form flips between (closure ...)
;; and #[...]-with-.elc-path with the checkout's byte-compile state.
;; Loading the source keeps the expect flavor- and path-free everywhere —
;; and locks the final-image interpreted-closure env FILTER (GNU
;; loadup.el:387-392): without it these envs balloon to the whole lexical
;; environment.
(let ((load-suffixes '(".el"))) (load "emacs-lisp/gv" nil t))
(list (gv-get '(car x) #'cons)
      (consp (macroexpand '(setf (car x) 5)))
      (macroexpand-all '(cl-incf (car x)))
      (macroexpand-all '(cl-pushnew 1 lst)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((let* ((v x)) ((car v) closure ((vars v) (setter closure (t) (val &rest args) (cons 'setcar (append args (list val))))) (v) (apply setter v vars))) t (let* ((v x)) (setcar v (+ (car v) 1))) (if (memql 1 lst) (with-no-warnings lst) (setq lst (cons 1 lst))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
