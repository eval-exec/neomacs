//! Oracle pins for the identity of a byte-code closure's slot objects.
//!
//! GNU keeps a closure as a pseudovector (alloc.c `Fmake_byte_code`,
//! `Fmake_closure`), so `(aref f 1)` and `(aref f 2)` return the one code
//! string and the one constants vector the closure holds: `eq` to themselves
//! on every read, and `make-closure` instances share the prototype's code
//! string while each gets a fresh constants vector.
//!
//! Neomacs executes from its own representation and keeps those objects
//! beside it (P3.2 L4a), so `aref` answers as GNU does, but its constants
//! pool is not the Lisp vector `aref` returns: a Lisp `aset` into that vector
//! is not seen by the function (P3.2 L4b), which stays an expected failure.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// `aref` on slots 1 and 2 returns the same objects on every read, through
/// `append` and `vconcat` too, across a collection, and across `make-closure`
/// instances exactly as GNU shares them.
#[test]
fn oracle_closure_code_and_constants_slots_are_eq_stable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((proto (make-byte-code 0 "\300\301\302E\207"
                              [V0 V1 neomacs--oracle-ci-tail] 3 "Doc."))
       (early (make-closure proto 1 'one))
       (plain (make-byte-code 257 "\211\300\\\207" [3] 3))
       (late (make-closure proto 2 'two)))
  (list (eq (aref plain 2) (aref plain 2))
        (eq (aref plain 1) (aref plain 1))
        (eq (aref early 2) (aref early 2))
        (eq (aref early 1) (aref early 1))
        (eq (aref early 1) (aref proto 1))
        (eq (aref late 1) (aref proto 1))
        (eq (aref early 1) (aref late 1))
        (eq (aref early 2) (aref late 2))
        (eq (aref early 2) (aref proto 2))
        (eq (nth 1 (append early nil)) (aref early 1))
        (eq (nth 2 (append early nil)) (aref early 2))
        (eq (aref (vconcat early) 2) (aref early 2))
        (eq (aref (make-closure early 3) 1) (aref early 1))
        (let ((v (aref plain 2)) (s (aref plain 1)))
          (garbage-collect)
          (list (eq v (aref plain 2)) (eq s (aref plain 1)) v))
        (aref early 2) (aref late 2) (aref proto 2)))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t t t t t t nil nil t t t t (t t [3]) [1 one neomacs--oracle-ci-tail] [2 two neomacs--oracle-ci-tail] [V0 V1 neomacs--oracle-ci-tail])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// `print-circle` sees the closure's slot objects as the ones Lisp holds.
#[test]
fn oracle_closure_slot_objects_print_circle_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((plain (make-byte-code 257 "\211\300\\\207" [3] 3))
       (v (aref plain 2))
       (print-circle t))
  (prin1-to-string (list v plain (aref plain 1))))
"#;

    let expect = expect_test::expect![[
        r#""OK \"(#1=[3] #[257 #2=\\\"\\\\211\\\\300\\\\\\\\\\\\207\\\" #1# 3] #2#)\"""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// Expected failure until P3.2 L4b: in GNU the function runs from the
/// constants vector `aref` returns, so a Lisp `aset` into it changes what the
/// function computes (the last element, 100). Neomacs's JIT bakes constants
/// and its interpreter reads its own pool, so the function still sees 3.
#[test]
fn oracle_closure_constants_aset_is_seen_by_the_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((proto (make-byte-code 257 "\211\300\\\207" [V0] 3))
       (f (make-closure proto 3))
       (before (funcall f 1))
       (c (aref f 2)))
  (aset c 0 99)
  (list before (aref (aref f 2) 0) (eq c (aref f 2)) (funcall f 1)))
"#;

    let expect = expect_test::expect![[r#""OK (4 99 t 100)""#]];
    crate::common::assert_oracle_divergence_expect(form, expect);
}

/// Expected failure: slot 4, a docstring, is one string object in GNU, shared
/// by `make-closure` instances. Neomacs builds a fresh string per read.
#[test]
fn oracle_closure_docstring_slot_is_eq_stable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((proto (make-byte-code 0 "\300\207" [V0] 1 "Doc."))
       (c (make-closure proto 1)))
  (list (eq (aref proto 4) (aref proto 4))
        (eq (aref c 4) (aref proto 4))
        (aref c 4)))
"#;

    let expect = expect_test::expect![[r#""OK (t t \"Doc.\")""#]];
    crate::common::assert_oracle_divergence_expect(form, expect);
}

/// `make-byte-code` returns a closure whose slots 1 and 2 ARE the string and
/// vector it was given.
#[test]
fn oracle_make_byte_code_slots_are_its_arguments() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (unibyte-string 192 135)) (v (vector 42))
       (f (make-byte-code 0 s v 1)))
  (list (eq (aref f 1) s) (eq (aref f 2) v) (funcall f)))
"#;

    let expect = expect_test::expect![[r#""OK (t t 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// Expected failure until P3.2 L4b: an `aset` into the vector given to
/// `make-byte-code` changes what the function returns in GNU.
#[test]
fn oracle_make_byte_code_constants_aset_is_seen_by_the_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((v (vector 42)) (f (make-byte-code 0 (unibyte-string 192 135) v 1)))
  (list (funcall f) (progn (aset v 0 43) (funcall f))))
"#;

    let expect = expect_test::expect![[r#""OK (42 43)""#]];
    crate::common::assert_oracle_divergence_expect(form, expect);
}
