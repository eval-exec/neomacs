//! Oracle parity for `make-closure` instances (GNU `Fmake_closure`,
//! alloc.c): a fresh closure per call whose constant vector is a fresh copy
//! of the prototype's with the first N slots replaced, every other slot
//! shared with the prototype.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_make_closure_instances_identity_slots_and_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((proto (make-byte-code 0 "\300\301\302E\207"
                              [V0 V1 neomacs--oracle-mc-tail] 3
                              "Instance doc." "p"
                              'neomacs--oracle-mc-x1 'neomacs--oracle-mc-x2))
       (c1 (make-closure proto 1 'one))
       (c2 (make-closure proto 1 'one))
       (c3 (make-closure proto 3 'three)))
  (list (funcall c1) (funcall c3) (funcall proto)
        (eq c1 c2) (eq c1 proto)
        (equal c1 c2) (equal c1 c3) (equal c1 proto)
        (aref c1 2) (aref c3 2) (aref proto 2)
        (eq (aref c1 2) (aref c2 2)) (eq (aref c1 2) (aref proto 2))
        (equal (aref c1 1) (aref proto 1)) (string-to-list (aref c1 1))
        (multibyte-string-p (aref c1 1))
        (length c1) (length proto)
        (let ((slots (append c1 nil)))
          (list (length slots) (car slots) (stringp (nth 1 slots))
                (nthcdr 2 slots)))
        (aref c1 0) (aref c1 3) (aref c1 4) (aref c1 5) (aref c1 6) (aref c1 7)
        (func-arity c1) (interactive-form c1) (documentation c1 t)
        (commandp c1) (closurep c1) (byte-code-function-p c1)
        (funcall (make-closure c3 5 'five))
        (funcall (make-closure proto 7))
        (aref (make-closure proto) 2)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((1 one neomacs--oracle-mc-tail) (3 three neomacs--oracle-mc-tail) (V0 V1 neomacs--oracle-mc-tail) nil nil t nil nil [1 one neomacs--oracle-mc-tail] [3 three neomacs--oracle-mc-tail] [V0 V1 neomacs--oracle-mc-tail] nil nil t (192 193 194 69 135) nil 8 8 (8 0 t ([1 one neomacs--oracle-mc-tail] 3 \"Instance doc.\" \"p\" neomacs--oracle-mc-x1 neomacs--oracle-mc-x2)) 0 3 \"Instance doc.\" \"p\" neomacs--oracle-mc-x1 neomacs--oracle-mc-x2 (0 . 0) (interactive \"p\") \"Instance doc.\" t t t (5 five neomacs--oracle-mc-tail) (7 V1 neomacs--oracle-mc-tail) [V0 V1 neomacs--oracle-mc-tail])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_make_closure_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((proto (make-byte-code 0 "\300\207" [V0] 1)))
  (list (condition-case err (make-closure proto 1 2) (error err))
        (condition-case err (make-closure 'not-a-prototype 1) (error err))
        (condition-case err (make-closure) (error (car err)))
        (condition-case err (apply #'make-closure proto '(1 2 3)) (error err))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((error \"Closure vars do not fit in constvec\") (wrong-type-argument byte-code-function-p not-a-prototype) wrong-number-of-arguments (error \"Closure vars do not fit in constvec\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
