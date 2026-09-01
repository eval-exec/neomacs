//! Oracle parity tests for bool-vector edge semantics.
//!
//! GNU implements the constructors in `src/alloc.c` and the bool-vector
//! primitive operations in `src/data.c`.  In particular, bool-vector binary
//! operations signal `wrong-length-argument` for length mismatches, and their
//! explicit destination argument returns the destination object only when the
//! destination changed.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_bool_vector_constructor_truthiness_and_negative_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((bv (bool-vector nil 0 "" 'sym (list 1))))
   (list (aref bv 0)
         (aref bv 1)
         (aref bv 2)
         (aref bv 3)
         (aref bv 4)))
 (let ((bv (make-bool-vector 4 0)))
   (list (aref bv 0)
         (aref bv 1)
         (aref bv 2)
         (aref bv 3)))
 (let ((bv (make-bool-vector 3 nil)))
   (aset bv 0 0)
   (aset bv 1 "")
   (aset bv 2 nil)
   (list (aref bv 0)
         (aref bv 1)
         (aref bv 2)))
 (condition-case err
     (make-bool-vector -1 nil)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![
        r#""OK ((nil t t t t) (t t t t) (t t nil) (wrong-type-argument (wholenump -1)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_bool_vector_binary_ops_signal_wrong_length_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((a (bool-vector t nil t))
      (b (bool-vector nil t))
      (dest-short (make-bool-vector 2 nil))
      (dest-long (make-bool-vector 4 nil)))
  (list
   (condition-case err
       (bool-vector-union a b)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-intersection a b)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-set-difference a b)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-exclusive-or a b)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-subsetp a b)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-union a a dest-short)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-not a dest-long)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![
        r#""OK ((wrong-length-argument (3 2)) (wrong-length-argument (3 2)) (wrong-length-argument (3 2)) (wrong-length-argument (3 2)) (wrong-length-argument (3 2 2)) (wrong-length-argument (3 3 2)) (wrong-length-argument (3 4)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_bool_vector_destination_return_value_tracks_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((a (bool-vector t nil t nil))
       (b (bool-vector nil t t nil))
       (union-dest (bool-vector t t t nil))
       (xor-dest (bool-vector t t nil nil))
       (inter-dest (bool-vector nil nil t nil))
       (diff-dest (bool-vector t nil nil nil)))
  (list
   ;; GNU returns nil when explicit dest already contains the requested result.
   (bool-vector-union a b union-dest)
   (bool-vector-exclusive-or a b xor-dest)
   (bool-vector-intersection a b inter-dest)
   (bool-vector-set-difference a b diff-dest)
   ;; A changed explicit dest is returned as the exact same object.
   (let ((dest (make-bool-vector 4 nil)))
     (list (eq (bool-vector-union a b dest) dest)
           (append dest nil)))
   (let ((dest (make-bool-vector 4 nil)))
     (list (eq (bool-vector-not a dest) dest)
           (append dest nil)))
   ;; `bool-vector-not` always returns the destination, even if unchanged.
   (let ((dest (bool-vector nil t nil t)))
     (list (eq (bool-vector-not a dest) dest)
           (append dest nil)))))
"#;

    let expect = expect_test::expect![
        r#""OK (nil nil nil nil (t (t t t nil)) (t (nil t nil t)) (t (nil t nil t)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_bool_vector_destination_can_alias_inputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((a (bool-vector t nil t nil))
      (b (bool-vector nil t t nil))
      (c (bool-vector t nil t nil))
      (d (bool-vector nil t t nil))
      (e (bool-vector t nil t nil)))
  (list
   (list (eq (bool-vector-union a b a) a)
         (append a nil))
   (list (eq (bool-vector-intersection c d d) d)
         (append d nil))
   (list (eq (bool-vector-not e e) e)
         (append e nil))))
"#;

    let expect =
        expect_test::expect![r#""OK ((t (t t t nil)) (t (nil nil t nil)) (t (nil t nil t)))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_bool_vector_count_consecutive_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((a (bool-vector t t nil nil t)))
  (list
   (bool-vector-count-consecutive a t 5)
   (bool-vector-count-consecutive a nil 5)
   (condition-case err
       (bool-vector-count-consecutive a t 6)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-count-consecutive a t -1)
     (error (list (car err) (cdr err))))
   (condition-case err
       (bool-vector-count-consecutive a t 1.5)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (0 0 (args-out-of-range (#&5\"\u{13}\" 6)) (wrong-type-argument (wholenump -1)) (wrong-type-argument (wholenump 1.5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
