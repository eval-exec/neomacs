/// Batch 485: format-spec deep, replace-regexp-in-string edge, Unicode edge, string edge.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx485_format_spec_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format string\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'format-spec)
  (let ((spec (format-spec-make ?a "hello" ?b "world" ?n 42)))
    (list (format-spec "%a-%b" spec)
          (format-spec "%(a%|b%)" (format-spec-make ?a "x" ?b "y"))
          (format-spec "%%a" spec))))
"##,
        expect,
    );
}

#[test]
fn div_cx485_replace_regexp_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aabbcc\" \"abc\" \"\\\\?bc\" \"xabc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (replace-regexp-in-string "\\(.\\)" "\\1\\1" "abc")
      (replace-regexp-in-string "a" "\\&" "abc")
      (replace-regexp-in-string "a" "\\?" "abc")
      (replace-regexp-in-string "^" "x" "abc"))
"##,
        expect,
    );
}

#[test]
fn div_cx485_unicode_combining() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 0 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-width "a\u0301bc")
      (string-width "cafe\u0301")
      (char-width #x0301)
      (length "a\u0301bc"))
"##,
        expect,
    );
}

#[test]
fn div_cx485_unicode_smp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-width "\U0001F600")
      (char-width #x1F600)
      (length "\U0001F600")
      (string-bytes "\U0001F600"))
"##,
        expect,
    );
}

#[test]
fn div_cx485_unicode_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 2 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-width "\0\1\2\3")
      (char-width 0)
      (string-width "\t\n\r"))
"##,
        expect,
    );
}

#[test]
fn div_cx485_string_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\" \"c\" \"\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (substring "abc" 0 0)
      (substring "abc" 3)
      (substring "abc" -1)
      (concat)
      (string))
"##,
        expect,
    );
}

#[test]
fn div_cx485_truncate_string_to_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"he…\" \"hello\" \"世\" \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (truncate-string-to-width "hello" 3 nil nil ?.)
      (truncate-string-to-width "hello" 6)
      (truncate-string-to-width "世界" 2)
      (truncate-string-to-width "abc" 5 nil nil t))
"##,
        expect,
    );
}

#[test]
fn div_cx485_compare_strings_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (compare-strings "abc" 0 3 "abc" 0 3)
      (compare-strings "abc" 0 2 "abd" 0 2)
      (compare-strings "abc" nil nil "ABC" nil nil t))
"##,
        expect,
    );
}

#[test]
fn div_cx485_assoc_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((b . 2) 2 2 (a . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((al '((a . 1) (b . 2) (c . 3))))
  (list (assoc 'b al)
        (assoc-default 'b al)
        (assoc-default 'b al 'eq)
        (rassoc 1 al)))
"##,
        expect,
    );
}

#[test]
fn div_cx485_copy_sequence_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep #s(hash-table))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (copy-sequence '(1 2 3))
      (copy-sequence [1 2 3])
      (copy-sequence "abc")
      (length (copy-sequence (make-hash-table))))
"##,
        expect,
    );
}

#[test]
fn div_cx485_cl_subseq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (cl-subseq '(1 2 3 4 5) 1 3)
      (cl-subseq [1 2 3 4] 2)
      (cl-subseq "hello" 1 4))
"##,
        expect,
    );
}

#[test]
fn div_cx485_map_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function map-vector)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(map-vector (lambda (e) (* 2 e)) [1 2 3 4])
"##,
        expect,
    );
}

#[test]
fn div_cx485_map_car() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 6 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(mapcar (lambda (x) (* x 2)) '(1 2 3 4))
"##,
        expect,
    );
}

#[test]
fn div_cx485_map_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"1-2-3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(mapconcat (lambda (x) (format "%d" x)) '(1 2 3) "-")
"##,
        expect,
    );
}

#[test]
fn div_cx485_map_can() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2 2 4 3 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(mapcan (lambda (x) (list x (* x 2))) '(1 2 3))
"##,
        expect,
    );
}
