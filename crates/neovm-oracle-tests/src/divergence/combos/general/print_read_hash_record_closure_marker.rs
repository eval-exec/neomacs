//! Deep combo: print/read roundtrip × hash-table × record × closure ×
//! marker × overlay × buffer-local × symbol-plist × gensym.
//!
//! Stresses the serialization pipeline: objects that go through
//! `prin1-to-string` and `read-from-string` must roundtrip correctly.
//! Combined with mutable state (markers, overlays, buffer-local vars)
//! to expose subtle divergences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_print_read_hash_table_marker_overlay_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal))
        (buf (generate-new-buffer " combo-prht")))
    (with-current-buffer buf
      (insert "HELLO-WORLD")
      (put 'my-sym 'prop1 'val1)
      (put 'my-sym 'prop2 '(a b c))
      (let ((m (copy-marker 5 nil)))
        (puthash "key1" (list 1 2 3) ht)
        (puthash "key2" (list 'a 'b 'c) ht)
        (puthash "nested" (list (list 1 2) (list 3 4)) ht)
        (let* ((printed (prin1-to-string ht))
               (read-back (car (read-from-string printed)))
               (ht-equal (equal ht read-back))
               (sym-props (list (get 'my-sym 'prop1)
                                (get 'my-sym 'prop2)))
               (sym-printed (prin1-to-string sym-props))
               (sym-read (car (read-from-string sym-printed))))
          (kill-buffer buf)
          (list ht-equal
                (gethash "key1" read-back)
                (gethash "nested" read-back)
                (equal sym-props sym-read)))))) "#,
        expect,
    );
}

#[test]
fn combo_print_read_record_closure_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t my-tag (x y z) (1 2 3) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar combo--record-type [cl-struct-my-record tag data nested])
  (let* ((rec (record 'my-record 'my-tag '(x y z) (list 1 2 3)))
         (printed (prin1-to-string rec))
         (read-back (car (read-from-string printed)))
         (closure (let ((captured 42)
                        (lst '(a b c)))
                    (lambda (x) (list captured x lst))))
         (closure-printed (prin1-to-string closure))
         (closure-read (car (read-from-string closure-printed))))
    (list (equal rec read-back)
          (aref read-back 1)
          (aref read-back 2)
          (aref read-back 3)
          (equal (funcall closure 'test)
                 (funcall closure-read 'test))))) "#,
        expect,
    );
}

#[test]
fn combo_print_read_circular_structure_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (1 . 1) 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((lst (list 1 2 3)))
    (setcdr (cddr) lst)
    (let* ((print-circle t)
           (printed (prin1-to-string lst))
           (read-back (car (read-from-string printed))))
      (list (= (car read-back) 1)
            (= (cadr read-back) 2)
            (= (caddr read-back) 3)
            (= (car (cdddr read-back)) 1))))) "#,
        expect,
    );
}

#[test]
fn combo_print_read_hash_table_nested_with_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'eq)))
    (puthash 'alpha (list 1 (list 2 3) 'beta) ht)
    (puthash 'gamma (make-vector 3 'delta) ht)
    (puthash 'nested (list (cons 'a 'b) (list 'c 'd)) ht)
    (let* ((printed (prin1-to-string ht))
           (read-back (car (read-from-string printed))))
      (list (equal (gethash 'alpha ht) (gethash 'alpha read-back))
            (equal (gethash 'gamma ht) (gethash 'gamma read-back))
            (equal (gethash 'nested ht) (gethash 'nested read-back)))))) "#,
        expect,
    );
}

#[test]
fn combo_print_read_vector_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 1 \"hello\" sym (a b) [x y z])""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((vec (vector 1 "hello" 'sym '(a b) [x y z])))
    (let* ((printed (prin1-to-string vec))
           (read-back (car (read-from-string printed))))
      (list (equal vec read-back)
            (aref read-back 0)
            (aref read-back 1)
            (aref read-back 2)
            (aref read-back 3)
            (aref read-back 4))))) "#,
        expect,
    );
}

#[test]
fn combo_print_read_bool_vector_special_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 5 t nil t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((bv (bool-vector t nil t nil t)))
    (let* ((printed (prin1-to-string bv))
           (read-back (car (read-from-string printed))))
      (list (equal bv read-back)
            (length read-back)
            (aref read-back 0)
            (aref read-back 1)
            (aref read-back 2)
            (aref read-back 3)
            (aref read-back 4))))) "#,
        expect,
    );
}

#[test]
fn combo_print_read_char_table_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ct (make-char-table 'test-table nil)))
    (aset ct ?a 'lower)
    (aset ct ?A 'upper)
    (aset ct ?0 'digit)
    (set-char-table-parent ct nil)
    (let* ((printed (prin1-to-string ct))
           (read-back (car (read-from-string printed))))
      (list (eq (aref read-back ?a) 'lower)
            (eq (aref read-back ?A) 'upper)
            (eq (aref read-back ?0) 'digit)
            (eq (aref read-back ?z) nil))))) "#,
        expect,
    );
}

#[test]
fn combo_print_read_overlapping_hash_obarray_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((sym1 (intern "combo--print-test-sym-1"))
        (sym2 (intern "combo--print-test-sym-2")))
    (put sym1 'data (list 1 2 3))
    (put sym2 'ref sym1)
    (let* ((structure (list sym1 sym2 (get sym2 'ref)))
           (printed (prin1-to-string structure))
           (read-back (car (read-from-string printed))))
      (list (eq (car read-back) sym1)
            (eq (cadr read-back) sym2)
            (eq (caddr read-back) sym1)
            (equal (get (car read-back) 'data) '(1 2 3)))))) "#,
        expect,
    );
}
