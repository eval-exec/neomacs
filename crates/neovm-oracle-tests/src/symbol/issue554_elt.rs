//! GNU Felt validates SEQUENCE then delegates array access to Faref.
use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

fn policy_form(compiled: bool) -> String {
    let function = if compiled {
        "(progn (require 'bytecomp) (byte-compile '(lambda (table) (elt table 256))))"
    } else {
        "(lambda (table) (elt table 256))"
    };
    // GNU lread.c suppresses positioned symbols inside char-table reader syntax.
    // Public construction retains the positioned purpose. Count131 must use
    // Unicode U+0083 (bytes C2 83), not raw-byte character bytes C0 83.
    format!(
        r#"(let ((function {function}))
        (mapcar
          (lambda (enabled)
            (let* ((symbols-with-pos-enabled t)
                   (table (make-char-table
                           (position-symbol 'char-code-property-table 0) nil)))
              (set-char-table-range table '(256 . 383) "\002\002\u0083\001")
              (let* ((symbols-with-pos-enabled enabled)
                     (value (funcall function table)))
                (if (stringp value) 'compressed value))))
          '(nil t)))"#
    )
}

#[test]
fn issue554_oracle_elt_positioned_compressed_purpose_follows_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neomacs) = eval_oracle_and_neovm(&policy_form(false));
    assert_ok_eq("(compressed 2)", &oracle, &neomacs);
}

#[test]
fn issue554_oracle_compiled_elt_positioned_compressed_purpose_follows_policy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neomacs) = eval_oracle_and_neovm(&policy_form(true));
    assert_ok_eq("(compressed 2)", &oracle, &neomacs);
}

#[test]
fn issue554_oracle_elt_sequence_check_precedes_index_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neomacs) = eval_oracle_and_neovm(
        r#"
      (list (condition-case err (elt 42 'bad) (error err))
            (condition-case err (elt [] 'bad) (error err))
            (condition-case err (elt (make-char-table 'test) -1) (error err)))
    "#,
    );
    assert_ok_eq(
        "((wrong-type-argument sequencep 42) (wrong-type-argument fixnump bad) (wrong-type-argument characterp -1))",
        &oracle,
        &neomacs,
    );
}
