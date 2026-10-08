//! Arithmetic result limits, dynamic bindings, and bytecode against GNU.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn integer_width_result_boundaries_and_restoration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((x (expt 2 200)))
             (list
              (let ((integer-width 128)) (= (expt 2 127) (ash 1 127)))
              (condition-case e (let ((integer-width 128)) (expt 2 128)) (error e))
              (condition-case e (let ((integer-width 128)) (* x 1)) (error e))
              (let ((integer-width 128)) (eq x (* x)))
              (condition-case e
                  (let ((integer-width 128))
                    (funcall (make-byte-code 514 "\1\1_\207" [] 4) x x))
                (error e))
              integer-width
              (= (expt 2 200) x)))"#,
        expect_test::expect![[
            r#""OK (t (overflow-error) (overflow-error) t (overflow-error) 65536 t)""#
        ]],
    );
}
