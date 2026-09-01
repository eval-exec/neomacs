//! Per-face *face-all-attributes* matrix (all GNU faces).
//!
//! One focused #[test] per face in `(face-list)`: query face-all-attributes
//! against the selected frame.
//!
//! These are parity GOLDENS, not known divergences. The face-attribute cluster
//! they were written for was fixed on 2026-07-13 by `a604c3a19`
//! (internal-get-lisp-face-attribute returned the realized face, whose colors
//! were realized against a color-capable display during the bootstrap image
//! build, instead of the lisp face GNU returns). The snapshots below carry the
//! GNU-correct `(:inherit . unspecified)` form.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_attr_abbrev_table_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-function-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'abbrev-table-name (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_blink_matching_paren_offscreen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . show-paren-match))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'blink-matching-paren-offscreen (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'bold (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_bold_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'bold-italic (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'border (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_buffer_menu_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'buffer-menu-buffer (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . link))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'button (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_child_frame_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'child-frame-border (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_completions_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit italic shadow))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'completions-annotations (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_completions_common_part() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'completions-common-part (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_completions_first_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'completions-first-difference (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_completions_group_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . t) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . shadow))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'completions-group-separator (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_completions_group_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . shadow))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'completions-group-title (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_completions_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'completions-highlight (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_confusingly_reordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . warning))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'confusingly-reordered (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_cursor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . \"white\") (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'cursor (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . \"default\") (:foundry . \"default\") (:width . normal) (:height . 1) (:weight . normal) (:slant . normal) (:underline) (:overline) (:extend) (:strike-through) (:box) (:inverse-video) (:foreground . \"unspecified-fg\") (:background . \"unspecified-bg\") (:stipple) (:inherit))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'default (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_eldoc_highlight_function_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'eldoc-highlight-function-argument (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_ampersand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-ampersand (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_binding_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-binding-variable (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_bound_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . reset) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-use-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-bound-variable (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-charset (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-coding (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_completion_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-completion-category (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_completion_category_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-completion-category))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-completion-category-definition (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"red\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-condition (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-builtin-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-constant (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_defcharset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-charset))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-defcharset (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_defcoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-coding))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-defcoding (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_defface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-defface (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_deficon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-icon))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-deficon (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_defmacro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-defun))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-defmacro (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_defoclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-oclosure))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-defoclosure (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-function-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-defun (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-defvar (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-constant-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-feature (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_free_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . reset) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-use-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-free-variable (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-function-call-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-function (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_function_property_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-use-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-function-property-declaration (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-group (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_icon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-icon (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-keyword-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-macro (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_major_mode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"#4ade80\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-major-mode-name (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_nnoo_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-nnoo-backend (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_non_local_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . \"red\") (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-function))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-non-local-exit (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_oclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-oclosure (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"#5c9cff\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-rx (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_shadowed_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-bound-variable))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-shadowed-variable (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_shadowing_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-binding-variable))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-shadowing-variable (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_shorthand_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"cyan\") (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-keyword-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-shorthand-font-lock-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-builtin-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-slot (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_special_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-macro))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-special-form (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_special_variable_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-free-variable))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-special-variable-declaration (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_symbol_at_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . \"#00422a\") (:stipple . unspecified) (:inherit . unspecified))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-symbol-at-mouse (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_symbol_role() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"#5c9cff\") (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-function-call-face))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-symbol-role (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_symbol_role_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"#5c9cff\") (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-function-name-face))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-symbol-role-definition (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-constant-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-theme (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-thing (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_throw_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-constant-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-throw-tag (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-type (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_unknown_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"#7fa9a9\") (:background . unspecified) (:stipple . unspecified) (:inherit . elisp-function))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-unknown-call (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_variable_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-variable-at-point (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_warning_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-warning-type (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_elisp_widget_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-type-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'elisp-widget-type (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'error (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_escape_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"cyan\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'escape-glyph (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_file_name_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . shadow))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'file-name-shadow (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_fill_column_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . normal) (:slant . normal) (:underline) (:overline) (:extend . unspecified) (:strike-through) (:box) (:inverse-video) (:foreground . unspecified) (:background . unspecified) (:stipple) (:inherit . shadow))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'fill-column-indicator (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_fixed_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . \"Monospace\") (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'fixed-pitch (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_fixed_pitch_serif() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . \"Monospace Serif\") (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'fixed-pitch-serif (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_bracket_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-punctuation-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-bracket-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_builtin_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-builtin-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_comment_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-comment-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-comment-delimiter-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_comment_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-comment-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_constant_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-constant-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-punctuation-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-delimiter-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_doc_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-string-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-doc-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_doc_markup_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-constant-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-doc-markup-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_escape_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-regexp-grouping-backslash))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-escape-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_function_call_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-function-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-function-call-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_function_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-function-name-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_keyword_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-keyword-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_misc_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-punctuation-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-misc-punctuation-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_negation_char_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-negation-char-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_number_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-number-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_operator_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-operator-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_preprocessor_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-builtin-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-preprocessor-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_property_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-property-name-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_property_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-property-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-property-use-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-punctuation-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_regexp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-string-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-regexp-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_regexp_grouping_backslash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-regexp-grouping-backslash (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_regexp_grouping_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-regexp-grouping-construct (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_string_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-string-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_type_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-type-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_variable_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-variable-name-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_variable_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . font-lock-variable-name-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-variable-use-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_font_lock_warning_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . error))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'font-lock-warning-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . \"gray\") (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'fringe (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_glyphless_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . escape-glyph))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'glyphless-char (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . mode-line))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'header-line (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_header_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . header-line))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'header-line-active (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_header_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . mode-line-highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'header-line-highlight (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_header_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . header-line))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'header-line-inactive (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_help_argument_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . italic))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'help-argument-name (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_help_for_help_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . 1.26) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'help-for-help-header (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_help_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . \"grey90\") (:stipple . unspecified) (:inherit . fixed-pitch))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'help-key-binding (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'highlight (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_homoglyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"cyan\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'homoglyph (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'internal-border (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_isearch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'isearch (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_isearch_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'isearch-fail (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_isearch_group_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . isearch))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'isearch-group-1 (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_isearch_group_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . isearch))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'isearch-group-2 (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . italic) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'italic (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'lazy-highlight (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit shadow default))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'line-number (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_line_number_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . line-number))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'line-number-current-line (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_line_number_major_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . line-number))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'line-number-major-tick (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_line_number_minor_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . line-number))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'line-number-minor-tick (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . underline))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'link (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_link_visited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . link))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'link-visited (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . default))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'margin (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'match (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_menu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'menu (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_minibuffer_nonselected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"dark red\") (:background . \"yellow\") (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'minibuffer-nonselected (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"cyan\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'minibuffer-prompt (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mode-line (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mode_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . mode-line))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mode-line-active (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mode_line_buffer_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mode-line-buffer-id (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mode_line_emphasis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mode-line-emphasis (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mode_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mode-line-highlight (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mode_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . mode-line))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mode-line-inactive (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mouse (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_mouse_drag_and_drop_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . region))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'mouse-drag-and-drop-region (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . region))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'next-error (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_next_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . t) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'next-error-message (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_nobreak_hyphen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"cyan\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'nobreak-hyphen (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_nobreak_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'nobreak-space (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . isearch))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'query-replace (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_read_multiple_choice_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit help-key-binding underline))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'read-multiple-choice-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'region (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'scroll-bar (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_secondary_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'secondary-selection (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_separator_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"ForestGreen\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'separator-line (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'shadow (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_show_paren_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . underline))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'show-paren-match (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_show_paren_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . show-paren-match))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'show-paren-match-expression (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_show_paren_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'show-paren-mismatch (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'success (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . \"grey\") (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-bar (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_bar_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . tab-bar))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-bar-tab (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_bar_tab_group_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . tab-bar-tab))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-bar-tab-group-current (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_bar_tab_group_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit shadow tab-bar-tab-inactive))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-bar-tab-group-inactive (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_bar_tab_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-bar-tab-highlight (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_bar_tab_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . tab-bar-tab))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-bar-tab-inactive (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_bar_tab_ungrouped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit shadow tab-bar-tab-inactive))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-bar-tab-ungrouped (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . \"grey\") (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-line (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . tab-line))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-line-active (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tab_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . tab-line))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tab-line-inactive (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tabulated_list_fake_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . t) (:overline . t) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tabulated-list-fake-header (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box :line-width 1 :style released-button) (:inverse-video . unspecified) (:foreground . \"black\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tool-bar (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . variable-pitch))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tooltip (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_trailing_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'trailing-whitespace (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tty_menu_disabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . shadow))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tty-menu-disabled-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tty_menu_enabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tty-menu-enabled-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_tty_menu_selected_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . t) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'tty-menu-selected-face (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . t) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'underline (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_variable_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . \"Sans Serif\") (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'variable-pitch (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_variable_pitch_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . 1.1) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . variable-pitch))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'variable-pitch-text (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_conflict_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-conflict-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_edited_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-edited-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_ignored_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-ignored-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_locally_added_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-locally-added-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_locked_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-locked-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_missing_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-missing-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_needs_update_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-needs-update-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_removed_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-removed-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_state_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-state-base (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vc_up_to_date_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . vc-state-base))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vc-up-to-date-state (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_vertical_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . mode-line-inactive))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'vertical-border (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . unspecified) (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'warning (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"gray60\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'window-divider (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_window_divider_first_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"gray80\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'window-divider-first-pixel (selected-frame))",
        expect,
    );
}

#[test]
fn div_face_attr_window_divider_last_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . unspecified) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"gray40\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-all-attributes 'window-divider-last-pixel (selected-frame))",
        expect,
    );
}
