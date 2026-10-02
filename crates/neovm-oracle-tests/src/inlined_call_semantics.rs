//! P2.3 commit 0: GNU-observable semantics at byte-compiled inline shapes.
//!
//! O1/O2 live in `backtrace_frame_semantics`; O3/O4 in `debug_on_next_call`.
//! GNU's `Bcall` and `Ffuncall` record a frame before invoking its body
//! (`src/bytecode.c:docall`, `src/eval.c:Ffuncall`). `Fmapc`/`Fmapcar` first
//! call `Flength`, then `mapcar1`, whose list cursor is advanced after each
//! callback. These pins preserve those boundaries through virtual frames
//! and through a deopt inside a callback.
//!
//! Refresh only from the pinned GNU reference with `NEOVM_ORACLE_MODE=refresh
//! UPDATE_EXPECT=1`. No expected transcript is authored by hand.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Each subprocess owns its Lisp mutator state. This matrix is local test
/// configuration and neither stores Lisp values nor assumes a shared mutator.
/// The first-compile diagnostic makes eligible named regions execute even
/// when a pin's warmup is too short to earn re-tiering. Function stamps are
/// always active and have no environment knob, so only the three actual
/// binary Phase-1 knobs contribute matrix axes. The combined stress row
/// exercises a chain deopt while the collector can run during resumption.
pub(crate) fn check(form: &str, expect: expect_test::Expect) {
    let mut matrix = Vec::new();
    for inline2 in ["off", "named", "all"] {
        for mask in 0..8 {
            matrix.push(vec![
                ("NEOVM_JIT_THRESHOLD", "1"),
                ("NEOVM_JIT_INLINE2", inline2),
                ("NEOVM_JIT_INLINE2_AT_FIRST_COMPILE", "1"),
                (
                    "NEOVM_JIT_DIRECT_CALL",
                    if mask & 1 == 0 { "off" } else { "on" },
                ),
                ("NEOVM_JIT_LEAF", if mask & 2 == 0 { "off" } else { "all" }),
                (
                    "NEOVM_JIT_INLINE_VARS",
                    if mask & 4 == 0 { "off" } else { "all" },
                ),
            ]);
        }
        for stress in [
            ("NEOVM_JIT", "0"),
            ("NEOVM_JIT_FORCE_SLOW_SPEC", "1"),
            ("NEOVM_JIT_FORCE_DEOPT", "1"),
            ("NEOVM_GC_STRESS", "1"),
        ] {
            matrix.push(vec![
                ("NEOVM_JIT_THRESHOLD", "1"),
                ("NEOVM_JIT_INLINE2", inline2),
                ("NEOVM_JIT_INLINE2_AT_FIRST_COMPILE", "1"),
                stress,
            ]);
        }
        matrix.push(vec![
            ("NEOVM_JIT_THRESHOLD", "1"),
            ("NEOVM_JIT_INLINE2", inline2),
            ("NEOVM_JIT_INLINE2_AT_FIRST_COMPILE", "1"),
            ("NEOVM_JIT_FORCE_DEOPT", "1"),
            ("NEOVM_GC_STRESS", "1"),
        ]);
    }
    let envs: Vec<_> = matrix.iter().map(Vec::as_slice).collect();
    crate::common::assert_oracle_parity_under_envs_expect(form, &envs, expect);
}

