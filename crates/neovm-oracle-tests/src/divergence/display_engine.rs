//! Divergence tests: display engine, glyphless chars, display tables.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_display_table_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'make-display-table)
  (fboundp 'display-table-slot)
  (fboundp 'set-display-table-slot)
  (fboundp 'standard-display-table)
  (fboundp 'buffer-display-table)
  (fboundp 'window-display-table)) "#,
        expect,
    );
}

#[test]
fn divergence_glyphless_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'glyphless-char-display)
  (boundp 'glyphless-char-display-control)
  (listp glyphless-char-display-control)
  (fboundp 'glyphless-char-p)) "#,
        expect,
    );
}

#[test]
fn divergence_redisplay_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'redisplay)
  (fboundp 'force-window-update)
  (fboundp 'window-text-height)
  (fboundp 'window-text-width)
  (boundp 'redisplay-dont-pause)
  (boundp 'redisplay-skip-initialization)) "#,
        expect,
    );
}

#[test]
fn divergence_line_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'default-line-height)
  (fboundp 'line-pixel-height)
  (fboundp 'window-line-height)
  (boundp 'line-spacing)
  (numberp line-spacing)) "#,
        expect,
    );
}

#[test]
fn divergence_invisible_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'buffer-invisibility-spec)
  (listp buffer-invisibility-spec)
  (fboundp 'add-to-invisibility-spec)
  (fboundp 'remove-from-invisibility-spec)) "#,
        expect,
    );
}

#[test]
fn divergence_selective_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'selective-display)
  (boundp 'selective-display-ellipses)
  (fboundp 'set-selective-display)) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_arrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'overlay-arrow-position)
  (boundp 'overlay-arrow-string)
  (fboundp 'set-overlay-arrow)) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_arrow_defvar_lisp_declarations_are_special() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU declares all three variables with DEFVAR_LISP in xdisp.c.  Besides
    // installing a value, that declaration makes lexical `let' bindings
    // dynamically visible to independently created functions.
    let expect = expect_test::expect![[r#""OK (t t t changed)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (special-variable-p 'overlay-arrow-position)
  (special-variable-p 'overlay-arrow-string)
  (special-variable-p 'overlay-arrow-variable-list)
  (let ((overlay-arrow-position nil))
    (funcall
     (eval
      '(lambda ()
         (setq overlay-arrow-position 'changed))
      t))
    overlay-arrow-position))"#,
        expect,
    );
}

#[test]
fn divergence_truncate_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'truncate-lines)
  (booleanp truncate-lines)
  (boundp 'truncate-partial-width-windows)
  (numberp truncate-partial-width-windows)) "#,
        expect,
    );
}

#[test]
fn divergence_word_wrap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (boundp 'word-wrap)
  (booleanp word-wrap)
  (boundp 'wrap-prefix)
  (boundp 'wrap-prefix-function)) "#,
        expect,
    );
}

#[test]
fn divergence_display_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (fboundp 'set-window-margins)
  (fboundp 'window-margins)
  (fboundp 'set-window-fringes)
  (fboundp 'window-fringes)
  (fboundp 'set-window-scroll-bars)
  (fboundp 'window-scroll-bars)) "#,
        expect,
    );
}
