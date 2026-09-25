//! Oracle pins for interpreted-closure creation through
//! `cconv-make-interpreted-closure` (P4.1 Stage 0, tests T0.1-T0.8).
//!
//! Neomacs can answer these through the `cconv-make-interpreted-closure`
//! memo (`NEOVM_CCONV_MEMO`) and the native untrimmed path
//! (`NEOVM_CCONV_FAST`).  Every form creates the same closures repeatedly so
//! that a memo would serve the later ones, and is checked against GNU once
//! and against Neomacs under a knob matrix: both off (the default), both on,
//! and the memo in strict verify mode (every hit is also computed by the
//! Lisp and compared; a mismatch aborts the process).
//!
//! Regenerate the expectations from GNU only:
//! `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1 cargo nextest run -p neovm-oracle-tests -E 'test(/cconv_memo/)'`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The Neomacs configurations every form must answer identically under.
const KNOB_MATRIX: &[&[(&str, &str)]] = &[
    &[],
    &[("NEOVM_CCONV_MEMO", "on"), ("NEOVM_CCONV_FAST", "on")],
    &[
        ("NEOVM_CCONV_MEMO", "verify"),
        ("NEOVM_CCONV_FAST", "on"),
        ("NEOVM_CCONV_MEMO_STRICT", "1"),
    ],
];

/// T0.1: printed closures, environment order, shadowed duplicates, dynamic
/// variables, the shared `(t)` constant, and the body shared with the source.
/// The definitions are expanded once, as loading a file expands them, so
/// every creation sees the same `#'(lambda ...)' constant.
#[test]
fn oracle_cconv_memo_closure_shapes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (eval
   (macroexpand-all
    '(progn
       (defvar cm-o-dyn 1)
       (defun cm-o-a (x y) (let ((z 3) (cm-o-dyn 2)) (lambda (w) (list x z w cm-o-dyn))))
       (defun cm-o-b (x) (lambda () 1))
       (defun cm-o-c (x) (let ((f (lambda (q) (+ q x)))) (setq x 5) f))
       (defun cm-o-d (x) (let ((x 1) (x 2)) (lambda () x)))
       (defun cm-o-e (x) (let ((a 1) (b 2)) (lambda () (mapcar (lambda (y) (list y b)) (list x)))))))
   t)
  (let (out)
    (dotimes (i 3)
      (push (list (prin1-to-string (cm-o-a 1 2))
                  (prin1-to-string (cm-o-b 1))
                  (eq (aref (cm-o-b 1) 2) (aref (cm-o-b 2) 2))
                  (prin1-to-string (cm-o-c 1))
                  (prin1-to-string (cm-o-d 0))
                  (prin1-to-string (funcall (cm-o-e i)))
                  (eq (aref (cm-o-a 1 2) 1) (aref (cm-o-a 3 4) 1)))
            out))
    out))