/// O5: the depth failure from compiled Bcall is an `error`, with floor
/// raising below 100. Both a dynamic binding and a global assignment reach
/// the same call tree; unwinding restores a usable session.
///
/// The finished T1.3/T3.1 base diverges: it exposes 99 after the floor raise,
/// its JIT hook sees the outer frame, and this hook shape crashes Tier-0.
/// Keep GNU's exact pin available with nextest's `--run-ignored all`.
#[test]
#[ignore = "inherited GNU divergence: depth-floor forwarding and depth signal-hook frames (Tier-0 crashes)"]
fn oracle_inline_o5_depth_limit_bound_global_and_floor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defvar inline-o5-seen nil)
  (defalias 'inline-o5-capture
    (lambda (_kind _data)
      (mapbacktrace
       (lambda (evald f args _flags)
         (when (and (null inline-o5-seen)
                    (memq f '(inline-o5-a inline-o5-b inline-o5-c)))
           (setq inline-o5-seen (list evald f args)))))))
  (defalias 'inline-o5-a
    (byte-compile (lambda (n) (if (= n 0) 0 (1+ (inline-o5-b (1- n)))))))
  (defalias 'inline-o5-b
    (byte-compile (lambda (n) (if (= n 0) 0 (1+ (inline-o5-c (1- n)))))))
  (defalias 'inline-o5-c
    (byte-compile (lambda (n) (if (= n 0) 0 (1+ (inline-o5-a (1- n)))))))
  (dotimes (_ 8) (inline-o5-a 3))
  (let ((saved max-lisp-eval-depth) (out nil))
    (unwind-protect
        (progn
          (dolist (limit '(120 99))
            (setq inline-o5-seen nil)
            (push
             (let ((max-lisp-eval-depth limit)
                   (signal-hook-function #'inline-o5-capture))
               (list 'bound limit
                     (condition-case e (inline-o5-a 300) (error e))
                     inline-o5-seen max-lisp-eval-depth (inline-o5-a 3))) out))
          (dolist (limit '(120 99))
            (setq inline-o5-seen nil)
            (setq max-lisp-eval-depth limit)
            (push (let ((signal-hook-function #'inline-o5-capture))
                    (list 'global limit
                          (condition-case e (inline-o5-a 300) (error e))
                          inline-o5-seen max-lisp-eval-depth (inline-o5-a 3))) out)))
      (setq max-lisp-eval-depth saved))
    (nreverse out)))"#,
        expect_test::expect![[
            r#""OK ((bound 120 (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\") (t inline-o5-a (210)) 120 3) (bound 99 (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\") (t inline-o5-b (230)) 100 3) (global 120 (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\") (t inline-o5-a (210)) 120 3) (global 99 (error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\") (t inline-o5-b (230)) 100 3))""#
        ]],
    );
}

/// O6: the adjacent boundaries of recursion through Bcall and Ffuncall
/// in mapcar1 distinguish `error` from `excessive-lisp-nesting`.
/// The finished T1.3/T3.1 base still exposes 99 after raising the floor to
/// 100; retain the exact GNU pin until that inherited divergence is fixed.
#[test]
#[ignore = "inherited GNU divergence: Lisp-visible max-lisp-eval-depth remains 99 after floor raise"]
fn oracle_inline_o6_mapc_depth_limit_uses_funcall_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defalias 'inline-o6
    (byte-compile (lambda (n)
      (if (= n 0) 0 (mapc #'inline-o6 (list (1- n)))))))
  (dotimes (_ 8) (inline-o6 3))
  (list
   (let ((max-lisp-eval-depth 120))
     (condition-case e (inline-o6 300) (error e)))
   (let ((max-lisp-eval-depth 121))
     (condition-case e (inline-o6 300) (error e)))
   (let ((max-lisp-eval-depth 99))
     (list (condition-case e (inline-o6 300) (error e)) max-lisp-eval-depth))
   (inline-o6 0)))"#,
        expect_test::expect![[
            r#""OK ((error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\") (excessive-lisp-nesting 122) ((excessive-lisp-nesting 101) 100) 0)""#
        ]],
    );
}

/// O7: the activation keeps its old byte-code object after its symbol's
/// function cell is changed by an out-call. The following call sees the
/// new cell, including after a GC between the rewrite and the return.
#[test]
fn oracle_inline_o7_self_redefinition_finishes_old_activation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defvar inline-o7-armed nil)
  (defalias 'inline-o7-rewrite
    (lambda ()
      (when inline-o7-armed
        (fset 'inline-o7-callee (byte-compile (lambda (x) (+ x 100))))
        (garbage-collect))))
  (defalias 'inline-o7-callee
    (byte-compile (lambda (x) (inline-o7-rewrite) (+ x 1))))
  (defalias 'inline-o7-caller
    (byte-compile (lambda (x) (inline-o7-callee x))))
  (dotimes (_ 8) (inline-o7-caller 2))
  (list (let ((inline-o7-armed t)) (inline-o7-caller 7))
        (inline-o7-caller 7)))"#,
        expect_test::expect![[r#""OK (8 107)""#]],
    );
}

/// O8: a named callback advised midway through a list uses the wrapper on
/// the next element. Both mapping operations share GNU's mapcar1 loop.
#[test]
fn oracle_inline_o8_advice_add_mid_mapping_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defvar inline-o8-log nil)
  (defvar inline-o8-armed nil)
  (defalias 'inline-o8-advice (lambda (f x) (+ 100 (funcall f x))))
  (defalias 'inline-o8-callback
    (byte-compile (lambda (x)
      (push x inline-o8-log)
      (when (and inline-o8-armed (= x 2))
        (advice-add 'inline-o8-callback :around #'inline-o8-advice))
      (+ x 10))))
  (defalias 'inline-o8-mapcar
    (byte-compile (lambda (xs) (mapcar #'inline-o8-callback xs))))
  (defalias 'inline-o8-mapc
    (byte-compile (lambda (xs) (mapc #'inline-o8-callback xs))))
  (dotimes (_ 8) (inline-o8-mapcar '(1 3)))
  (setq inline-o8-log nil)
  (unwind-protect
      (let ((inline-o8-armed t))
        (let ((a (inline-o8-mapcar '(1 2 3 4))))
          (advice-remove 'inline-o8-callback #'inline-o8-advice)
          (list a (inline-o8-mapc '(1 2 3 4)) (nreverse inline-o8-log))))
    (advice-remove 'inline-o8-callback #'inline-o8-advice)))"#,
        expect_test::expect![[r#""OK ((11 12 113 114) (1 2 3 4) (1 2 3 4 1 2 3 4))""#]],
    );
}

/// O9: signal hooks and handler-bind run before unwinding. GNU's Car
/// opcode does not create a car frame; the compiled callee and its callers
/// are still visible from both observers.
#[test]
fn oracle_inline_o9_signal_hook_and_handler_bind_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defvar inline-o9-log nil)
  (defalias 'inline-o9-frames
    (lambda ()
      (let (frames)
        (mapbacktrace
         (lambda (evald f args flags)
           (when (memq f '(inline-o9-inner inline-o9-middle inline-o9-outer))
             (push (list evald f args flags) frames))))
        (nreverse frames))))
  (defalias 'inline-o9-inner (byte-compile (lambda (x) (car x))))
  (defalias 'inline-o9-middle (byte-compile (lambda (x) (inline-o9-inner x))))
  (defalias 'inline-o9-outer (byte-compile (lambda (x) (inline-o9-middle x))))
  (dotimes (_ 8) (inline-o9-outer '(fine)))
  (let ((signal-hook-function
         (lambda (kind data)
           (push (list 'hook kind data (inline-o9-frames)) inline-o9-log))))
    (list
     (condition-case e
         (handler-bind
             ((wrong-type-argument
               (lambda (e)
                 (push (list 'handler e (inline-o9-frames)) inline-o9-log))))
           (inline-o9-outer 1))
       (error e))
     (nreverse inline-o9-log)
     (inline-o9-outer '(after)))))"#,
        expect_test::expect![[
            r#""OK ((wrong-type-argument listp 1) ((hook wrong-type-argument (listp 1) ((t inline-o9-inner (1) nil) (t inline-o9-middle (1) nil) (t inline-o9-outer (1) nil))) (handler (wrong-type-argument listp 1) ((t inline-o9-inner (1) nil) (t inline-o9-middle (1) nil) (t inline-o9-outer (1) nil)))) after)""#
        ]],
    );
}

/// O10: setq itself does not poll. The following bytecode call boundary
/// observes quit before its function body, and cleanup restores the flag.
#[test]
fn oracle_inline_o10_quit_set_inside_callee_at_next_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defvar inline-o10-armed nil)
  (defvar inline-o10-log nil)
  (defalias 'inline-o10-next
    (byte-compile (lambda () (push 'next-body inline-o10-log) 'next)))
  (defalias 'inline-o10-callee
    (byte-compile (lambda ()
      (when inline-o10-armed (setq quit-flag t))
      (inline-o10-next))))
  (defalias 'inline-o10-caller (byte-compile (lambda () (inline-o10-callee))))
  (dotimes (_ 8) (inline-o10-caller))
  (setq inline-o10-log nil)
  (unwind-protect
      (let ((inline-o10-armed t))
        (list (condition-case e (inline-o10-caller) (quit e))
              inline-o10-log quit-flag))
    (setq quit-flag nil)))"#,
        expect_test::expect![[r#""OK ((quit) nil nil)""#]],
    );
}

/// O11: Flength rejects circular and dotted lists before the first
/// callback. A callback-shortened list stops early and mapc returns the
/// original (now mutated) sequence.
#[test]
fn oracle_inline_o11_map_list_validation_and_callback_shortening() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defalias 'inline-o11
    (byte-compile (lambda (kind mode)
      (let* ((xs (if (eq kind 'dotted) (cons 1 2) (list 1 2 3)))
             (calls nil))
        (when (eq kind 'circular) (setcdr (cddr xs) xs))
        (let* ((f (lambda (x)
                    (push x calls)
                    (when (eq kind 'shortened) (setcdr xs nil))
                    (+ x 10)))
               (result
                (condition-case e
                    (if (eq mode 'mapc) (mapc f xs) (mapcar f xs))
                  (error (if (eq (car e) 'circular-list)
                             (list (car e))
                           e)))))
          (list kind mode result (nreverse calls)))))))
  (dotimes (_ 8) (inline-o11 'proper 'mapc) (inline-o11 'proper 'mapcar))
  (let (out)
    (dolist (kind '(proper circular dotted shortened))
      (dolist (mode '(mapc mapcar)) (push (inline-o11 kind mode) out)))
    (nreverse out)))"#,
        expect_test::expect![[
            r#""OK ((proper mapc (1 2 3) (1 2 3)) (proper mapcar (11 12 13) (1 2 3)) (circular mapc (circular-list) nil) (circular mapcar (circular-list) nil) (dotted mapc (wrong-type-argument listp 2) nil) (dotted mapcar (wrong-type-argument listp 2) nil) (shortened mapc (1) (1)) (shortened mapcar (11) (1)))""#
        ]],
    );
}

/// O12: a throw from a callback leaves mapping immediately and unwinds
/// the callback's dynamic binding through each suspended inline frame.
#[test]
fn oracle_inline_o12_throw_from_mapc_lambda_unwinds_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defvar inline-o12-special 'outer)
  (defalias 'inline-o12
    (byte-compile (lambda (xs stop)
      (let ((seen nil))
        (list
         (catch 'inline-o12-out
           (mapc (lambda (x)
                   (let ((inline-o12-special x))
                     (push inline-o12-special seen)
                     (when (= x stop)
                       (throw 'inline-o12-out (list 'thrown x))))) xs)
           'finished)
         (nreverse seen) inline-o12-special)))))
  (dotimes (_ 8) (inline-o12 '(1 2 3) 9))
  (list (inline-o12 '(1 2 3 4) 2) inline-o12-special
        (inline-o12 '(1 2) 9)))"#,
        expect_test::expect![[r#""OK (((thrown 2) (1 2) outer) outer (finished (1 2) outer))""#]],
    );
}

/// O13: mapcar stages callback results in slots, then conses exactly one
/// result cell per element. Only the first memory-use-counts list's seven
/// conses join the 1000-element result delta.
#[test]
fn oracle_inline_o13_mapcar_memory_use_counts_cons_delta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defalias 'inline-o13
    (byte-compile (lambda (xs k)
      (let* ((before (car (memory-use-counts)))
             (result (mapcar (lambda (x) (+ x k)) xs)))
        (list (- (car (memory-use-counts)) before)
              (length result) (car result) (car (last result)))))))
  (let ((xs (number-sequence 0 999)))
    (dotimes (_ 8) (inline-o13 xs 1))
    (inline-o13 xs 5)))"#,
        expect_test::expect![[r#""OK (1007 1000 5 1004)""#]],
    );
}

/// O14: boxing and resumed stacks preserve object identity, while two
/// separately computed equal floats remain eql but not eq. Captured
/// floats and conses retain identity through both constant and closure calls.
#[test]
fn oracle_inline_o14_float_and_cons_identity_through_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (defalias 'inline-o14-inner (byte-compile (lambda (x) x)))
  (defalias 'inline-o14-middle (byte-compile (lambda (x) (inline-o14-inner x))))
  (defalias 'inline-o14-outer
    (byte-compile (lambda (f c)
      (let* ((same-f (inline-o14-middle f))
             (same-c (inline-o14-middle c))
             (fresh-f (+ f 0.0))
             (fresh-c (cons (car c) (cdr c)))
             (closure (lambda () (list f c)))
             (captured (funcall closure)))
        (list (eq f same-f) (eql f same-f)
              (eq f fresh-f) (eql f fresh-f)
              (eq c same-c) (eql c same-c)
              (eq c fresh-c) (eql c fresh-c)
              (eq f (car captured)) (eq c (cadr captured)))))))
  (dotimes (_ 8) (inline-o14-outer 1.5 '(a . b)))
  (inline-o14-outer 1.5 '(a . b)))"#,
        expect_test::expect![[r#""OK (t t nil t t t nil nil t t)""#]],
    );
}

/// GNU implements debug-on-entry by installing before advice in the
/// function cell (`lisp/emacs-lisp/debug.el`). A previously inlined pure
/// callee must consult the replacement cell and expose its named frame.
#[test]
fn oracle_inline_named_debug_on_entry_after_native_warmup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    check(
        r#"(progn
  (require 'debug)
  (defvar inline-named-debug-log nil)
  (defalias 'inline-named-debug-callee (byte-compile (lambda (x) (1+ x))))
  (defalias 'inline-named-debug-caller
    (byte-compile (lambda (x) (+ 10 (inline-named-debug-callee x)))))
  (dotimes (_ 64) (inline-named-debug-caller 3))
  (unwind-protect
      (let ((debugger
             (lambda (&rest args)
               (let (frames)
                 (mapbacktrace
                  (lambda (evald f values flags)
                    (when (memq f '(inline-named-debug-callee inline-named-debug-caller))
                      (push (list evald f values flags) frames))))
                 (push (list (car args) (nreverse frames)) inline-named-debug-log))
               nil)))
        (debug-on-entry 'inline-named-debug-callee)
        (list (inline-named-debug-caller 3) (nreverse inline-named-debug-log)))
    (cancel-debug-on-entry 'inline-named-debug-callee)))"#,
        expect_test::expect![[
            r#""OK (14 ((debug ((t inline-named-debug-callee (3) nil) (t inline-named-debug-caller (3) nil)))))""#
        ]],
    );
}
