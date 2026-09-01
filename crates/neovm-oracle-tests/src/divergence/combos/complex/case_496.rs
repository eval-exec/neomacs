/// Batch 496: completing-read-multiple, read-regexp, read-char-by-name deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx496_completing_read_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (completing-read-multiple "colors: " '("red" "green" "blue"))
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_regexp_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-regexp "regexp: " "default-regexp")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_shell_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-shell-command "command: ")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read--expression "expr: ")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(fboundp 'read-buffer-to-switch)
"##,
        expect,
    );
}

#[test]
fn div_cx496_read_number_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-number "number: " 42)
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (read-float "float: " 3.14)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx496_read_color() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK wrong-number-of-arguments""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (read-color "color: " "red")
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx496_read_file_name_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-file-name "file: " "/tmp" nil t "default")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_directory_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-directory-name "dir: " "/tmp" nil t)
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_yes_no() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (y-or-n-p "test? ")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_char_choice() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-char-choice "char: " '(?a ?b ?c))
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_char_by_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-char-by-name "char name: ")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_minibuffer_internal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-from-minibuffer "enter: ")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx496_read_expression_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-from-minibuffer "minibuf: " "default" nil nil nil nil)
  (error (car e)))
"##,
    );
}
