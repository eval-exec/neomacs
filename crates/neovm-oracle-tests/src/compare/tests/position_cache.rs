//! GNU-generated parity coverage for cached compare-strings positions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_compare_position_cache_far_start_mixed_width_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((s (concat (make-string 64 ?ж) "aあ😀xy" (make-string 64 ?あ))) (x "xy"))
  (list (compare-strings x 0 2 s 67 69)
        (compare-strings "aあ😀" 0 3 s 64 67)
        (compare-strings "xz" 0 2 s 67 69)
        (compare-strings s 67 69 "xz" 0 2)
        (compare-strings "x" 0 1 s 67 69)
        (compare-strings "xyz" 0 3 s 67 69)))"#;
    let expect = expect_test::expect![[r#""OK (t t 2 -2 -2 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_negative_clamped_and_empty_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s "aжあ😀xyz"))
  (list (compare-strings "xyz" nil nil s -3 nil)
        (compare-strings "xyz" 0 999 s 4 999)
        (compare-strings "あ😀" nil nil s -5 -3)
        (compare-strings s 4 4 "" nil nil)
        (compare-strings s (length s) nil "" 0 0)
        (compare-strings "" nil nil s 1 2)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_unibyte_raw_and_unicode_codes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((u (unibyte-string #x80 #xe9 #xff))
       (r (string-make-multibyte u))
       (v (string #x80 #xe9 #xff)))
  (list (multibyte-string-p u) (multibyte-string-p r)
        (compare-strings u nil nil r nil nil)
        (compare-strings r nil nil u nil nil)
        (compare-strings u 1 2 r 1 2)
        (compare-strings u nil nil v nil nil)
        (compare-strings v nil nil r nil nil)
        (mapcar (lambda (i) (compare-strings u i (1+ i) r i (1+ i))) '(0 1 2))))"#;
    let expect = expect_test::expect![[r#""OK (nil t t t t 1 -1 (t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_repeated_same_and_alternating_operands() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((s (concat (make-string 32 ?ж) "xy" (make-string 32 ?あ)))
       (a (copy-sequence s)) (b (copy-sequence s)) (out nil))
  (dotimes (_ 4)
    (push (list (compare-strings "xy" 0 2 s 32 34)
                (compare-strings s 32 34 s 32 34)
                (compare-strings a 32 34 b 32 34)
                (compare-strings "xy" 0 2 a 32 34)
                (compare-strings "xy" 0 2 b 32 34)) out))
  (nreverse out))"#;
    let expect =
        expect_test::expect![[r#""OK ((t t t t t) (t t t t t) (t t t t t) (t t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_ascii_multibyte_and_short_multibyte_needle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((s (concat (make-string 32 ?ж) "Жq" (make-string 32 ?ж)))
       (ascii (string-make-multibyte (unibyte-string ?x ?y))))
  (list (compare-strings "Жq" 0 2 s 32 34)
        (compare-strings s 32 34 "Жq" 0 2)
        (compare-strings ascii nil nil "xy" nil nil)
        (compare-strings (unibyte-string ?x ?y) nil nil ascii nil nil)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_valid_ascii_mutation_after_warm_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((s (copy-sequence "жaあb😀cxy"))
       (before (compare-strings "xy" 0 2 s 6 8)))
  (aset s 6 ?z)
  (list before (compare-strings "zy" 0 2 s 6 8)
        (compare-strings "xy" 0 2 s 6 8)
        (compare-strings s 6 8 "xy" 0 2)))"#;
    let expect = expect_test::expect![[r#""OK (t t -1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_same_width_fillarray_and_clear_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((s (copy-sequence "жжж"))
       (before (compare-strings "ж" 0 1 s 1 2)))
  (fillarray s ?я)
  (let ((after (compare-strings "я" 0 1 s 1 2)))
    (clear-string s)
    (list before after (length s) (multibyte-string-p s)
          (compare-strings (make-string 6 0) nil nil s nil nil)
          (compare-strings (make-string 2 0) nil nil s 2 4))))"#;
    let expect = expect_test::expect![[r#""OK (t t 6 nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_standard_folded_multibyte_and_raw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((u (unibyte-string #xe9)) (r (string-make-multibyte u)))
  (list (compare-strings "ЖQ" nil nil "жq" nil nil t)
        (compare-strings "Σ" nil nil "ς" nil nil t)
        (compare-strings "µ" nil nil "μ" nil nil t)
        (compare-strings "İ" nil nil "i" nil nil t)
        (compare-strings "ı" nil nil "I" nil nil t)
        (compare-strings u nil nil r nil nil t)
        (compare-strings "é" nil nil u nil nil t)))"#;
    let expect = expect_test::expect![[r#""OK (t t t 1 1 t -1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_compare_position_cache_errors_validate_before_offset_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((s "aжあ😀"))
  (mapcar (lambda (args)
            (condition-case err (apply #'compare-strings args)
              (error (list (car err) (cdr err)))))
          (list (list s -5 nil "a" nil nil)
                (list s 3 2 "a" nil nil)
                (list s 5 nil "a" nil nil)
                (list s 0 -5 "a" nil nil)
                (list s 0 nil "a" 0 2)
                (list s 0.0 nil "a" nil nil)
                (list s 0 nil 1 nil nil))))"#;
    let expect = expect_test::expect![[
        r#""OK ((args-out-of-range (\"aжあ😀\" -5 nil)) (args-out-of-range (\"aжあ😀\" 3 2)) (args-out-of-range (\"aжあ😀\" 5 nil)) (args-out-of-range (\"aжあ😀\" 0 -5)) 2 (wrong-type-argument (integerp 0.0)) (wrong-type-argument (stringp 1)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
