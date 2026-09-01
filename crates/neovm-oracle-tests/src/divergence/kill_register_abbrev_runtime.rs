//! Kill-ring/yank, kill-append, registers, rectangle extract/insert,
//! transpose-words, and abbrev expansion/table queries parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn kill_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"one two\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((kill-ring nil) (kill-ring-yank-pointer nil))
    (kill-new "one") (kill-append " two" nil)
    (list (current-kill 0) (length kill-ring))))"##,
        expect,
    );
}

#[test]
fn kill_yank() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"defghabc\" \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdefgh") (goto-char 1)
  (kill-region 1 4)
  (goto-char (point-max)) (yank)
  (list (buffer-string) (current-kill 0)))"##,
        expect,
    );
}

#[test]
fn rectangle_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"XX12\\nYY34\\nZZ56\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "12\n34\n56\n")
  (goto-char 1)
  (insert-rectangle '("XX" "YY" "ZZ"))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn rectangle_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"ab\" \"de\" \"gh\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc\ndef\nghi\n")
  (list (extract-rectangle 1 11)))"##,
        expect,
    );
}

#[test]
fn registers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello\" 42 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((register-alist nil))
  (set-register ?a "hello")
  (set-register ?b 42)
  (list (get-register ?a) (get-register ?b) (get-register ?z)))"##,
        expect,
    );
}

#[test]
fn transpose_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"def abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abc def")
  (goto-char 4) (transpose-words 1)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn abbrev_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"the receive\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (define-abbrev-table 'neo-abbrev-table '(("teh" "the") ("recv" "receive")))
  (setq local-abbrev-table neo-abbrev-table)
  (abbrev-mode 1)
  (insert "teh") (expand-abbrev)
  (insert " recv") (expand-abbrev)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn abbrev_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"by the way\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (define-abbrev-table 'neo-at2 '(("btw" "by the way")))
  (list (abbrev-expansion "btw" neo-at2)
        (abbrev-expansion "nope" neo-at2)
        (abbrev-table-p neo-at2)))"##,
        expect,
    );
}
