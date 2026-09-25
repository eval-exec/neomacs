//! Oracle pins for Tier-I fallback B (P4.2 Part A, U3.9): hot interpreted
//! closures run through the closure-compiled interpreter
//! (`NEOVM_TIER_I`), which must keep every GNU-observable behaviour of the
//! tree walker.  The O-suite of the design (`p4-2-closure-tierup-aot` §9):
//! each form calls its closures before observing, and runs under a knob
//! matrix -- off (the tree walker), on at threshold 1 (every body compiled at
//! its first call), verify at threshold 1 (a balance check after every
//! compiled form, aborting on a mismatch), and on together with the cconv
//! memo.
//!
//! O19 (the GC residue of the last completed call) is not pinned against GNU:
//! GNU's conservative stack scan makes it nondeterministic.  The in-process
//! tests (`tier_i/tests`) run the residue helpers unchanged.
//!
//! Regenerate the expectations from GNU only:
//! `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1 cargo nextest run -p neovm-oracle-tests -E 'test(/closure::tier_i/)'`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// The Neomacs configurations every form must answer identically under.
const KNOB_MATRIX: &[&[(&str, &str)]] = &[
    &[],
    &[("NEOVM_TIER_I", "on"), ("NEOVM_TIER_I_THRESHOLD", "1")],
    &[("NEOVM_TIER_I", "verify"), ("NEOVM_TIER_I_THRESHOLD", "1")],
    &[
        ("NEOVM_TIER_I", "on"),
        ("NEOVM_TIER_I_THRESHOLD", "2"),
        ("NEOVM_CCONV_MEMO", "on"),
        ("NEOVM_CCONV_FAST", "on"),
    ],
];

fn check(form: &str, expect: expect_test::Expect) {
    crate::common::assert_oracle_parity_under_envs_expect(form, KNOB_MATRIX, expect);
}

/// O1: `mapbacktrace` frames (evaluated flag, function, argument count)
/// inside `let`/`if`/`progn`/a call.
#[test]
fn oracle_tier_i_o1_mapbacktrace_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o1-probe ()
    (let (frames)
      (mapbacktrace (lambda (evald fun args _flags)
                      (push (list evald fun (if (listp args) (length args) args)) frames))
                    #'ti-o1-probe)
      (let ((n 0) (keep nil))
        (dolist (f (nreverse frames))
          (when (< n 7) (push f keep))
          (setq n (1+ n)))
        (nreverse keep))))
  (defun ti-o1 (x)
    (let ((y (1+ x)))
      (if (> y 0) (progn (list (ti-o1-probe) y)) 'neg)))
  (list (ti-o1 1) (ti-o1 2) (ti-o1 3)))
"#,
        expect_test::expect![[
            r#""OK ((((t ti-o1-probe 0) (nil list 2) (nil progn 1) (nil if 3) (nil let 2) (t ti-o1 1) (nil list 3)) 2) (((t ti-o1-probe 0) (nil list 2) (nil progn 1) (nil if 3) (nil let 2) (t ti-o1 1) (nil list 3)) 3) (((t ti-o1-probe 0) (nil list 2) (nil progn 1) (nil if 3) (nil let 2) (t ti-o1 1) (nil list 3)) 4))""#
        ]],
    );
}

