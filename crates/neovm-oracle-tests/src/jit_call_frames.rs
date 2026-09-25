//! Oracle parity for calls between compiled leaves (design
//! `p1-1-direct-native-calls` §10): every form byte-compiles its functions
//! and runs under `NEOVM_JIT_THRESHOLD=1`, so they are native from their
//! first call and their calls to each other take the JIT's speculated call
//! path (GNU ignores the variable).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Every function compiled at its first call.
const JIT_ENV: &[(&str, &str)] = &[("NEOVM_JIT_THRESHOLD", "1")];

/// O6: a byte-compiled recursion under an unbounded `max-lisp-eval-depth`
/// ends in GNU's `setup_frame` error, "Bytecode stack overflow"
/// (src/bytecode.c:514-515), not in a crash, and the session runs on. The
/// depth differs (GNU's bytecode stack against the native one); the error
/// does not.
#[test]
fn oracle_jit_deep_recursion_signals_bytecode_stack_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--o6-deep
    (byte-compile (lambda (n) (if (= n 0) 0 (1+ (neovm--o6-deep (1- n)))))))
  (dotimes (_ 60) (neovm--o6-deep 100))
  (list (let ((max-lisp-eval-depth most-positive-fixnum))
          (condition-case err (neovm--o6-deep 100000000) (error err)))
        (neovm--o6-deep 500)))"#;
    let expect = expect_test::expect![[r#""OK ((error \"Bytecode stack overflow\") 500)""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// O1: `mapbacktrace` and `backtrace-frame` inside a chain of compiled
/// callers show the called SYMBOLS with the calls' own arguments -- one,
/// two and three of them -- as GNU's `Bcall` records them
/// (`record_in_backtrace (call_fun = TOP)`, src/bytecode.c:792-796), not
/// the byte-code objects the symbols name; `backtrace-frame` finds a frame
/// by its function's symbol.
#[test]
fn oracle_jit_speculated_call_frames_record_the_called_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--o1-seen nil)
  (defalias 'neovm--o1-show
    (byte-compile
     (lambda ()
       (let (fs)
         (mapbacktrace
          (lambda (_evald f args _flags)
            (unless (eq f 'mapbacktrace)
              (push (list (if (symbolp f) f (type-of f)) args) fs))))
         (setq neovm--o1-seen (nreverse fs))
         (list (backtrace-frame 0 'neovm--o1-show) (backtrace-frame 1 'neovm--o1-show))))))
  (defalias 'neovm--o1-inner
    (byte-compile (lambda (x) (if (eq x 'show) (neovm--o1-show) (+ x 1)))))
  (defalias 'neovm--o1-outer
    (byte-compile (lambda (x y) (if y (neovm--o1-inner x) 0))))
  (defalias 'neovm--o1-mid3
    (byte-compile (lambda (a b c) (list (neovm--o1-inner a) b c))))
  (defalias 'neovm--o1-top (byte-compile (lambda (x) (neovm--o1-mid3 x 1 2))))
  (dotimes (i 6000) (neovm--o1-outer i 1) (neovm--o1-top i))
  (list (neovm--o1-outer 'show 7) (seq-take neovm--o1-seen 3)
        (neovm--o1-top 'show) (seq-take neovm--o1-seen 3)))"#;
    let expect = expect_test::expect![[
        r#""OK (((t neovm--o1-show) (t neovm--o1-inner show)) ((neovm--o1-show nil) (neovm--o1-inner (show)) (neovm--o1-outer (show 7))) (((t neovm--o1-show) (t neovm--o1-inner show)) 1 2) ((neovm--o1-show nil) (neovm--o1-inner (show)) (neovm--o1-mid3 (show 1 2))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// O2: `called-interactively-p` and a `backtrace-frame` BASE lookup through
/// compiled wrappers, both of which walk frames by function symbol.
#[test]
fn oracle_jit_speculated_calls_keep_called_interactively_p_and_frame_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--o2-inner
    (byte-compile (lambda () (backtrace-frame 1 'neovm--o2-inner))))
  (defalias 'neovm--o2-cmd
    (byte-compile
     (lambda () (interactive) (list (called-interactively-p 'any) (neovm--o2-inner)))))
  (defalias 'neovm--o2-wrap (byte-compile (lambda () (neovm--o2-cmd))))
  (dotimes (_ 6000) (neovm--o2-wrap))
  (list (neovm--o2-wrap) (call-interactively 'neovm--o2-cmd)))"#;
    let expect = expect_test::expect![[r#""OK ((nil (t neovm--o2-cmd)) (t (t neovm--o2-cmd)))""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// A compiled activation whose symbol is redefined under it, followed by a
/// collection, keeps its function alive, as GNU's bytecode frame does
/// (`fp->fun`, src/bytecode.c:518): the object, held only weakly otherwise,
/// is still in the table.
#[test]
fn oracle_jit_running_function_survives_its_redefinition_and_a_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--pin-tab (make-hash-table :test 'eq :weakness 'key))
  (defvar neovm--pin-armed nil)
  (defvar neovm--pin-during nil)
  (fset 'neovm--pin-f
        (byte-compile
         (lambda (n)
           (if (= n 0)
               (if neovm--pin-armed
                   (progn
                     (fset 'neovm--pin-f #'ignore)
                     (garbage-collect)
                     (setq neovm--pin-during (hash-table-count neovm--pin-tab))
                     0)
                 0)
             (1+ (neovm--pin-f (1- n)))))))
  (fset 'neovm--pin-g (byte-compile (lambda (n) (neovm--pin-f n))))
  (dotimes (_ 6000) (neovm--pin-g 3))
  (puthash (symbol-function 'neovm--pin-f) t neovm--pin-tab)
  (list (let ((neovm--pin-armed t)) (neovm--pin-g 3)) neovm--pin-during))"#;
    let expect = expect_test::expect![[r#""OK (3 1)""#]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}
