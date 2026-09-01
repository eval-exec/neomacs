//! Strong uncovered-features-27 oracle tests — org-protocol, org-collect, org-plot.
//!
//! Every test returns concrete structured data to surface divergences.

use crate::common::{
    assert_oracle_parity, assert_oracle_parity_with_shared_tempdir,
    return_if_neovm_enable_oracle_proptest_not_set,
};

// ═══════════════════════════════════════════════════════════════════════
// org-collect-keywords
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"TITLE\" \"Test\") (\"AUTHOR\" \"Me\") (\"DATE\" \"2026-01-15\") (\"OPTIONS\" \"toc:nil\") (\"FILETAGS\" \":t1:t2:\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: Test\n#+AUTHOR: Me\n#+DATE: 2026-01-15\n#+OPTIONS: toc:nil\n#+FILETAGS: :t1:t2:")
  (org-collect-keywords '("TITLE" "AUTHOR" "DATE" "OPTIONS" "FILETAGS")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-collect-keywords with multiple values
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_collect_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"TITLE\" \"T1\" \"T2\") (\"AUTHOR\" \"A\" \"B\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+TITLE: T1\n#+TITLE: T2\n#+AUTHOR: A\n#+AUTHOR: B")
  (org-collect-keywords '("TITLE" "AUTHOR")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-collect-keywords with categories
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_collect_cat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((\"CATEGORY\" \"default\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CATEGORY: default\n* H1\n:PROPERTIES:\n:CATEGORY: custom\n:END:")
  (org-collect-keywords '("CATEGORY")))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-plot/gnuplot
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_plot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#+PLOT: title:\\\"Test\\\" type:2d with:lines\\n| x | y |\\n|---+---|\\n| 1 | 2 |\\n| 2 | 4 |\\n| 3 | 6 |\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PLOT: title:\"Test\" type:2d with:lines\n| x | y |\n|---+---|\n| 1 | 2 |\n| 2 | 4 |\n| 3 | 6 |")
  (goto-char (point-min))
  (condition-case nil
      (org-plot/gnuplot)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-plot/gnuplot with options
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_plot_opts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK \"#+PLOT: title:\\\"Test\\\" type:3d with:lines set:\\\"xlabel 'X'\\\" set:\\\"ylabel 'Y'\\\"\\n| x | y | z |\\n|---+---+---|\\n| 1 | 2 | 3 |\\n| 4 | 5 | 6 |\"""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+PLOT: title:\"Test\" type:3d with:lines set:\"xlabel 'X'\" set:\"ylabel 'Y'\"\n| x | y | z |\n|---+---+---|\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |")
  (goto-char (point-min))
  (condition-case nil
      (org-plot/gnuplot)
    (error nil))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-protocol-protocol-handler
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_protocol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case nil
    (org-protocol-protocol-handler "org-protocol://store-link?url=http://example.com&title=Test")
  (error nil))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-protocol-parse-parameters
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_protocol_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-protocol-parse-parameters)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-protocol-parse-parameters "org-protocol://store-link?url=http://example.com&title=Test")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-protocol-sanitize-uri
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_protocol_sanitize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-protocol-sanitize-uri)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (org-protocol-sanitize-uri "http://example.com")
        (org-protocol-sanitize-uri "https://test.org/path?a=1&b=2")
        (org-protocol-sanitize-uri "file:///tmp/test.txt"))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-protocol-check-protocol-for
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_protocol_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-protocol-check-protocol-for)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(org-protocol-check-protocol-for "store-link")"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache-status
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-cache-status)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (let ((s (org-element-cache-status)))
    (list (plist-get s :size)
          (plist-get s :key))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element-cache-reset
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_cache_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-cache-status)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\nBody")
  (org-element-cache-reset)
  (let ((s (org-element-cache-status)))
    (list (plist-get s :size)
          (plist-get s :key))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-get/put-range
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 | 3 |\n| 4 | 5 | 6 |")
  (goto-char (point-min))
  (list (org-table-get "1" "2")
        (org-table-get "2" "3")
        (progn (org-table-put "1" "2" "X") (org-table-get "1" "2"))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-get-elem
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_elem() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-table-get-elem)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |")
  (goto-char (point-min))
  (list (org-table-get-elem 1 1)
        (org-table-get-elem 1 2)
        (org-table-get-elem 2 1)
        (org-table-get-elem 2 2)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-current-line/column
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |")
  (goto-char (point-min))
  (forward-line 1)
  (list (org-table-current-line)
        (org-table-current-column)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-analyze
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_analyze() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |")
  (goto-char (point-min))
  (let ((a (org-table-analyze)))
    (list (nth 0 a) (nth 1 a))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-maybe-eval-formula
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"| a | b | c |\\n| 1 | 2 |   |\\n| 3 | 4 |   |\\n#+TBLFM: $3=$1+$2\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b | c |\n| 1 | 2 |   |\n| 3 | 4 |   |\n#+TBLFM: $3=$1+$2")
  (goto-char (point-min))
  (forward-line 1)
  (org-table-maybe-eval-formula)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-iterate
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_iter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 |   |\n| 2 |   |\n#+TBLFM: $2=$1*2")
  (org-table-iterate)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-iterate-buffer-tables
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_iter_buf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | 2 a |\\n| 1 |   2 |\\n#+TBLFM: $2=$1*2\\n\\n| c | 3 c |\\n| 3 |   9 |\\n#+TBLFM: $2=$1*3\" 0 11 (face org-table) 11 12 (face org-table-row) 12 13 (face org-table) 13 14 (face org-table rear-nonsticky t display (space :relative-width 1)) 14 15 (face org-table) 15 16 (face org-table display (space :relative-width 1.001)) 16 17 (face org-table) 17 18 (face org-table rear-nonsticky t display (space :relative-width 1)) 18 20 (face org-table) 20 21 (face org-table) 21 22 (face org-table display (space :relative-width 1.001)) 22 23 (face org-table) 23 24 (face org-table-row) 42 47 (face org-table) 47 53 (face org-table) 53 54 (face org-table-row) 54 55 (face org-table) 55 56 (face org-table rear-nonsticky t display (space :relative-width 1)) 56 57 (face org-table) 57 58 (face org-table display (space :relative-width 1.001)) 58 59 (face org-table) 59 60 (face org-table rear-nonsticky t display (space :relative-width 1)) 60 62 (face org-table) 62 63 (face org-table) 63 64 (face org-table display (space :relative-width 1.001)) 64 65 (face org-table) 65 66 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n| 1 |   |\n#+TBLFM: $2=$1*2\n\n| c | d |\n| 3 |   |\n#+TBLFM: $2=$1*3")
  (org-table-iterate-buffer-tables)
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-export
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Export done.\"""#]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r##"(let ((file (expand-file-name "test.csv" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
  (with-temp-buffer
    (org-mode)
    (insert "| a | b |\n| 1 | 2 |")
    (condition-case nil
        (org-table-export file "orgtbl-to-csv")
      (error nil))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-import
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_import() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n| 1 | 2 |\\n| 3 | 4 |\\n\" 0 9 (face org-table) 9 10 (face org-table-row) 10 19 (face org-table) 19 20 (face org-table-row) 20 21 (face org-table) 21 22 (face org-table rear-nonsticky t display (space :relative-width 1)) 22 23 (face org-table) 23 24 (face org-table display (space :relative-width 1.001)) 24 25 (face org-table) 25 26 (face org-table rear-nonsticky t display (space :relative-width 1)) 26 27 (face org-table) 27 28 (face org-table display (space :relative-width 1.001)) 28 29 (face org-table) 29 30 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_with_shared_tempdir_expect(
        r##"(let ((file (expand-file-name "test.csv" (getenv "NEOVM_ORACLE_TEST_TMPDIR"))))
  (with-temp-file file
    (insert "a,b\n1,2\n3,4"))
  (with-temp-buffer
    (org-mode)
    (condition-case nil
        (org-table-import file nil)
      (error nil))
    (buffer-string)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-convert-region
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_convert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"| a | b |\\n| 1 | 2 |\\n| 3 | 4 |\\n\" 0 1 (face org-table) 1 2 (face org-table rear-nonsticky t display (space :relative-width 1)) 2 3 (face org-table) 3 4 (face org-table display (space :relative-width 1.001)) 4 5 (face org-table) 5 6 (face org-table rear-nonsticky t display (space :relative-width 1)) 6 7 (face org-table) 7 8 (face org-table display (space :relative-width 1.001)) 8 9 (face org-table) 9 10 (face org-table-row) 10 11 (face org-table) 11 12 (face org-table rear-nonsticky t display (space :relative-width 1)) 12 13 (face org-table) 13 14 (face org-table display (space :relative-width 1.001)) 14 15 (face org-table) 15 16 (face org-table rear-nonsticky t display (space :relative-width 1)) 16 17 (face org-table) 17 18 (face org-table display (space :relative-width 1.001)) 18 19 (face org-table) 19 20 (face org-table-row) 20 21 (face org-table) 21 22 (face org-table rear-nonsticky t display (space :relative-width 1)) 22 23 (face org-table) 23 24 (face org-table display (space :relative-width 1.001)) 24 25 (face org-table) 25 26 (face org-table rear-nonsticky t display (space :relative-width 1)) 26 27 (face org-table) 27 28 (face org-table display (space :relative-width 1.001)) 28 29 (face org-table) 29 30 (face org-table-row))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "a\tb\n1\t2\n3\t4")
  (goto-char (point-min))
  (org-table-convert-region (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-table-to-lisp
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn uf27_table_lisp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"a\" \"b\") hline (\"1\" \"2\") (\"3\" \"4\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "| a | b |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |")
  (org-table-to-lisp))"##,
        expect,
    );
}