/// O2: a signal inside a leaf primitive seen by `handler-bind` and
/// `backtrace-frames`.
#[test]
fn oracle_tier_i_o2_handler_bind_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o2 (x)
    (let ((seen nil))
      (condition-case nil
          (handler-bind ((wrong-type-argument
                          (lambda (_e)
                            (setq seen (mapcar (lambda (f)
                                                 (list (car f)
                                                       (if (symbolp (cadr f)) (cadr f) 'fn)))
                                               (seq-take (backtrace-frames) 6))))))
            (let ((y x)) (if y (car y) 'none)))
        (error nil))
      seen))
  (list (ti-o2 1) (ti-o2 2)))
"#,
        expect_test::expect![[
            r#""OK (((t backtrace-frames) (nil seq-take) (nil mapcar) (nil setq) (t fn) (t car)) ((t backtrace-frames) (nil seq-take) (nil mapcar) (nil setq) (t fn) (t car)))""#
        ]],
    );
}

/// O3: `called-interactively-p` counts the special-form frames of
/// interpreted code.
#[test]
fn oracle_tier_i_o3_called_interactively_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o3 (x)
    (interactive (list 1))
    (let ((y x)) (if y (list y (called-interactively-p 'any)) 'none)))
  (list (ti-o3 1) (ti-o3 2) (call-interactively #'ti-o3) (call-interactively #'ti-o3)))
"#,
        expect_test::expect![[r#""OK ((1 nil) (2 nil) (1 t) (1 t))""#]],
    );
}

/// O4: `backtrace-frame` with BASE, and the frames' evaluated arguments.
#[test]
fn oracle_tier_i_o4_backtrace_frame_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o4 (a)
    (let ((z (* a 10)))
      (list (backtrace-frame 0 #'ti-o4) (backtrace-frame 1 #'ti-o4) z)))
  (list (ti-o4 5) (ti-o4 6)))
"#,
        expect_test::expect![[
            r#""OK (((t ti-o4 5) (nil list (ti-o4 5) (ti-o4 6)) 50) ((t ti-o4 6) (nil list (ti-o4 5) (ti-o4 6)) 60))""#
        ]],
    );
}

/// O5: `debug-on-exit` set with `backtrace-debug` on a compiled form's
/// frame, and the value the debugger returns for it.
#[test]
fn oracle_tier_i_o5_backtrace_debug_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defvar ti-o5-log nil)
  (defun ti-o5-probe () (backtrace-debug 1 t #'ti-o5-probe) 'probe)
  (defun ti-o5 (x) (let ((y (list x (ti-o5-probe)))) (list 'result y)))
  (let ((debugger (lambda (&rest args)
                    (push args ti-o5-log)
                    (if (eq (car args) 'exit) (list 'replaced (cadr args)) nil))))
    (list (ti-o5 1) (ti-o5 2) (reverse ti-o5-log))))
"#,
        expect_test::expect![[
            r#""OK ((result (replaced (1 probe))) (result (replaced (2 probe))) ((exit (1 probe)) (exit (2 probe))))""#
        ]],
    );
}

/// O6: `debug-on-next-call` armed from `signal-hook-function` and from the
/// body itself.
#[test]
fn oracle_tier_i_o6_debug_on_next_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defvar ti-o6-log nil)
  (defun ti-o6-body (x) (let ((y (* x 2))) (list x y)))
  (defun ti-o6 ()
    (let ((debugger (lambda (&rest args) (push (car args) ti-o6-log) nil)))
      (list (progn (setq debug-on-next-call t) (ti-o6-body 3))
            (let ((signal-hook-function (lambda (_e _d) (setq debug-on-next-call t))))
              (condition-case e (ti-o6-body 'a) (error (car e))))
            (ti-o6-body 4))))
  (list (ti-o6) (ti-o6) (reverse ti-o6-log)))
"#,
        expect_test::expect![[
            r#""OK ((nil nil (4 8)) (nil nil (4 8)) (t exit t exit t exit t exit))""#
        ]],
    );
}

/// O7: the point where `max-lisp-eval-depth` overflows and the error data,
/// at 400 and at 99 (raised to 100).
#[test]
fn oracle_tier_i_o7_eval_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o7 (k) (if (> k 0) (let ((r (ti-o7 (1- k)))) (1+ r)) 0))
  (ti-o7 3)
  (list
   (let ((max-lisp-eval-depth 400) (lo 0) (hi 400))
     (while (< lo hi)
       (let ((mid (/ (+ lo hi) 2)))
         (if (condition-case nil (progn (ti-o7 mid) t) (error nil))
             (setq lo (1+ mid))
           (setq hi mid))))
     lo)
   (let ((max-lisp-eval-depth 400)) (condition-case e (ti-o7 400) (error e)))
   (let ((max-lisp-eval-depth 99)) (condition-case e (ti-o7 200) (error e)))))
"#,
        expect_test::expect![[
            r#""OK (125 (excessive-lisp-nesting 401) (excessive-lisp-nesting 101))""#
        ]],
    );
}

/// O8: `quit-flag` set by Lisp fires at the next form, inside the handler.
#[test]
fn oracle_tier_i_o8_quit_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o8 (x)
    (condition-case e
        (let ((a x)) (setq quit-flag t) (list 'after a))
      (quit (list 'quit-caught e))))
  (list (ti-o8 1) (ti-o8 2) quit-flag))
"#,
        expect_test::expect![[r#""OK ((quit-caught (quit)) (quit-caught (quit)) nil)""#]],
    );
}

/// O9: a macro in a compiled body expands at every evaluation, and a head
/// that becomes a macro after tiering (or stops being one) is honoured.
#[test]
fn oracle_tier_i_o9_macro_timing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defvar ti-o9-n 0)
  (defmacro ti-o9-mac (x) (setq ti-o9-n (1+ ti-o9-n)) `(list ,x ,ti-o9-n))
  (let ((internal-make-interpreted-closure-function nil))
    (defun ti-o9-use (n) (let ((r nil)) (dotimes (i n) (push (ti-o9-mac i) r)) r))
    (defun ti-o9-later (x) (ti-o9-head x)))
  (defun ti-o9-head (x) (list 'fn x))
  (list (ti-o9-use 3) (ti-o9-use 2) ti-o9-n
        (ti-o9-later 1) (ti-o9-later 2)
        (progn (defmacro ti-o9-head (x) `(list 'mac ,x)) (ti-o9-later 3))
        (progn (fset 'ti-o9-head (lambda (x) (list 'fn-again x))) (ti-o9-later 4))))
"#,
        expect_test::expect![[
            r#""OK (((2 3) (1 2) (0 1)) ((1 5) (0 4)) 5 (fn 1) (fn 2) (mac 3) (fn-again 4))""#
        ]],
    );
}

