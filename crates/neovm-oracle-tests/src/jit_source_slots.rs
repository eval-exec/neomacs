//! Oracle parity for closure source slots (design `p2-1-feedback-reopt`
//! §4, O1/O4; `NEOVM_JIT_SPEC_SOURCES`): a byte-compiled caller whose
//! `funcall` of a closure argument the interpreter recorded as one closure
//! source compiles to a guarded call of that source's leaf. The frames the
//! call records, the depth error it meets, the errors of a wrong argument
//! count and the answers of instances with different captured values must
//! be GNU's (GNU ignores the variables). Every caller is warmed up from
//! interpreted code past the default threshold, so it compiles against the
//! feedback its interpreted calls recorded.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// Record call targets, and compile source slots from them.
const SOURCE_SLOT_ENV: &[(&str, &str)] = &[
    ("NEOVM_JIT_FEEDBACK", "use"),
    ("NEOVM_JIT_SPEC_SOURCES", "on"),
];

/// Instances of one closure source through one call site: each answers with
/// its own captured value; another source and a symbol at the same site
/// answer through the generic call; `mapbacktrace` inside the closure shows
/// the called object (GNU `Bcall` records `call_fun`, src/bytecode.c:792)
/// with the call's own arguments, then the caller's frame.
#[test]
fn oracle_jit_source_slot_calls_frames_and_instances() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defvar neovm--ss-seen nil)
  (defalias 'neovm--ss-call (byte-compile (lambda (f x) (funcall f x))))
  (defalias 'neovm--ss-make
    (byte-compile
     (lambda (k)
       (lambda (x)
         (if (eq x 'show)
             (let (fs)
               (mapbacktrace
                (lambda (_evald f args _flags)
                  (unless (eq f 'mapbacktrace)
                    (push (list (if (symbolp f) f (type-of f))
                                (mapcar (lambda (a) (if (or (symbolp a) (numberp a)) a (type-of a)))
                                        args))
                          fs))))
               (setq neovm--ss-seen (seq-take (nreverse fs) 2))
               k)
           (+ x k))))))
  (defalias 'neovm--ss-other (byte-compile (lambda (x) (* x 2))))
  (let ((f1 (neovm--ss-make 1)) (f2 (neovm--ss-make 10)) (sum 0))
    (dotimes (i 3000) (setq sum (+ sum (neovm--ss-call f1 i))))
    (list sum
          (neovm--ss-call f2 5)
          (neovm--ss-call (neovm--ss-make 100) 5)
          (neovm--ss-call (symbol-function 'neovm--ss-other) 5)
          (neovm--ss-call 'neovm--ss-other 6)
          (neovm--ss-call f2 'show)
          neovm--ss-seen
          (neovm--ss-call f1 7))))"#;
    let expect = expect_test::expect![[
        r#""OK (4501500 15 105 10 12 10 ((byte-code-function (show)) (neovm--ss-call (byte-code-function show))) 8)""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, SOURCE_SLOT_ENV, expect);
}

/// A recursion through a source slot meets `max-lisp-eval-depth` where GNU
/// does, with GNU's error, and the session runs on.
#[test]
fn oracle_jit_source_slot_depth_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--ssd-call (byte-compile (lambda (f n) (funcall f f n))))
  (defalias 'neovm--ssd-make
    (byte-compile
     (lambda (base)
       (lambda (self n) (if (= n 0) base (1+ (neovm--ssd-call self (1- n))))))))
  (let ((f (neovm--ssd-make 0)))
    (dotimes (_ 3000) (neovm--ssd-call f 3))
    (list (let ((max-lisp-eval-depth 200))
            (condition-case err (neovm--ssd-call f 100000) (error err)))
          (neovm--ssd-call f 50)
          (neovm--ssd-call (neovm--ssd-make 1000) 10))))"#;
    let expect = expect_test::expect![[
        r#""OK ((error \"Lisp nesting exceeds ‘max-lisp-eval-depth’\") 50 1010)""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, SOURCE_SLOT_ENV, expect);
}

/// A wrong argument count, a `&rest` closure and an `&optional` closure of
/// the recorded source at the slot's site: GNU's errors and bindings.
#[test]
fn oracle_jit_source_slot_argument_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(progn
  (defalias 'neovm--ssa-call1 (byte-compile (lambda (f x) (funcall f x))))
  (defalias 'neovm--ssa-call2 (byte-compile (lambda (f x y) (funcall f x y))))
  (defalias 'neovm--ssa-make (byte-compile (lambda (k) (lambda (x) (+ x k)))))
  (defalias 'neovm--ssa-make-opt (byte-compile (lambda (k) (lambda (x &optional y) (list x y k)))))
  (defalias 'neovm--ssa-make-rest (byte-compile (lambda (k) (lambda (x &rest r) (list x r k)))))
  (let ((f (neovm--ssa-make 1)) (o (neovm--ssa-make-opt 2)) (r (neovm--ssa-make-rest 3)))
    (dotimes (i 3000) (neovm--ssa-call1 f i) (neovm--ssa-call2 o i i))
    (list (condition-case err (neovm--ssa-call2 f 1 2) (error (car err)))
          (neovm--ssa-call1 o 5)
          (neovm--ssa-call2 o 5 6)
          (neovm--ssa-call1 r 7)
          (neovm--ssa-call2 r 7 8))))"#;
    let expect = expect_test::expect![[
        r#""OK (wrong-number-of-arguments (5 nil 2) (5 6 2) (7 nil 3) (7 (8) 3))""#
    ]];
    crate::common::assert_oracle_parity_with_env_expect(form, SOURCE_SLOT_ENV, expect);
}
