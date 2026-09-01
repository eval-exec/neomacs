/// Batch 498: easy-mmode-define-minor-mode, easy-mmode-define-navigation.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx498_easy_mmode_define() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'easy-mmode)
  (easy-mmode-define-minor-mode neo-cx498-easy-mode "easy" nil nil nil)
  (fboundp 'neo-cx498-easy-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx498_easy_mmode_define_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'easy-mmode)
  (easy-mmode-define-minor-mode neo-cx498-global-easy-sub "sub" nil nil nil)
  (defun neo-cx498-easy-mode-on () (neo-cx498-global-easy-sub 1))
  (easy-mmode-define-global-mode neo-cx498-global-easy-mode
    neo-cx498-global-easy-sub neo-cx498-easy-mode-on)
  (fboundp 'neo-cx498-global-easy-mode))
"##,
        expect,
    );
}

#[test]
fn div_cx498_easy_mmode_nav() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'easy-mmode)
  (easy-mmode-define-navigation neo-cx498-page-nav "\f" "page")
  (fboundp 'neo-cx498-page-nav-forward-page))
"##,
        expect,
    );
}

#[test]
fn div_cx498_pp_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(a b c d)\\n\" \"(lambda (x) (* x 2))\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (pp-to-string '(a b c d))
      (pp-to-string '(lambda (x) (* x 2))))
"##,
        expect,
    );
}

#[test]
fn div_cx498_pp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"(defun a (x) x)\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(defun a (x) x)")
  (pp-buffer)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_indent_rigidly() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"    line1\\n    line2\\n    line3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "line1\nline2\nline3")
  (indent-rigidly (point-min) (point-max) 4)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_untabify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"                text\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "\t\ttext")
  (untabify (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_tabify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\ttext\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "        text")
  (tabify (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_upcase_downcase_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"HELLO world foo\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world foo")
  (goto-char 1)
  (upcase-word 1)
  (forward-word 1)
  (downcase-word 1)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_capitalize_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Hello World\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (goto-char 1)
  (capitalize-word 2)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_negative_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-number-of-arguments t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (condition-case e (negative-argument) (error (car e)))
      (fboundp 'digit-argument))
"##,
        expect,
    );
}

#[test]
fn div_cx498_kill_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"line2\\nline3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "line1\nline2\nline3")
  (goto-char 1)
  (kill-line 1)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_open_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"before\\n\\nafter\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "before\nafter")
  (goto-char 8)
  (open-line 1)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_split_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"split \\n      line\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "split line")
  (goto-char 7)
  (split-line)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx498_delete_indentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello\n  world")
  (goto-char 7)
  (delete-indentation)
  (buffer-string))
"##,
        expect,
    );
}