/// O10: a `defvar` after tiering makes a compiled `let` dynamic.
#[test]
fn oracle_tier_i_o10_defvar_after_tiering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o10-reader () (if (boundp 'ti-o10-v) ti-o10-v 'unbound))
  (defun ti-o10 (x) (let ((ti-o10-v x)) (list (ti-o10-reader) ti-o10-v)))
  (list (ti-o10 1) (ti-o10 2)
        (progn (defvar ti-o10-v 'global) (ti-o10 3))
        ti-o10-v))
"#,
        expect_test::expect![[r#""OK ((unbound 1) (unbound 2) (3 3) global)""#]],
    );
}

/// O11: a dynamic `let` does not shadow a lexical binding of the same
/// symbol; a local `defvar` makes later bindings dynamic; `setq` finds the
/// lexical cell first.
#[test]
fn oracle_tier_i_o11_shadowing_quirk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o11-read () (if (boundp 'ti-o11-x) ti-o11-x 'unbound))
  (defun ti-o11 (v)
    (let ((ti-o11-x v))
      (defvar ti-o11-x)
      (let ((ti-o11-x (* 100 v)))
        (setq ti-o11-x (1+ ti-o11-x))
        (list ti-o11-x (ti-o11-read)))))
  (list (ti-o11 1) (ti-o11 2)))
"#,
        expect_test::expect![[r#""OK ((2 100) (3 200))""#]],
    );
}

/// O12: closures made by compiled code: environment contents, order,
/// sharing of binding cells, and mutation through a captured cell.
#[test]
fn oracle_tier_i_o12_closure_environments() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o12-counter ()
    (let ((n 0) (unused 'u))
      (list (lambda () (setq n (1+ n))) (lambda () n))))
  (defun ti-o12-shadow (x)
    (let ((f (lambda () x)))
      (let ((x (1+ x)))
        (let* ((x (* x 10)) (g (lambda () x)))
          (setq x (1+ x))
          (list x (funcall f) (funcall g))))))
  (let* ((c (ti-o12-counter)) (inc (car c)) (get (cadr c)))
    (funcall inc) (funcall inc)
    (list (funcall get) (prin1-to-string inc) (prin1-to-string get)
          (eq (car (aref inc 2)) (car (aref get 2)))
          (ti-o12-shadow 1) (ti-o12-shadow 2))))
