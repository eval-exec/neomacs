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
