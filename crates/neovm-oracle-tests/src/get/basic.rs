//! Oracle parity tests for `get`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_prop_get_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 12""#]];
    let (oracle_set, neovm_set) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((s 'oracle-prop-get)) (put s 'k 12) (get s 'k))",
        expect,
    );
    assert_ok_eq("12", &oracle_set, &neovm_set);

    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle_missing, neovm_missing) = crate::common::eval_oracle_and_neovm_expect(
        "(let ((s 'oracle-prop-get-missing)) (get s 'k))",
        expect,
    );
    assert_ok_eq("nil", &oracle_missing, &neovm_missing);
}

#[test]
fn oracle_prop_get_wrong_type_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp 1)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(get 1 'k)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_get_overriding_plist_environment_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
      (let ((s (make-symbol "oracle-prop-get-shadow")))
        (put s :k :real)
        (list
         :nil-override
         (let ((overriding-plist-environment
                (list (cons s (list :k nil :other :override)))))
           (list (get s :k)
                 (get s :other)))
         :put-through
         (let ((overriding-plist-environment
                (list (cons s (list :k :shadow)))))
           (list (get s :k)
                 (put s :k :new)
                 (get s :k)
                 (symbol-plist s)))
         :malformed-env
         (let ((overriding-plist-environment
                (list (cons s (cons :missing (cons :x :bad))))))
           (list (get s :k)
                 (condition-case e
                     (get s :absent)
                   (error (list (car e) (cdr e))))))
         :wrong-key
         (let ((overriding-plist-environment
                (list (cons (make-symbol "oracle-prop-get-shadow")
                            (list :k :copy)))))
           (get s :k))))"#;

    let expected = "(:nil-override (:real :override) :put-through (:shadow :new :shadow (:k :new)) :malformed-env (:new nil) :wrong-key :new)";
    let expect = expect_test::expect![[
        r#""OK (:nil-override (:real :override) :put-through (:shadow :new :shadow (:k :new)) :malformed-env (:new nil) :wrong-key :new)""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(expected, &oracle, &neovm);
}

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_get_latest_value(
        a in -100_000i64..100_000i64,
        b in -100_000i64..100_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((s 'oracle-prop-get-rand)) (put s 'k {}) (put s 'k {}) (get s 'k))",
            a, b
        );
        let expected = b.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