"#,
        expect_test::expect![[
            r##""OK (2 \"#[nil ((setq n (1+ n))) ((n . 2))]\" \"#[nil (n) ((n . 2))]\" t (21 1 21) (31 2 31))""##
        ]],
    );
}

/// O13: `internal-make-interpreted-closure-function` rebound to nil and to a
/// counting wrapper, and `:closure-dont-trim-context`.
#[test]
fn oracle_tier_i_o13_closure_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defvar ti-o13-n 0)
  (defun ti-o13 (x) (let ((y 2) (z 3)) (lambda () (list x y))))
  (defun ti-o13-keep (x) (let ((y 2)) (lambda () :closure-dont-trim-context x)))
  (list (prin1-to-string (ti-o13 1))
        (let ((internal-make-interpreted-closure-function nil))
          (prin1-to-string (ti-o13 1)))
        (let ((internal-make-interpreted-closure-function
               (let ((orig internal-make-interpreted-closure-function))
                 (lambda (&rest args) (setq ti-o13-n (1+ ti-o13-n)) (apply orig args)))))
          (ti-o13 1) (ti-o13 2) ti-o13-n)
        (prin1-to-string (ti-o13-keep 1))
        (prin1-to-string (ti-o13-keep 1))))
"#,
        expect_test::expect![[
            r##""OK (\"#[nil ((list x y)) ((y . 2) (x . 1))]\" \"#[nil ((list x y)) ((z . 3) (y . 2) (x . 1) t)]\" 2 \"#[nil (x) ((y . 2) (x . 1) t)]\" \"#[nil (x) ((y . 2) (x . 1) t)]\")""##
        ]],
    );
}

