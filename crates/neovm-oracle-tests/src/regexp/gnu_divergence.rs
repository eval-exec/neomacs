//! GNU regexp parity probes for edge cases that previously diverged.
//!
//! These tests document behavior confirmed against local GNU Emacs source and
//! oracle runs.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_regexp_gnu_mid_pattern_anchors_are_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((probe (lambda (regexp string)
                       (let ((pos (string-match regexp string)))
                         (list regexp string pos
                               (and pos (match-string 0 string)))))))
      (list
       (funcall probe "a^b" "a^b")
       (funcall probe "a^b" "ab")
       (funcall probe "a$b" "a$b")
       (funcall probe "a$b" "ab")
       (funcall probe "\\(a\\|b^c\\)" "b^c")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a^b\" \"a^b\" 0 \"a^b\") (\"a^b\" \"ab\" nil nil) (\"a$b\" \"a$b\" 0 \"a$b\") (\"a$b\" \"ab\" nil nil) (\"\\\\(a\\\\|b^c\\\\)\" \"b^c\" 0 \"b^c\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_backslash_d_is_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((probe (lambda (regexp string)
                       (let ((pos (string-match regexp string)))
                         (list regexp string pos
                               (and pos (match-string 0 string)))))))
      (list
       (funcall probe "\\d" "5")
       (funcall probe "\\d" "d")
       (funcall probe "\\D" "x")
       (funcall probe "\\D" "D")
       (funcall probe "a\\db" "adb")
       (funcall probe "a\\db" "a5b")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\d\" \"5\" nil nil) (\"\\\\d\" \"d\" 0 \"d\") (\"\\\\D\" \"x\" nil nil) (\"\\\\D\" \"D\" 0 \"D\") (\"a\\\\db\" \"adb\" 0 \"adb\") (\"a\\\\db\" \"a5b\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_escaped_control_letters_are_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((probe (lambda (regexp string)
                       (let ((pos (string-match regexp string)))
                         (list regexp string pos
                               (and pos (match-string 0 string)))))))
      (list
       (funcall probe "\\t" "t")
       (funcall probe "\\t" "\t")
       (funcall probe "\\n" "n")
       (funcall probe "\\n" "\n")
       (funcall probe "\\r" "r")
       (funcall probe "\\r" "\r")
       (funcall probe "\\f" "f")
       (funcall probe "\\f" "\f")
       (funcall probe "\\a" "a")
       (funcall probe "\\a" (string 7))
       (funcall probe "\\e" "e")
       (funcall probe "\\e" (string 27))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\t\" \"t\" 0 \"t\") (\"\\\\t\" \"\t\" nil nil) (\"\\\\n\" \"n\" 0 \"n\") (\"\\\\n\" \"\\n\" nil nil) (\"\\\\r\" \"r\" 0 \"r\") (\"\\\\r\" \"\\r\" nil nil) (\"\\\\f\" \"f\" 0 \"f\") (\"\\\\f\" \"\\f\" nil nil) (\"\\\\a\" \"a\" 0 \"a\") (\"\\\\a\" \"\u{7}\" nil nil) (\"\\\\e\" \"e\" 0 \"e\") (\"\\\\e\" \"\u{1b}\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_at_point_anchor_is_not_for_string_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((probe (lambda (regexp string &optional start)
                       (let ((pos (string-match regexp string start)))
                         (list regexp string start pos
                               (and pos (match-string 0 string)))))))
      (list
       (funcall probe "\\=" "")
       (funcall probe "\\=" "abc")
       (funcall probe "a\\=b" "ab")
       (funcall probe "\\=b" "ab" 1)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\=\" \"\" nil nil nil) (\"\\\\=\" \"abc\" nil nil nil) (\"a\\\\=b\" \"ab\" nil nil nil) (\"\\\\=b\" \"ab\" 1 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_bare_intervals_are_literals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((probe (lambda (regexp string)
                       (let ((pos (string-match regexp string)))
                         (list regexp string pos
                               (and pos (match-string 0 string)))))))
      (list
       (funcall probe "\\{1\\}" "{1}")
       (funcall probe "\\{1,2\\}" "{1,2}")
       (funcall probe "\\{,2\\}" "{,2}")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\{1\\\\}\" \"{1}\" 0 \"{1}\") (\"\\\\{1,2\\\\}\" \"{1,2}\" 0 \"{1,2}\") (\"\\\\{,2\\\\}\" \"{,2}\" 0 \"{,2}\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_malformed_symbol_boundary_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (condition-case err
          (string-match "\\_x" "_x")
        (error (list :error (car err) (cadr err))))
      (condition-case err
          (string-match "\\_" "_")
        (error (list :error (car err) (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:error invalid-regexp \"Invalid regular expression\") (:error invalid-regexp \"Premature end of regular expression\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_invalid_syntax_class_designators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((probe (lambda (regexp string)
                       (condition-case err
                           (let ((pos (string-match regexp string)))
                             (list regexp string pos
                                   (and pos (match-string 0 string))))
                         (error (list regexp string :error
                                      (car err) (cadr err)))))))
      (list
       (funcall probe "\\sz" "z")
       (funcall probe "\\sq" "q")
       (funcall probe "\\s0" "0")
       (funcall probe "\\S0" "0")
       (funcall probe "\\S0" "a")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\sz\" \"z\" nil nil) (\"\\\\sq\" \"q\" nil nil) (\"\\\\s0\" \"0\" nil nil) (\"\\\\S0\" \"0\" 0 \"0\") (\"\\\\S0\" \"a\" 0 \"a\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_unknown_group_extension_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
      (condition-case err
          (string-match "\\(?x:a\\)" "(?x:a)")
        (error (list :error (car err) (cadr err))))
      (condition-case err
          (string-match "\\(??:a\\)" "(??:a)")
        (error (list :error (car err) (cadr err))))
      (condition-case err
          (string-match "\\(?-1:a\\)" "(?-1:a)")
        (error (list :error (car err) (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:error invalid-regexp \"Invalid regular expression\") (:error invalid-regexp \"Invalid regular expression\") (:error invalid-regexp \"Invalid regular expression\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_category_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((probe (lambda (regexp string)
                       (let ((pos (string-match regexp string)))
                         (list regexp string pos
                               (and pos (match-string 0 string)))))))
      (list
       (funcall probe "\\ca" "\n")
       (funcall probe "\\ca" "\t")
       (funcall probe "\\ca" "A")
       (funcall probe "\\c|" (string #x4e2d))
       (funcall probe "\\c6" (string #x0664))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\ca\" \"\\n\" nil nil) (\"\\\\ca\" \"\t\" nil nil) (\"\\\\ca\" \"A\" 0 \"A\") (\"\\\\c|\" \"中\" 0 \"中\") (\"\\\\c6\" \"٤\" 0 \"٤\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_unicode_case_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((case-fold-search t)
      (probe (lambda (regexp string)
               (let ((pos (string-match regexp string)))
                 (list regexp string pos
                       (and pos (match-string 0 string)))))))
  (list
   (funcall probe (string #x03a9) (string #x03c9))
   (funcall probe (string #x0414) (string #x0434))
   (funcall probe (string #x00e9) (string #x00c9))
   (funcall probe "[[:upper:]]+" "abc")
   (funcall probe "[[:lower:]]+" "ABC")))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Ω\" \"ω\" 0 \"ω\") (\"Д\" \"д\" 0 \"д\") (\"é\" \"É\" 0 \"É\") (\"[[:upper:]]+\" \"abc\" 0 \"abc\") (\"[[:lower:]]+\" \"ABC\" 0 \"ABC\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_gnu_custom_case_table_folding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((tbl (make-char-table 'case-table nil)))
  (set-char-table-range tbl ?X ?q)
  (set-case-table tbl)
  (list
   (char-table-range (current-case-table) ?X)
   (char-table-p (char-table-extra-slot (current-case-table) 0))
   (char-table-range (char-table-extra-slot (current-case-table) 1) ?X)
   (char-table-range (char-table-extra-slot (current-case-table) 1) ?q)
   (char-table-range (char-table-extra-slot (current-case-table) 2) ?X)
   (char-table-range (char-table-extra-slot (current-case-table) 2) ?q)
   (let ((case-fold-search t))
     (string-match "X" "q"))
   (let ((case-fold-search t))
     (string-match-p "X" "q"))
   (let ((case-fold-search t))
     (posix-string-match "X" "q"))))"#;
    let expect = expect_test::expect![[r#""OK (113 t 113 nil nil 88 0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
