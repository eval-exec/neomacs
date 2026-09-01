//! Strict combo oracle probes, batch 67: hash-table-copy (deep copy
//! independence), map-char-table iteration, with-demoted-errors macro,
//! and define-inline.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_o1_hash_table_copy_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function hash-table-copy)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h (make-hash-table :test 'equal)))
  (puthash 'a 1 h)
  (puthash 'b 2 h)
  (let ((h2 (hash-table-copy h)))
    (puthash 'c 3 h2)
    (remhash 'a h2)
    (list (hash-table-count h)
          (hash-table-count h2)
          (gethash 'a h)
          (gethash 'a h2)
          (gethash 'c h))))
"##,
        expect,
    );
}

#[test]
fn div_o1_map_char_table_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (2 (((123 . 4194303) range-val) ((123 . 4194303) val)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'syntax-table nil))
      (count 0)
      (ranges nil))
  (aset ct ?a 'val)
  (aset ct ?b 'val)
  (set-char-table-range ct '(?x . ?z) 'range-val)
  (map-char-table
   (lambda (range val)
     (when val
       (setq count (1+ count))
       (setq ranges (cons (list range val) ranges))))
   ct)
  (list count (sort ranges
                    (lambda (x y)
                      (let ((rx (car x)) (ry (car y)))
                        (cond ((and (integerp rx) (integerp ry)) (< rx ry))
                              ((integerp rx) t)
                              ((integerp ry) nil)
                              (t (< (car rx) (car ry)))))))))
"##,
        expect,
    );
}

#[test]
fn div_o1_with_demoted_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 3 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-demoted-errors "Demoted: %S" (error "boom"))
      (with-demoted-errors "Demoted: %S" (+ 1 2))
      (condition-case err
          (with-demoted-errors "Demoted: %S" (signal 'arith-error '("x")))
        (error (cdr err))))
"##,
        expect,
    );
}

#[test]
fn div_o1_define_inline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (define-inline probe-inline-fn (x y)
    (+ x y))
  (list (probe-inline-fn 3 4)
        (fboundp 'probe-inline-fn)
        (functionp (symbol-function 'probe-inline-fn))))
"##,
        expect,
    );
}
