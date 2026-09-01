use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx454_split_string_omit_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\") (\"a\" \"b\" \"c\") (\"a\" \"\" \"b\" \"c\") (\"café\" \"世界\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (split-string "a|b|c" "|" t)
      (split-string "a||b|c" "|" t)
      (split-string "a||b|c" "|")
      (split-string "café|世界" "|" t))"##,
        expect,
    );
}

#[test]
fn div_cx454_mapconcat_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a, b, c\" \"1-2-3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (mapconcat #'identity '("a" "b" "c") ", ")
      (mapconcat #'number-to-string '(1 2 3) "-"))"##,
        expect,
    );
}

#[test]
fn div_cx454_assoc_string_casefold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"Foo\" . 1) (\"bar\" . 2) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((al '(("Foo" . 1) ("bar" . 2))))
  (list (assoc-string "foo" al t)
        (assoc-string "BAR" al t)
        (assoc-string "baz" al t)))"##,
        expect,
    );
}

#[test]
fn div_cx454_cl_position_find_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((lst '((:a 1) (:b 2) (:a 3))))
  (list (cl-position :a lst :key #'car)
        (cl-find :a lst :key #'car)
        (cl-count :a lst :key #'car)))"##,
        expect,
    );
}

#[test]
fn div_cx454_cl_delete_duplicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-delete-duplicates)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (cl-delete-duplicates '(1 2 1 3 2 4) :test #'=)
      (cl-delete-duplicates '("a" "b" "a" "c") :test #'equal))"##,
        expect,
    );
}

#[test]
fn div_cx454_seq_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'seq)
  (seq-group-by #'cl-evenp '(1 2 3 4 5 6)))"##,
        expect,
    );
}

#[test]
fn div_cx454_seq_min_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'seq)
  (list (seq-min '(3 1 4 1 5)) (seq-max '(3 1 4 1 5))))"##,
        expect,
    );
}

#[test]
fn div_cx454_cl_reduce_some_every() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-reduce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (cl-reduce #'+ '(1 2 3 4))
      (cl-some #'oddp '(2 4 6))
      (cl-every #'numberp '(1 2 3)))"##,
        expect,
    );
}

#[test]
fn div_cx454_bufferpos_filepos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello\nworld\n")
  (list (bufferpos-to-filepos 3)
        (filepos-to-bufferpos 3)))"##,
        expect,
    );
}

#[test]
fn div_cx454_string_to_syntax_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0) (2) (1) (4) (5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (string-to-syntax " ")
      (string-to-syntax "w")
      (string-to-syntax ".")
      (string-to-syntax "(")
      (string-to-syntax ")"))"##,
        expect,
    );
}

#[test]
fn div_cx454_seq_into() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([1 2 3] (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'seq)
  (list (seq-into '(1 2 3) 'vector)
        (seq-into [1 2 3] 'list)))"##,
        expect,
    );
}

#[test]
fn div_cx454_match_data_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 11 0 5 6 11) (0 11 0 5 6 11))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world foo")
  (string-match "\\([a-z]+\\) \\([a-z]+\\)" "hello world")
  (list (match-data) (match-data t)))"##,
        expect,
    );
}

#[test]
fn div_cx454_string_as_multibyte_unibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"cafe\" \"cafe\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "cafe"))
  (list (string-as-unibyte s)
        (string-as-multibyte (string-as-unibyte s))
        (equal s (string-as-multibyte (string-as-unibyte s)))))"##,
        expect,
    );
}

#[test]
fn div_cx454_format_time_string_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"2024-06-16 12:00:00\" \"Sunday, June 16 2024\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 0 12 16 6 2024 nil)))
  (list (format-time-string "%Y-%m-%d %H:%M:%S" t1)
        (format-time-string "%A, %B %d %Y" t1)))"##,
        expect,
    );
}

#[test]
fn div_cx454_window_config_frame() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((c (current-window-configuration)))
  (list (window-configuration-p c)
        (framep (window-configuration-frame c))))"##,
        expect,
    );
}
