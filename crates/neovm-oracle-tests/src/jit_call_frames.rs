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

/// O7: speculated calls of `&rest` callees -- short of the optionals, an
/// empty tail, one and three tail elements, a callee with only `&rest` --
/// bind what the interpreter binds, and the callee's frame records the
/// called symbol with the call's own arguments (GNU `Bcall` records the
/// call before `setup_frame` conses the tail, src/bytecode.c:795, :546).
#[test]
fn oracle_jit_speculated_rest_calls_bind_and_frame_like_the_interpreter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--o7-seen nil)
  (defalias 'neovm--o7-f
    (byte-compile
     (lambda (a &optional b &rest r)
       (when (eq a 'show)
         (mapbacktrace
          (lambda (_evald f args _flags)
            (when (and (symbolp f) (null neovm--o7-seen)
                       (string-prefix-p "neovm--o7-" (symbol-name f)))
              (setq neovm--o7-seen (list f args))))))
       (list a b r))))
  (defalias 'neovm--o7-only (byte-compile (lambda (&rest r) (if r (length r) r))))
  (defalias 'neovm--o7-call
    (byte-compile
     (lambda (x)
       (list (neovm--o7-f x) (neovm--o7-f x 2) (neovm--o7-f x 2 3)
             (neovm--o7-f x 2 3 4 5) (neovm--o7-only) (neovm--o7-only x x x x)))))
  (defalias 'neovm--o7-five (byte-compile (lambda (x) (neovm--o7-f x 2 3 4 5))))
  (dotimes (i 6000) (neovm--o7-call i) (neovm--o7-five i))
  (list (neovm--o7-call 1)
        (progn (setq neovm--o7-seen nil) (neovm--o7-five 'show) neovm--o7-seen)))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 nil nil) (1 2 nil) (1 2 (3)) (1 2 (3 4 5)) nil 4) (neovm--o7-f (show 2 3 4 5)))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}

/// O8: a speculated call its callee's arity rejects signals GNU's
/// `wrong-number-of-arguments` with the callee's frame already recorded --
/// the called symbol and the call's arguments -- as `Bcall` records it
/// before `setup_frame` checks the arity (src/bytecode.c:795, :539).
#[test]
fn oracle_jit_speculated_call_with_a_rejected_arity_frames_the_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--o8-seen nil)
  (defalias 'neovm--o8-one (byte-compile (lambda (x) (if x x 0))))
  (defalias 'neovm--o8-caller
    (byte-compile
     (lambda (x y) (if y (with-no-warnings (neovm--o8-one x y)) (neovm--o8-one x)))))
  (dotimes (i 6000) (neovm--o8-caller i nil))
  (list (condition-case err
            (handler-bind
                ((wrong-number-of-arguments
                  (lambda (_err)
                    (let (fs)
                      (mapbacktrace
                       (lambda (_evald f args _flags)
                         (when (and (symbolp f)
                                    (string-prefix-p "neovm--o8-" (symbol-name f)))
                           (push (list f args) fs))))
                      (setq neovm--o8-seen (nreverse fs))))))
              (neovm--o8-caller 1 2))
          (error err))
        neovm--o8-seen))"#;
    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments (1 . 1) 2) ((neovm--o8-one (1 2)) (neovm--o8-caller (1 2))))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, JIT_ENV, expect);
}
