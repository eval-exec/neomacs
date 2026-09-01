//! Oracle parity tests for advanced regexp patterns:
//! `string-match` with START, `re-search-forward` with COUNT,
//! `replace-regexp-in-string` with all 6 parameters,
//! Emacs-specific regex syntax (shy groups, word boundaries, etc.).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-match with START parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_string_match_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s "foo bar foo baz foo"))
                    (list (string-match "foo" s)
                          (string-match "foo" s 1)
                          (string-match "foo" s 9)
                          (string-match "foo" s 15)
                          (string-match "xyz" s 0)))"#;
    let expect = expect_test::expect![[r#""OK (0 8 16 16 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_string_match_groups_at_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Groups with START offset
    let form = r#"(let ((s "key1=val1 key2=val2 key3=val3"))
                    (let ((pos 0) (results nil))
                      (while (string-match
                              "\\([a-z]+[0-9]\\)=\\([a-z]+[0-9]\\)"
                              s pos)
                        (setq results
                              (cons (list (match-string 1 s)
                                          (match-string 2 s))
                                    results))
                        (setq pos (match-end 0)))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"key1\" \"val1\") (\"key2\" \"val2\") (\"key3\" \"val3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// re-search-forward with COUNT
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_re_search_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "aaa bbb aaa ccc aaa ddd aaa")
                    (goto-char (point-min))
                    ;; Find 3rd occurrence
                    (let ((found (re-search-forward "aaa" nil t 3)))
                      (list found (point)
                            ;; Position after 3rd "aaa"
                            (buffer-substring (- (point) 3) (point)))))"#;
    let expect = expect_test::expect![[r#""OK (20 20 \"aaa\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_re_search_count_negative() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Negative count: search backward
    let form = r#"(with-temp-buffer
                    (insert "xx yy xx zz xx")
                    (goto-char (point-max))
                    (let ((found (re-search-forward "xx" nil t -2)))
                      (list found (point))))"#;
    let expect = expect_test::expect![[r#""OK (7 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-regexp-in-string — all 6 params
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
                    (replace-regexp-in-string "foo" "bar" "foo baz foo")
                    (replace-regexp-in-string "[0-9]+" "N" "a1 b23 c456")
                    (replace-regexp-in-string "\\s-+" " " "a   b\t\tc"))"#;
    let expect = expect_test::expect![[r#""OK (\"bar baz bar\" \"aN bN cN\" \"a b c\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_replace_fixedcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FIXEDCASE parameter (4th arg)
    let form = r#"(list
                    ;; Without fixedcase: case of replacement follows match
                    (replace-regexp-in-string "hello" "world"
                                              "Hello HELLO hello" nil)
                    ;; With fixedcase=t: replacement used as-is
                    (replace-regexp-in-string "hello" "world"
                                              "Hello HELLO hello" t))"#;
    let expect = expect_test::expect![[r#""OK (\"World WORLD world\" \"world world world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_replace_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LITERAL parameter (5th arg): don't treat \ specially in REP
    let form = r#"(list
                    ;; With literal=nil: \1 is backreference
                    (replace-regexp-in-string
                     "\\([a-z]+\\)-\\([0-9]+\\)"
                     "\\2-\\1"
                     "foo-123 bar-456")
                    ;; With literal=t: replacement is literal
                    (replace-regexp-in-string
                     "[0-9]+" "\\1"
                     "a1b2c3" nil t))"#;
    let expect = expect_test::expect![[r#""OK (\"123-foo 456-bar\" \"a\\\\1b\\\\1c\\\\1\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_replace_start() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // START parameter (6th arg): begin replacing from position
    let form = r#"(list
                    (replace-regexp-in-string "x" "Y" "xAxBxCxDx" nil nil 4)
                    (replace-regexp-in-string "[aeiou]" "*"
                                              "hello world" nil nil 3))"#;
    let expect =
        expect_test::expect![[r#""ERR (error \"replace-match subexpression does not exist\" 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_replace_function_rep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // REP can be a function that receives the matched string
    let form = r#"(list
                    (replace-regexp-in-string
                     "[0-9]+"
                     (lambda (m)
                       (number-to-string (* 2 (string-to-number m))))
                     "a1 b2 c3")
                    (replace-regexp-in-string
                     "\\b[a-z]"
                     #'upcase
                     "hello world foo bar"))"#;
    let expect = expect_test::expect![[r#""OK (\"a2 b4 c6\" \"Hello World Foo Bar\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Emacs regex specifics: shy groups, word boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_shy_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // \\(?: ... \\) is a shy group — doesn't capture
    let form = r#"(progn
                    (string-match
                     "\\(?:foo\\|bar\\)-\\([0-9]+\\)"
                     "bar-42")
                    (list (match-string 0 "bar-42")
                          (match-string 1 "bar-42")))"#;
    let expect = expect_test::expect![[r#""OK (\"bar-42\" \"42\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_word_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((text "cat concatenate category scat"))
                    (let ((pos 0) (matches nil))
                      (while (string-match "\\bcat\\b" text pos)
                        (setq matches
                              (cons (match-string 0 text) matches)
                              pos (match-end 0)))
                      (list (nreverse matches)
                            (length matches))))"#;
    let expect = expect_test::expect![[r#""OK ((\"cat\") 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: regex-based parser
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_url_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse URLs using regex groups
    let form = r#"(let ((urls '("https://example.com/path?q=1"
                                "http://foo.bar:8080/api/v1"
                                "ftp://files.server.net/data")))
                    (mapcar
                     (lambda (url)
                       (when (string-match
                              "\\(https?\\|ftp\\)://\\([^/:]+\\)\\(?::\\([0-9]+\\)\\)?\\(/[^?]*\\)"
                              url)
                         (list (match-string 1 url)
                               (match-string 2 url)
                               (match-string 3 url)
                               (match-string 4 url))))
                     urls))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"https\" \"example.com\" nil \"/path\") (\"http\" \"foo.bar\" \"8080\" \"/api/v1\") (\"ftp\" \"files.server.net\" nil \"/data\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: regex-based text transformer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_text_transformer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Convert camelCase to snake_case using regex
    let form = r#"(let ((camel-to-snake
                         (lambda (s)
                           (let ((result
                                  (replace-regexp-in-string
                                   "\\([a-z]\\)\\([A-Z]\\)"
                                   "\\1_\\2" s)))
                             (downcase result)))))
                    (mapcar camel-to-snake
                            '("camelCase"
                              "getElementById"
                              "XMLHttpRequest"
                              "simpleWord"
                              "already_snake")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"c_am_el_ca_se\" \"g_et_el_em_en_tb_yi_d\" \"x_ml_ht_tp_re_qu_es_t\" \"s_im_pl_ew_or_d\" \"a_lr_ea_dy_s_na_ke\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