/// O14: arity errors of a tiered closure carry the closure object.
#[test]
fn oracle_tier_i_o14_arity_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o14 (a b) (let ((c a)) (list c b)))
  (list (ti-o14 1 2)
        (condition-case e (ti-o14 1) (error (list (car e) (interpreted-function-p (cadr e)) (caddr e))))
        (condition-case e (funcall #'ti-o14 1 2 3) (error (list (car e) (caddr e))))
        (condition-case e (apply #'ti-o14 '(1)) (error (car e)))))
"#,
        expect_test::expect![[
            r#""OK ((1 2) (wrong-number-of-arguments t 1) (wrong-number-of-arguments 3) wrong-number-of-arguments)""#
        ]],
    );
}

/// O15: a redefined or advised function (and primitive) after tiering.
#[test]
fn oracle_tier_i_o15_advice_and_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defvar ti-o15-n 0)
  (defun ti-o15-callee (x) (list 'v1 x))
  (defun ti-o15 (x) (let ((y x)) (list (ti-o15-callee y) (car (list y)))))
  (let ((r1 (list (ti-o15 1) (ti-o15 2))))
    (defun ti-o15-callee (x) (list 'v2 x))
    (let ((r2 (ti-o15 3)))
      (advice-add 'ti-o15-callee :around (lambda (f &rest a) (list 'adv (apply f a))))
      (advice-add 'car :around (lambda (f &rest a) (setq ti-o15-n (1+ ti-o15-n)) (apply f a)))
      (let ((r3 (ti-o15 4)))
        (advice-remove 'car (lambda (f &rest a) (setq ti-o15-n (1+ ti-o15-n)) (apply f a)))
        (list r1 r2 r3 (> ti-o15-n 0))))))
"#,
        expect_test::expect![[r#""OK ((((v1 1) 1) ((v1 2) 2)) ((v2 3) 3) ((adv (v2 4)) 4) t)""#]],
    );
}

/// O16: `setcar` on a compiled body's code after tiering.
#[test]
fn oracle_tier_i_o16_self_modifying_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o16 () (let ((a 1) (b 2)) (list a b 'orig)))
  (ti-o16) (ti-o16)
  (let* ((body (aref (symbol-function 'ti-o16) 1))
         (letform (car body))
         (listform (nth 2 letform)))
    (setcar (cdr (nth 3 listform)) 'changed)
    (list (ti-o16)
          (progn (setcar (cdr listform) 'b) (ti-o16))
          (progn (setcar (car (nth 1 letform)) 'b) (ti-o16))
          (progn (setcdr (cdr listform) nil) (ti-o16)))))
"#,
        expect_test::expect![[r#""OK ((1 2 changed) (2 2 changed) (2 2 changed) (2))""#]],
    );
}

/// O17: unwind forms see a variable the body mutated, on a normal exit, a
/// throw and an error.
#[test]
fn oracle_tier_i_o17_unwind_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defvar ti-o17-log nil)
  (defun ti-o17 (mode)
    (let ((x 0))
      (catch 'out
        (unwind-protect
            (progn (setq x 1)
                   (cond ((eq mode 'throw) (throw 'out (list 'thrown x)))
                         ((eq mode 'error) (car 'bad))
                         (t (setq x 2) (list 'normal x))))
          (push (list mode x) ti-o17-log)))))
  (list (ti-o17 'throw) (ti-o17 'normal)
        (condition-case e (ti-o17 'error) (error (car e)))
        (reverse ti-o17-log)))
"#,
        expect_test::expect![[
            r#""OK ((thrown 1) (normal 2) wrong-type-argument ((throw 1) (normal 2) (error 1)))""#
        ]],
    );
}

/// O18: the `condition-case` variable and `:success`.  (A special variable
/// as the handler variable is left out: GNU binds it lexically whenever the
/// environment is lexical (`internal_lisp_condition_case`, eval.c:1676),
/// the tree walker binds it dynamically -- a pre-existing divergence that
/// Tier-I inherits unchanged, since it runs `run_condition_case_body`.)
#[test]
fn oracle_tier_i_o18_condition_case_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o18 (k)
    (list
     (condition-case err (if (= k 0) (car 1) 'fine)
       (error (list 'lex err (funcall (lambda () (car err)))))
       (:success (list 'ok err)))
     (condition-case nil (/ 1 k)
       (arith-error 'no-var)
       (:success 'ok))))
  (list (ti-o18 0) (ti-o18 1) (ti-o18 0)))
"#,
        expect_test::expect![[
            r#""OK (((lex (wrong-type-argument listp 1) wrong-type-argument) no-var) ((ok fine) ok) ((lex (wrong-type-argument listp 1) wrong-type-argument) no-var))""#
        ]],
    );
}

/// O20: a tiered closure stays an interpreted function.  (Two pre-existing
/// divergences of the tree walker are left out: GNU's closures made twice
/// from one `defun` body are not `eq` in their code slot while neomacs's
/// are, and neomacs's `sxhash-equal` of two `equal` closures differs.)
#[test]
fn oracle_tier_i_o20_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o20 (a b) (lambda (c) (list a b c)))
  (ti-o20 1 2) (ti-o20 1 2)
  (let ((f (ti-o20 1 2)) (g (ti-o20 3 4)) (d (symbol-function 'ti-o20)))
    (list (interpreted-function-p d) (byte-code-function-p d) (func-arity d)
          (prin1-to-string f) (funcall f 5) (funcall g 6)
          (equal f (ti-o20 1 2)))))
"#,
        expect_test::expect![[
            r##""OK (t nil (2 . 2) \"#[(c) ((list a b c)) ((b . 2) (a . 1))]\" (1 2 5) (3 4 6) t)""##
        ]],
    );
}

/// O21: a thread, then a hot closure.
#[test]
fn oracle_tier_i_o21_threads() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"
(progn
  (defun ti-o21 (x) (let ((y x)) (* y 2)))
  (list (ti-o21 1) (ti-o21 2)
        (thread-join (make-thread (lambda () (ti-o21 3))))
        (ti-o21 4)))
"#,
        expect_test::expect![[r#""OK (2 4 6 8)""#]],
    );
}
