//! GNU pins for function-cell changes during repeated callbacks.
//!
//! `mapcar1`, `Fmaphash`, and `Fassoc` retain the supplied function and call
//! through `calln` for each element (`src/fns.c`). `Ffuncall` records its
//! frame, runs the entry debugger, and only then resolves the current cell
//! in `funcall_general` (`src/eval.c`). Expectations are refreshed from GNU.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

const JIT_ENV: &[(&str, &str)] = &[("NEOVM_JIT_THRESHOLD", "1")];

/// The current callback finishes using its old body. Following callbacks
/// resolve native, interpreted, and bytecode replacements, while the frame
/// retains the called symbol through redefinition and collection.
#[test]
fn oracle_mapcar_fset_inside_callback_refreshes_native_callee_and_frames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--cr-frames nil)
  (defvar neovm--cr-replacement nil)
  (let ((out nil))
    (dolist (mode '(interpreted bytecode))
      (dolist (replacement-mode '(native interpreted bytecode))
        (setq neovm--cr-replacement
              (cond ((eq replacement-mode 'native) #'1+)
                    ((eq replacement-mode 'interpreted) (lambda (x) (+ x 10)))
                    (t (byte-compile (lambda (x) (+ x 20))))))
        (let ((old (lambda (x)
                     (when (= x 2)
                       (fset 'neovm--cr-f neovm--cr-replacement)
                       (garbage-collect)
                       (setq neovm--cr-frames
                             (list (backtrace-frame 0 'neovm--cr-f)
                                   (backtrace-frame 1 'neovm--cr-f))))
                     x)))
          (fset 'neovm--cr-f (if (eq mode 'bytecode) (byte-compile old) old))
          (push (list mode replacement-mode
                      (mapcar #'neovm--cr-f '(1 2 3 4)) neovm--cr-frames) out))))
    (nreverse out)))"#;
    let expect = expect_test::expect![[
        r#""OK ((interpreted native (1 2 4 5) ((t neovm--cr-f 2) (t mapcar neovm--cr-f (1 2 3 4)))) (interpreted interpreted (1 2 13 14) ((t neovm--cr-f 2) (t mapcar neovm--cr-f (1 2 3 4)))) (interpreted bytecode (1 2 23 24) ((t neovm--cr-f 2) (t mapcar neovm--cr-f (1 2 3 4)))) (bytecode native (1 2 4 5) ((t neovm--cr-f 2) (t mapcar neovm--cr-f (1 2 3 4)))) (bytecode interpreted (1 2 13 14) ((t neovm--cr-f 2) (t mapcar neovm--cr-f (1 2 3 4)))) (bytecode bytecode (1 2 23 24) ((t neovm--cr-f 2) (t mapcar neovm--cr-f (1 2 3 4)))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// Replacing the target of an alias chain must refresh the body while
/// preserving the alias as the named callback in the backtrace.
#[test]
fn oracle_mapcar_defalias_inside_callback_refreshes_alias_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--cr-alias-frames nil)
  (defalias 'neovm--cr-alias 'neovm--cr-alias-target)
  (defalias 'neovm--cr-alias-target
    (byte-compile
     (lambda (x)
       (when (= x 2)
         (defalias 'neovm--cr-alias-target #'1+)
         (setq neovm--cr-alias-frames
               (list (backtrace-frame 0 'neovm--cr-alias)
                     (backtrace-frame 1 'neovm--cr-alias))))
       x)))
  (list (mapcar #'neovm--cr-alias '(1 2 3 4)) neovm--cr-alias-frames))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 4 5) ((t neovm--cr-alias 2) (t mapcar neovm--cr-alias (1 2 3 4))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// Advice installed by the second callback affects the third callback;
/// cancellation by that third callback takes effect before the fourth.
#[test]
fn oracle_mapcar_advice_added_and_removed_inside_callbacks_refreshes_callee() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--cr-advice-filter
    (byte-compile (lambda (x) (+ x 100))))
  (defalias 'neovm--cr-advised
    (byte-compile
     (lambda (x)
       (when (= x 2)
         (advice-add 'neovm--cr-advised :filter-return #'neovm--cr-advice-filter))
       (when (= x 3)
         (advice-remove 'neovm--cr-advised #'neovm--cr-advice-filter))
       x)))
  (mapcar #'neovm--cr-advised '(1 2 3 4)))"#;
    let expect = expect_test::expect![[r#""OK (1 2 103 4)""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

#[test]
fn oracle_mapc_and_mapconcat_refresh_redefined_callbacks_mid_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--cr-mapc-log nil)
  (defalias 'neovm--cr-mapc-new
    (lambda (x) (push (list 'new x) neovm--cr-mapc-log)))
  (defalias 'neovm--cr-mapc-f
    (lambda (x)
      (when (= x 2) (fset 'neovm--cr-mapc-f #'neovm--cr-mapc-new))
      (push (list 'old x) neovm--cr-mapc-log)))
  (defalias 'neovm--cr-concat-f
    (byte-compile
     (lambda (x)
       (when (eq x 'b) (fset 'neovm--cr-concat-f #'symbol-name))
       (concat "old-" (symbol-name x)))))
  (list (mapc #'neovm--cr-mapc-f '(1 2 3 4))
        (nreverse neovm--cr-mapc-log)
        (mapconcat #'neovm--cr-concat-f '(a b c d) ":")))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4) ((old 1) (old 2) (new 3) (new 4)) \"old-a:old-b:c:d\")""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// Count activations rather than depending on hash-table enumeration order.
#[test]
fn oracle_maphash_fset_inside_callback_refreshes_later_activations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--cr-hash-old 0)
  (defvar neovm--cr-hash-new 0)
  (defvar neovm--cr-hash-sum 0)
  (defalias 'neovm--cr-hash-replacement
    (byte-compile
     (lambda (key value)
       (setq neovm--cr-hash-new (1+ neovm--cr-hash-new)
             neovm--cr-hash-sum (+ neovm--cr-hash-sum key value)))))
  (defalias 'neovm--cr-hash-f
    (byte-compile
     (lambda (key value)
       (setq neovm--cr-hash-old (1+ neovm--cr-hash-old)
             neovm--cr-hash-sum (+ neovm--cr-hash-sum key value))
       (fset 'neovm--cr-hash-f #'neovm--cr-hash-replacement))))
  (let ((table (make-hash-table :test 'eql)))
    (puthash 1 10 table)
    (puthash 2 20 table)
    (puthash 3 30 table)
    (puthash 4 40 table)
    (list (maphash #'neovm--cr-hash-f table)
          neovm--cr-hash-old neovm--cr-hash-new neovm--cr-hash-sum)))"#;
    let expect = expect_test::expect![[r#""OK (nil 1 3 110)""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

#[test]
fn oracle_assoc_testfn_fset_inside_callback_refreshes_native_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--cr-assoc-log nil)
  (defalias 'neovm--cr-assoc-f
    (byte-compile
     (lambda (entry-key sought-key)
       (push (list entry-key sought-key) neovm--cr-assoc-log)
       (when (eq entry-key 'c) (fset 'neovm--cr-assoc-f #'eq))
       nil)))
  (list (assoc 'b '((a . 1) (c . 2) (b . 3) (d . 4)) #'neovm--cr-assoc-f)
        (nreverse neovm--cr-assoc-log)))"#;
    let expect = expect_test::expect![[r#""OK ((b . 3) ((a b) (c b)))""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// The first callback uses the canonical native `1+`. Its exit debugger arms
/// the second callback, whose entry debugger replaces that cached native body.
/// Revalidation must follow the funcall prologue; its frame already exists.
#[test]
fn oracle_mapcar_entry_debugger_redefines_cached_native_callee_after_prologue() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--cr-debug-entries 0)
  (defvar neovm--cr-debug-log nil)
  (defalias 'neovm--cr-debug-driver
    (byte-compile
     (lambda ()
       (setq debug-on-next-call t)
       (mapcar #'1+ '(1 2 3 4)))))
  (let ((saved (symbol-function '1+))
        (debugger
         (lambda (&rest args)
           (cond
            ((eq (car args) 'exit)
             (push args neovm--cr-debug-log)
             (if (= neovm--cr-debug-entries 1)
                 (prog1 (cadr args) (setq debug-on-next-call t))
               (cadr args)))
            ((null (backtrace-frame 0 '1+)) (setq debug-on-next-call t))
            (t
             (setq neovm--cr-debug-entries (+ neovm--cr-debug-entries 1))
             (push (list (car args)
                         (backtrace-frame 0 '1+)
                         (backtrace-frame 1 '1+))
                   neovm--cr-debug-log)
             (when (= neovm--cr-debug-entries 2)
               (fset '1+ (symbol-function '1-)))
             nil)))))
    (unwind-protect
        (let ((mapped (neovm--cr-debug-driver)))
          (list mapped (nreverse neovm--cr-debug-log) debug-on-next-call))
      (fset '1+ saved)
      (setq debug-on-next-call nil))))"#;
    let expect = expect_test::expect![[
        r#""OK ((2 1 2 3) ((lambda (t 1+ 1) (t mapcar 1+ (1 2 3 4))) (exit 2) (lambda (t 1+ 2) (t mapcar 1+ (1 2 3 4))) (exit 1) (exit (2 1 2 3))) nil)""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// GNU implements debug-on-entry as advice. Installing it while one callback
/// is live must enter the debugger on the next callback and cancellation must
/// allow the remaining callbacks to continue with their original frames.
#[test]
fn oracle_mapcar_debug_on_entry_installed_and_cancelled_mid_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (require 'debug)
  (defvar neovm--cr-entry-log nil)
  (defalias 'neovm--cr-entry-f
    (byte-compile
     (lambda (x)
       (when (= x 2) (debug-on-entry 'neovm--cr-entry-f))
       x)))
  (let ((debugger
         (lambda (&rest args)
           (push (list (car args)
                       (backtrace-frame 0 'neovm--cr-entry-f)
                       (backtrace-frame 1 'neovm--cr-entry-f))
                 neovm--cr-entry-log)
           (cancel-debug-on-entry 'neovm--cr-entry-f)
           nil)))
    (list (mapcar #'neovm--cr-entry-f '(1 2 3 4))
          (nreverse neovm--cr-entry-log))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4) ((debug (t neovm--cr-entry-f 3) (t mapcar neovm--cr-entry-f (1 2 3 4)))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}
