//! UTF-8 / multibyte *char-table & category* divergence probes.
//!
//! Probes char-table range operations (`set-char-table-range` /
//! `char-table-range`) over multibyte char ranges, `aref` on the standard
//! syntax table for non-ASCII, `map-char-table`, and file append I/O with
//! multibyte content.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- char-table range get/set over multibyte --------------------------------

#[test]
fn div_utf8_char_table_range_set_get_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (hira cjk 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((ct (make-char-table 'example 0)))
  (set-char-table-range ct #x3042 'hira)
  (set-char-table-range ct '(#x4E00 . #x9FFF) 'cjk)
  (list (char-table-range ct #x3042)
        (char-table-range ct #x4E2D)
        (char-table-range ct #x100)
        (char-table-range ct ?a)))
"#,
        expect,
    );
}

#[test]
fn div_utf8_char_table_range_full_table_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (all emoji all)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((ct (make-char-table 'example :default)))
  (set-char-table-range ct t 'all)
  (set-char-table-range ct #x1f600 'emoji)
  (list (char-table-range ct ?a)
        (char-table-range ct #x1f600)
        (char-table-range ct #x3042)))
"#,
        expect,
    );
}

// --- aref on standard syntax table ------------------------------------------

#[test]
fn div_utf8_aref_standard_syntax_table_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2) (2) (2) (2))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (aref (standard-syntax-table) ?é)
      (aref (standard-syntax-table) ?\x3042)
      (aref (standard-syntax-table) ?a)
      (aref (standard-syntax-table) ?1))
"#,
        expect,
    );
}

// --- map-char-table over multibyte ranges -----------------------------------

#[test]
fn div_utf8_map_char_table_multibyte_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((12354 hira) (19968 19970 cjk))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((ct (make-char-table 'example nil))
      (acc nil))
  (set-char-table-range ct #x3042 'hira)
  (set-char-table-range ct '(#x4E00 . #x4E02) 'cjk)
  (map-char-table
   (lambda (key value)
     (push (if (consp key) (list (car key) (cdr key) value) (list key value)) acc))
   ct)
  (sort acc (lambda (a b) (< (car a) (car b)))))
"#,
        expect,
    );
}

// --- category table membership ----------------------------------------------

#[test]
fn div_utf8_category_table_membership() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-category)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((ct (category-table)))
  (list (char-category ?a)
        (char-category ?\x3042)
        (char-category ?\x4e2d)
        (char-category ?1)
        (char-category ?\s)))
"#,
        expect,
    );
}

// --- file append with multibyte content -------------------------------------

#[test]
fn div_utf8_file_append_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café世界\" 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((tmp (make-temp-file "app-")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'utf-8-unix))
          (write-region "café" nil tmp nil 'silent)
          (write-region "世界" nil tmp 'append 'silent))
        (with-temp-buffer
          (let ((coding-system-for-read 'utf-8-unix))
            (insert-file-contents tmp))
          (list (buffer-string) (length (buffer-string)))))
    (delete-file tmp)))
"#,
        expect,
    );
}
