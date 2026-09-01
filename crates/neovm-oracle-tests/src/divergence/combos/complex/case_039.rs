//! Complex combo batch 39 — extend word-movement vein: explicit subword-*
//! functions, transpose-words, word-search with subword, electric-indent,
//! auto-fill + subword, plus remaining minor-mode interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx39_subword_forward_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 10 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseString")
      (goto-char 1)
      (list (progn (subword-forward 1) (point))
            (progn (subword-forward 1) (point))
            (progn (subword-forward 1) (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_subword_backward_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseString")
      (goto-char 16)
      (list (progn (subword-backward 1) (point))
            (progn (subword-backward 1) (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_subword_upcase_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"CAMELCaseString\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseString")
      (goto-char 1)
      (subword-upcase 1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_subword_downcase_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"camelCaseString\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "CamelCaseString")
      (goto-char 1)
      (subword-downcase 1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_subword_capitalize_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"CamelCaseString\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseString")
      (goto-char 1)
      (subword-capitalize 1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_transpose_words_subword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Casecamel firstWord\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCase firstWord")
      (goto-char 1)
      (transpose-words 1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_word_search_forward_subword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCase String")
      (goto-char 1)
      (list (word-search-forward "camel" nil t)
            (progn (goto-char 1) (word-search-forward "camelCase" nil t))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_electric_indent_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"line one\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (electric-indent-mode 1)
      (insert "line one\n")
      (let ((last-command-event ?\n))
        (electric-indent-post-self-insert-function))
      (buffer-string))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx39_auto_fill_subword_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"camelCaseStringVariableHere that is long\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (auto-fill-mode 1)
      (let ((fill-column 15))
        (insert "camelCaseStringVariableHere that is long")
        (buffer-string)))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx39_superword_forward_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "snake_case_var rest")
      (goto-char 1)
      (list (progn (forward-word 1) (point))
            (progn (forward-word 1) (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_subword_mark_more() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "myCamelCaseVar rest")
      (goto-char 1)
      (mark-word 2)
      (list (region-beginning) (region-end)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_subword_transpose_explicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"CasecamelString\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseString")
      (goto-char 6)
      (subword-transpose 1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_forward_word_in_fundamental_vs_subword_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (13 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "camelCaseVar")
  (goto-char 1)
  (let ((no-mode (progn (forward-word 1) (point))))
    (erase-buffer)
    (insert "camelCaseVar")
    (goto-char 1)
    (subword-mode 1)
    (let ((with-subword (progn (forward-word 1) (point))))
      (list no-mode with-subword))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_display_property_text_width_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'display '(image nil))
  (current-column))
"##,
        expect,
    );
}

#[test]
fn div_cx39_process_exit_code_make_exit_3() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx39-e3" :command '("sh" "-c" "exit 3"))))
  (accept-process-output p 2)
  (process-exit-status p))
"##,
        expect,
    );
}

#[test]
fn div_cx39_encode_coding_region_utf8_world() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "世界"))
  (with-temp-buffer
    (insert s)
    (encode-coding-region (point-min) (point-max) 'utf-8)
    (length (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_set_buffer_multibyte_raw_byte_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 202 65 66 67))
  (set-buffer-multibyte t)
  (length (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx39_fill_paragraph_long_multibyte_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"café 非常に長い日本語の単語 end\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 10))
    (insert "café 非常に長い日本語の単語 end\n")
    (fill-paragraph)
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx39_abbrev_expansion_count_increment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function abbrev-expansion-count)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (make-abbrev-table)))
  (define-abbrev tbl "neoabbr" "expanded")
  (with-temp-buffer
    (set (make-local-variable 'local-abbrev-table) tbl)
    (abbrev-mode 1)
    (insert "neoabbr ")
    (expand-abbrev)
    (let ((count1 (abbrev-expansion-count (abbrev-symbol "neoabbr" tbl))))
      (insert "neoabbr ")
      (expand-abbrev)
      (list count1 (abbrev-expansion-count (abbrev-symbol "neoabbr" tbl))))))
"##,
        expect,
    );
}

#[test]
fn div_cx39_cl_loop_for_in_hashtable_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "x" 1 ht) (puthash "a" 2 ht) (puthash "m" 3 ht)
  (sort (cl-loop for k being the hash-keys of ht collect k) #'string<))
"##,
        expect,
    );
}