"#;
    let expect = expect_test::expect![[
        r##""OK ((\"#[(w) ((list x z w cm-o-dyn)) ((z . 3) (x . 1))]\" \"#[nil (1) (t)]\" t \"#[(q) ((+ q x)) ((x . 5))]\" \"#[nil (x) ((x . 2))]\" \"((2 2))\" t) (\"#[(w) ((list x z w cm-o-dyn)) ((z . 3) (x . 1))]\" \"#[nil (1) (t)]\" t \"#[(q) ((+ q x)) ((x . 5))]\" \"#[nil (x) ((x . 2))]\" \"((1 2))\" t) (\"#[(w) ((list x z w cm-o-dyn)) ((z . 3) (x . 1))]\" \"#[nil (1) (t)]\" t \"#[(q) ((+ q x)) ((x . 5))]\" \"#[nil (x) ((x . 2))]\" \"((0 2))\" t))""##
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// T0.2: an uninitialized `let' variable that is read makes the analysis
/// call `byte-compile-warn-x', void in GNU -Q, on every creation.
#[test]
fn oracle_cconv_memo_warning_error_repeats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun cm-o-w (y) (lambda () (let ((x)) (list x y))))
  (list (condition-case err (cm-o-w 1) (error err))
        (condition-case err (cm-o-w 1) (error err))
        (condition-case err (cm-o-w 1) (error err))
        (featurep 'bytecomp)))
"#;
    let expect = expect_test::expect![[
        r#""OK ((void-function byte-compile-warn-x) (void-function byte-compile-warn-x) (void-function byte-compile-warn-x) nil)""#
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// T0.3: between two creations, a `defvar' makes a binder special, a
/// `defmacro', a compiler macro and an autoloaded macro take the head.
#[test]
fn oracle_cconv_memo_changes_between_creations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun cm-o-f () (let ((a 1)) (lambda () (let ((cm-o-later 2)) (cm-o-g a cm-o-later)))))
  (list (prin1-to-string (cm-o-f)) (prin1-to-string (cm-o-f))
        (progn (defvar cm-o-later 0) (prin1-to-string (cm-o-f)))
        (progn (defmacro cm-o-g (&rest args) `(list ,@args)) (prin1-to-string (cm-o-f)))
        (progn (fmakunbound 'cm-o-g)
               (put 'cm-o-g 'compiler-macro (lambda (form &rest _) (cons 'vector (cdr form))))
               (prin1-to-string (cm-o-f)))
        (progn (put 'cm-o-g 'compiler-macro nil)
               (autoload 'cm-o-g "cm-o-nowhere" nil nil 'macro)
               (condition-case err (prin1-to-string (cm-o-f)) (error (car err))))
        (progn (fmakunbound 'cm-o-g) (prin1-to-string (cm-o-f)))))
"#;
    let expect = expect_test::expect![[
        r##""OK (\"#[nil ((let ((cm-o-later 2)) (cm-o-g a cm-o-later))) ((a . 1))]\" \"#[nil ((let ((cm-o-later 2)) (cm-o-g a cm-o-later))) ((a . 1))]\" \"#[nil ((let ((cm-o-later 2)) (cm-o-g a cm-o-later))) ((a . 1))]\" \"#[nil ((let ((cm-o-later 2)) (list a cm-o-later))) ((a . 1))]\" \"#[nil ((let ((cm-o-later 2)) (vector a cm-o-later))) ((a . 1))]\" file-missing \"#[nil ((let ((cm-o-later 2)) (cm-o-g a cm-o-later))) ((a . 1))]\")""##
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// T0.4: advice on the filter and on the expander runs on every creation.
#[test]
fn oracle_cconv_memo_advice_runs_every_time() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defvar cm-o-n 0)
  (defun cm-o-count (f &rest args) (setq cm-o-n (1+ cm-o-n)) (apply f args))
  (defun cm-o-h () (let ((a 1)) (lambda () a)))
  (cm-o-h) (cm-o-h)
  (advice-add 'cconv-make-interpreted-closure :around #'cm-o-count)
  (cm-o-h) (cm-o-h) (cm-o-h)
  (advice-remove 'cconv-make-interpreted-closure #'cm-o-count)
  (let ((n1 cm-o-n))
    (setq cm-o-n 0)
    (advice-add 'macroexp--expand-all :around #'cm-o-count)
    (cm-o-h) (cm-o-h)
    (advice-remove 'macroexp--expand-all #'cm-o-count)
    (list n1 cm-o-n (prin1-to-string (cm-o-h)) (prin1-to-string (cm-o-h)))))
"#;
    let expect =
        expect_test::expect![[r##""OK (3 6 \"#[nil (a) ((a . 1))]\" \"#[nil (a) ((a . 1))]\")""##]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// T0.5: `lexical-binding' nil in the current buffer (the analysis reads
/// the caller's value).
#[test]
fn oracle_cconv_memo_buffer_lexical_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun cm-o-lb () (let ((a 1)) (lambda () (let ((b 2)) (list a b)))))
  (list (prin1-to-string (cm-o-lb))
        (with-temp-buffer (setq lexical-binding nil) (prin1-to-string (cm-o-lb)))
        (prin1-to-string (cm-o-lb))
        (with-temp-buffer (setq lexical-binding nil) (prin1-to-string (cm-o-lb)))))
"#;
    let expect = expect_test::expect![[
        r##""OK (\"#[nil ((let ((b 2)) (list a b))) ((a . 1))]\" \"#[nil ((let ((b 2)) (list a b))) ((a . 1))]\" \"#[nil ((let ((b 2)) (list a b))) ((a . 1))]\" \"#[nil ((let ((b 2)) (list a b))) ((a . 1))]\")""##
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// T0.6: `:closure-dont-trim-context', interactive forms,
/// `(:documentation FORM)', and a body the expansion rewrites.
#[test]
fn oracle_cconv_memo_special_bodies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun cm-o-6a () (let ((a 1) (b 2)) (lambda () :closure-dont-trim-context a)))
  (defun cm-o-6b () (let ((a 1)) (lambda () (interactive (list a)) a)))
  (defun cm-o-6c () (let ((a 1)) (lambda () (:documentation (format "d%s" a)) a)))
  (defun cm-o-6d () (let ((a 1)) (lambda () (funcall #'(lambda (x) x) a))))
  (defun cm-o-6e () (lambda () :closure-dont-trim-context))
  (let (out)
    (dotimes (_ 3)
      (push (mapcar (lambda (f) (prin1-to-string (funcall f)))
                    '(cm-o-6a cm-o-6b cm-o-6c cm-o-6d cm-o-6e))
            out))
    out))
"#;
    let expect = expect_test::expect![[
        r##""OK ((\"#[nil (a) ((b . 2) (a . 1) t)]\" \"#[nil (a) ((a . 1)) nil nil (list a)]\" \"#[nil (a) ((a . 1)) nil \\\"d1\\\"]\" \"#[nil ((let ((x a)) x)) ((a . 1))]\" \"#[nil (:closure-dont-trim-context) (t)]\") (\"#[nil (a) ((b . 2) (a . 1) t)]\" \"#[nil (a) ((a . 1)) nil nil (list a)]\" \"#[nil (a) ((a . 1)) nil \\\"d1\\\"]\" \"#[nil ((let ((x a)) x)) ((a . 1))]\" \"#[nil (:closure-dont-trim-context) (t)]\") (\"#[nil (a) ((b . 2) (a . 1) t)]\" \"#[nil (a) ((a . 1)) nil nil (list a)]\" \"#[nil (a) ((a . 1)) nil \\\"d1\\\"]\" \"#[nil ((let ((x a)) x)) ((a . 1))]\" \"#[nil (:closure-dont-trim-context) (t)]\"))""##
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// T0.7: the lambda body is mutated between creations.
#[test]
fn oracle_cconv_memo_mutated_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun cm-o-7 () (let ((a 1) (b 2)) (lambda () (list a))))
  (let ((first (prin1-to-string (cm-o-7))))
    (prin1-to-string (cm-o-7))
    (setcar (cdr (car (aref (cm-o-7) 1))) 'b)
    (list first (prin1-to-string (cm-o-7)) (prin1-to-string (cm-o-7)))))
"#;
    let expect = expect_test::expect![[
        r##""OK (\"#[nil ((list a)) ((a . 1))]\" \"#[nil ((list b)) ((b . 2))]\" \"#[nil ((list b)) ((b . 2))]\")""##
    ]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// T0.8: the deepest recursion at which a closure can still be created
/// under a lowered `max-lisp-eval-depth', found by binary search.
#[test]
fn oracle_cconv_memo_depth_threshold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn
  (defun cm-o-deep (n) (if (> n 0) (cm-o-deep (1- n)) (let ((a 1)) (lambda () (list (list a))))))
  (cm-o-deep 0) (cm-o-deep 0)
  (let ((max-lisp-eval-depth 300) (lo 0) (hi 400))
    (while (< lo hi)
      (let ((mid (/ (+ lo hi) 2)))
        (if (condition-case nil (progn (cm-o-deep mid) t) (error nil))
            (setq lo (1+ mid))
          (setq hi mid))))
    lo))
"#;
    let expect = expect_test::expect![[r#""OK 132""#]];
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}
