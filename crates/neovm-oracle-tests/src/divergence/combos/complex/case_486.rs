/// Batch 486: minibuffer deep, completing-read, read-from-minibuffer, read-string.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx486_minibufferp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (minibufferp) (windowp (minibuffer-window)))
"##,
        expect,
    );
}

#[test]
fn div_cx486_window_minibuffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(window-minibuffer-p (selected-window))
"##,
        expect,
    );
}

#[test]
fn div_cx486_minibuffer_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (active-minibuffer-window) (minibuffer-window-active-p (minibuffer-window)))
"##,
        expect,
    );
}

#[test]
fn div_cx486_set_minibuffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK error""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (set-minibuffer-window (selected-window))
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx486_minibuffer_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (minibuffer-contents)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx486_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (minibuffer-prompt)
  (error (car e)))
"##,
        expect,
    );
}

#[test]
fn div_cx486_minibuffer_selected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(minibuffer-selected-window)
"##,
        expect,
    );
}

#[test]
fn div_cx486_minibuffer_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(minibuffer-depth)
"##,
        expect,
    );
}

#[test]
fn div_cx486_completing_read_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(completing-read-default "test: " '("a" "b" "c") nil nil nil nil "a")
"##,
    );
}

#[test]
fn div_cx486_completing_read_col() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (completing-read "select: " '("alpha" "beta" "gamma") nil t nil nil "alpha")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx486_read_from_minibuffer_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-from-minibuffer "enter: " nil nil nil nil nil)
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx486_read_string_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-string "enter: " "default")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx486_read_no_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-no-blanks-input "enter: " "default")
  (error (car e)))
"##,
    );
}

#[test]
fn div_cx486_read_passwd() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(fboundp 'read-passwd)
"##,
        expect,
    );
}

#[test]
fn div_cx486_read_answer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(condition-case e
    (read-answer "question (y/n): " '(("y" ?y "yes") ("n" ?n "no")))
  (error (car e)))
"##,
    );
}
