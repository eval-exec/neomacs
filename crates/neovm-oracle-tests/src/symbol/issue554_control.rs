//! GNU throw-time EQ and provided-feature callback boundaries.
use crate::common::{
    assert_ok_eq, eval_oracle_and_neovm, return_if_neovm_enable_oracle_proptest_not_set,
};

#[test]
fn issue554_oracle_catch_positioned_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neo) = eval_oracle_and_neovm(
        r#"(let ((symbols-with-pos-enabled t)) (catch 'x (throw (position-symbol 'x 0) 7)))"#,
    );
    assert_ok_eq("7", &oracle, &neo);
}

#[test]
fn issue554_oracle_catch_flag_unwinds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neo) = eval_oracle_and_neovm(
        r#"(let ((symbols-with-pos-enabled nil)) (catch 'x (let ((symbols-with-pos-enabled t)) (throw (position-symbol 'x 0) 7))))"#,
    );
    assert_ok_eq("7", &oracle, &neo);
}

#[test]
fn issue554_oracle_catch_disabled_rejects_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neo) = eval_oracle_and_neovm(
        r#"(let ((symbols-with-pos-enabled nil)) (condition-case err (catch 'x (throw (position-symbol 'x 0) 7)) (error (car err))))"#,
    );
    assert_ok_eq("no-catch", &oracle, &neo);
}

#[test]
fn issue554_oracle_catch_positioned_nil_is_not_raw_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neo) = eval_oracle_and_neovm(
        r#"(let ((symbols-with-pos-enabled t)) (catch nil (throw (position-symbol nil 0) 7)))"#,
    );
    assert_ok_eq("7", &oracle, &neo);
}

#[test]
fn issue554_oracle_provide_after_load_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let (oracle, neo) = eval_oracle_and_neovm(
        r#"(let ((symbols-with-pos-enabled t) (features nil)) (setq issue554-hit nil after-load-alist (list (list 'issue554-provide (lambda () (setq issue554-hit 7))))) (provide (position-symbol 'issue554-provide 0)) issue554-hit)"#,
    );
    assert_ok_eq("7", &oracle, &neo);
}
