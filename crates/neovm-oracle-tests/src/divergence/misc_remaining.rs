//! Divergence tests: remaining edge cases - random, counter, format edge.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buffer_modified_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (let ((tick1 (buffer-modified-tick)))
    (insert " World")
    (let ((tick2 (buffer-modified-tick)))
      (list (< tick1 tick2)
            (integerp tick1)
            (integerp tick2)))))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_chars_modified_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (let ((tick (buffer-chars-modified-tick)))
    (list (integerp tick)
          (>= tick 0))))"#,
        expect,
    );
}

#[test]
fn divergence_format_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"hello world\" \"‘foo’ and ‘bar’\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (format-message "hello %s" "world")
  (format-message "`foo' and `bar'")
  (stringp (format-message "%d" 42)))"#,
        expect,
    );
}

#[test]
fn divergence_propertize_buffer_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function buffer-substring-propertized)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'face 'bold)
  (list (buffer-substring 1 6)
        (buffer-substring-no-properties 1 6)
        (buffer-substring-propertized 1 6)))"#,
        expect,
    );
}

#[test]
fn divergence_minibuffer_prompt_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'minibuffer-prompt-properties)
  (listp minibuffer-prompt-properties)
  (plist-get minibuffer-prompt-properties 'read-only))"#,
        expect,
    );
}

#[test]
fn divergence_resize_mini_windows() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (grow-only) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'resize-mini-windows)
  (member resize-mini-windows '(nil t grow-only))
  (boundp 'max-mini-window-height))"#,
        expect,
    );
}

#[test]
fn divergence_enable_recursive_minibuffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (booleanp enable-recursive-minibuffers)
  (boundp 'minibuffer-depth-indicator-function))"#,
        expect,
    );
}

#[test]
fn divergence_visible_bell() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (booleanp visible-bell)
  (boundp 'ring-bell-function)
  (boundp 'visible-bell))"#,
        expect,
    );
}

#[test]
fn divergence_wait_delayed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'redisplay-sit-for)
  (fboundp 'sit-for)
  (fboundp 'discard-input))"#,
        expect,
    );
}

#[test]
fn divergence_track_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'track-mouse)
  (boundp 'track-mouse)
  (fboundp 'mouse-position)
  (fboundp 'mouse-set-point))"#,
        expect,
    );
}
