//! Divergence tests: print/read + serialization stress combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_nested_structure_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t (:name \"test\" :values (1 2 3) :nested (:a t :b nil) :extra \"special \\\"chars\\\"\") \"test\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((data '(:config (:name \"test\"
                                 :values (1 2 3)
                                 :nested (:a t :b nil)
                                 :extra \"special \\\"chars\\\"\")))
        (printed (prin1-to-string data))
        (read-back (car (read-from-string printed))))
  (list (equal data read-back)
        (plist-get read-back :config)
        (plist-get (plist-get read-back :config) :name)
        (= (length (plist-get (plist-get read-back :config) :values)) 3))) ",
        expect,
    );
}

#[test]
fn divergence_vector_with_mixed_types_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function equalp)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((v [1 \"two\" three (4 5) [6 7] (:key . val)])
        (printed (prin1-to-string v))
        (read-back (car (read-from-string printed))))
  (list (equalp v read-back)
        (aref read-back 0)
        (aref read-back 1)
        (aref read-back 3)
        (aref read-back 4)
        (= (length read-back) 6))) ",
        expect,
    );
}

#[test]
fn divergence_circular_hash_table_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((ht (make-hash-table :test 'equal))
        (print-circle t)
        (print-gensym t))
  (puthash \"self\" ht ht)
  (let ((printed (prin1-to-string ht)))
    (list (stringp printed)
          (> (length printed) 10)
          (string-match \"#\" printed)))) ",
        expect,
    );
}

#[test]
fn divergence_pp_formatted_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((data '((name . \"Alice\") (scores . (95 87 92)) (active . t)))
        (pp-output (with-output-to-string (pp data)))
        (single-line (prin1-to-string data)))
  (list (>= (length pp-output) (length single-line))
        (equal data (car (read-from-string pp-output)))
        (equal data (car (read-from-string single-line)))
        (> (length (split-string pp-output \"\\n\")) 1))) ",
        expect,
    );
}

#[test]
fn divergence_char_table_serialization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((ct (make-char-table 'syntax-table nil))
        (printed (prin1-to-string ct))
        (read-back (car (read-from-string printed))))
  (list (char-table-p read-back)
        (char-table-p ct)
        (equal (char-table-subtype ct) (char-table-subtype read-back)))) ",
        expect,
    );
}

#[test]
fn divergence_bool_vector_serialization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bool-vector-count-matches)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((bv (make-bool-vector 16 nil))
        (printed (prin1-to-string bv))
        (read-back (car (read-from-string printed))))
  (aset bv 0 t) (aset bv 5 t) (aset bv 15 t)
  (list (bool-vector-p bv)
        (= (bool-vector-count-matches bv t) 3)
        (= (bool-vector-count-matches bv nil) 13))) ",
        expect,
    );
}

#[test]
fn divergence_string_escape_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((strings (list \"hello\\nworld\"
                             \"tab\\there\"
                             \"\\\\backslash\"
                             \"\\\"quoted\\\"\"
                             \"bell\\007ring\"
                             \"\"))
        (roundtrip (lambda (s)
                     (let* ((p (prin1-to-string s))
                            (r (car (read-from-string p))))
                       (string= s r)))))
  (list (length strings)
        (cl-every roundtrip strings)
        (funcall roundtrip \"hello\\nworld\"))) ",
        expect,
    );
}

#[test]
fn divergence_record_serialization() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((r (record 'cl-tag 42 \"hello\" [1 2 3] '(a b)))
        (printed (prin1-to-string r))
        (read-back (car (read-from-string printed))))
  (list (recordp r)
        (recordp read-back)
        (equal r read-back)
        (= (aref read-back 1) 42)
        (string= (aref read-back 2) \"hello\"))) ",
        expect,
    );
}

#[test]
fn divergence_print_read_with_print_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 11 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((long-list (number-sequence 1 100))
        (print-length 5))
  (let* ((printed (prin1-to-string long-list))
         (read-back (car (read-from-string printed))))
    (list (<= (length read-back) 6)
          (string-match \"\\\\.\\\\.\\\\.\" printed)
          (= (nth 0 read-back) 1)
          (<= (length read-back) 6)))) ",
        expect,
    );
}

#[test]
fn divergence_nested_print_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 7 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((deep '((1 (2 (3 (4 (5 (6))))))))
        (print-level 3))
  (let* ((printed (prin1-to-string deep))
         (read-back (car (read-from-string printed))))
    (list (stringp printed)
          (string-match \"\\\\.\\\\.\\\\.\" printed)
          (listp read-back)
          (= (caar read-back) 1)))) ",
        expect,
    );
}
