//! Per-face *face-attribute :underline* matrix.
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_under_abbrev_table_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'abbrev-table-name :underline)",
        expect,
    );
}

#[test]
fn div_face_under_blink_matching_paren_offscreen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'blink-matching-paren-offscreen :underline)",
        expect,
    );
}

#[test]
fn div_face_under_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold :underline)", expect);
}

#[test]
fn div_face_under_bold_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold-italic :underline)", expect);
}

#[test]
fn div_face_under_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'border :underline)", expect);
}

#[test]
fn div_face_under_buffer_menu_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'buffer-menu-buffer :underline)",
        expect,
    );
}

#[test]
fn div_face_under_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'button :underline)", expect);
}

#[test]
fn div_face_under_child_frame_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'child-frame-border :underline)",
        expect,
    );
}

#[test]
fn div_face_under_completions_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-annotations :underline)",
        expect,
    );
}

#[test]
fn div_face_under_completions_common_part() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-common-part :underline)",
        expect,
    );
}

#[test]
fn div_face_under_completions_first_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-first-difference :underline)",
        expect,
    );
}

#[test]
fn div_face_under_completions_group_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-separator :underline)",
        expect,
    );
}

#[test]
fn div_face_under_completions_group_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-title :underline)",
        expect,
    );
}

#[test]
fn div_face_under_completions_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-highlight :underline)",
        expect,
    );
}

#[test]
fn div_face_under_confusingly_reordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'confusingly-reordered :underline)",
        expect,
    );
}

#[test]
fn div_face_under_cursor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'cursor :underline)", expect);
}

#[test]
fn div_face_under_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'default :underline)", expect);
}

#[test]
fn div_face_under_eldoc_highlight_function_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'eldoc-highlight-function-argument :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_ampersand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-ampersand :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_binding_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-binding-variable :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_bound_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-bound-variable :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-charset :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-coding :underline)", expect);
}

#[test]
fn div_face_under_elisp_completion_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_completion_category_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category-definition :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-condition :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-constant :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_defcharset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defcharset :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_defcoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defcoding :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_defface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defface :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_deficon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-deficon :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_defmacro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defmacro :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_defoclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defoclosure :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defun :underline)", expect);
}

#[test]
fn div_face_under_elisp_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defvar :underline)", expect);
}

#[test]
fn div_face_under_elisp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-face :underline)", expect);
}

#[test]
fn div_face_under_elisp_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-feature :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_free_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-free-variable :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-function :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_function_property_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-function-property-declaration :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-group :underline)", expect);
}

#[test]
fn div_face_under_elisp_icon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-icon :underline)", expect);
}

#[test]
fn div_face_under_elisp_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-macro :underline)", expect);
}

#[test]
fn div_face_under_elisp_major_mode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-major-mode-name :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_nnoo_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-nnoo-backend :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_non_local_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"red\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-non-local-exit :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_oclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-oclosure :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-rx :underline)", expect);
}

#[test]
fn div_face_under_elisp_shadowed_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowed-variable :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_shadowing_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowing-variable :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_shorthand_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shorthand-font-lock-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-slot :underline)", expect);
}

#[test]
fn div_face_under_elisp_special_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-form :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_special_variable_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-variable-declaration :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_symbol_at_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-at-mouse :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_symbol_role() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_symbol_role_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role-definition :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-theme :underline)", expect);
}

#[test]
fn div_face_under_elisp_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-thing :underline)", expect);
}

#[test]
fn div_face_under_elisp_throw_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-throw-tag :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-type :underline)", expect);
}

#[test]
fn div_face_under_elisp_unknown_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-unknown-call :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_variable_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-variable-at-point :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_warning_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-warning-type :underline)",
        expect,
    );
}

#[test]
fn div_face_under_elisp_widget_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-widget-type :underline)",
        expect,
    );
}

#[test]
fn div_face_under_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'error :underline)", expect);
}

#[test]
fn div_face_under_escape_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'escape-glyph :underline)", expect);
}

#[test]
fn div_face_under_file_name_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'file-name-shadow :underline)",
        expect,
    );
}

#[test]
fn div_face_under_fill_column_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fill-column-indicator :underline)",
        expect,
    );
}

#[test]
fn div_face_under_fixed_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fixed-pitch :underline)", expect);
}

#[test]
fn div_face_under_fixed_pitch_serif() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fixed-pitch-serif :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_bracket_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-bracket-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_builtin_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-builtin-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_comment_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-delimiter-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_comment_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_constant_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-constant-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-delimiter-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_doc_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_doc_markup_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-markup-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_escape_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-escape-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_function_call_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-call-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_function_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-name-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_keyword_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-keyword-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_misc_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-misc-punctuation-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_negation_char_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-negation-char-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_number_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-number-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_operator_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-operator-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_preprocessor_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-preprocessor-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_property_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-name-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_property_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-use-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-punctuation-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_regexp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_regexp_grouping_backslash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-backslash :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_regexp_grouping_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-construct :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_string_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-string-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_type_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-type-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_variable_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-name-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_variable_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-use-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_font_lock_warning_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-warning-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fringe :underline)", expect);
}

#[test]
fn div_face_under_glyphless_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'glyphless-char :underline)",
        expect,
    );
}

#[test]
fn div_face_under_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'header-line :underline)", expect);
}

#[test]
fn div_face_under_header_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-active :underline)",
        expect,
    );
}

#[test]
fn div_face_under_header_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-highlight :underline)",
        expect,
    );
}

#[test]
fn div_face_under_header_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-inactive :underline)",
        expect,
    );
}

#[test]
fn div_face_under_help_argument_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-argument-name :underline)",
        expect,
    );
}

#[test]
fn div_face_under_help_for_help_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-for-help-header :underline)",
        expect,
    );
}

#[test]
fn div_face_under_help_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-key-binding :underline)",
        expect,
    );
}

#[test]
fn div_face_under_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'highlight :underline)", expect);
}

#[test]
fn div_face_under_homoglyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'homoglyph :underline)", expect);
}

#[test]
fn div_face_under_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'internal-border :underline)",
        expect,
    );
}

#[test]
fn div_face_under_isearch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch :underline)", expect);
}

#[test]
fn div_face_under_isearch_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch-fail :underline)", expect);
}

#[test]
fn div_face_under_isearch_group_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'isearch-group-1 :underline)",
        expect,
    );
}

#[test]
fn div_face_under_isearch_group_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'isearch-group-2 :underline)",
        expect,
    );
}

#[test]
fn div_face_under_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'italic :underline)", expect);
}

#[test]
fn div_face_under_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'lazy-highlight :underline)",
        expect,
    );
}

#[test]
fn div_face_under_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'line-number :underline)", expect);
}

#[test]
fn div_face_under_line_number_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-current-line :underline)",
        expect,
    );
}

#[test]
fn div_face_under_line_number_major_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-major-tick :underline)",
        expect,
    );
}

#[test]
fn div_face_under_line_number_minor_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-minor-tick :underline)",
        expect,
    );
}

#[test]
fn div_face_under_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'link :underline)", expect);
}

#[test]
fn div_face_under_link_visited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'link-visited :underline)", expect);
}

#[test]
fn div_face_under_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'margin :underline)", expect);
}

#[test]
fn div_face_under_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'match :underline)", expect);
}

#[test]
fn div_face_under_menu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'menu :underline)", expect);
}

#[test]
fn div_face_under_minibuffer_nonselected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-nonselected :underline)",
        expect,
    );
}

#[test]
fn div_face_under_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-prompt :underline)",
        expect,
    );
}

#[test]
fn div_face_under_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mode-line :underline)", expect);
}

#[test]
fn div_face_under_mode_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-active :underline)",
        expect,
    );
}

#[test]
fn div_face_under_mode_line_buffer_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-buffer-id :underline)",
        expect,
    );
}

#[test]
fn div_face_under_mode_line_emphasis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-emphasis :underline)",
        expect,
    );
}

#[test]
fn div_face_under_mode_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-highlight :underline)",
        expect,
    );
}

#[test]
fn div_face_under_mode_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-inactive :underline)",
        expect,
    );
}

#[test]
fn div_face_under_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mouse :underline)", expect);
}

#[test]
fn div_face_under_mouse_drag_and_drop_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mouse-drag-and-drop-region :underline)",
        expect,
    );
}

#[test]
fn div_face_under_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'next-error :underline)", expect);
}

#[test]
fn div_face_under_next_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'next-error-message :underline)",
        expect,
    );
}

#[test]
fn div_face_under_nobreak_hyphen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'nobreak-hyphen :underline)",
        expect,
    );
}

#[test]
fn div_face_under_nobreak_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'nobreak-space :underline)",
        expect,
    );
}

#[test]
fn div_face_under_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'query-replace :underline)",
        expect,
    );
}

#[test]
fn div_face_under_read_multiple_choice_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'read-multiple-choice-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'region :underline)", expect);
}

#[test]
fn div_face_under_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'scroll-bar :underline)", expect);
}

#[test]
fn div_face_under_secondary_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'secondary-selection :underline)",
        expect,
    );
}

#[test]
fn div_face_under_separator_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'separator-line :underline)",
        expect,
    );
}

#[test]
fn div_face_under_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'shadow :underline)", expect);
}

#[test]
fn div_face_under_show_paren_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match :underline)",
        expect,
    );
}

#[test]
fn div_face_under_show_paren_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match-expression :underline)",
        expect,
    );
}

#[test]
fn div_face_under_show_paren_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-mismatch :underline)",
        expect,
    );
}

#[test]
fn div_face_under_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'success :underline)", expect);
}

#[test]
fn div_face_under_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar :underline)", expect);
}

#[test]
fn div_face_under_tab_bar_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar-tab :underline)", expect);
}

#[test]
fn div_face_under_tab_bar_tab_group_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-current :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tab_bar_tab_group_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-inactive :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tab_bar_tab_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-highlight :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tab_bar_tab_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-inactive :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tab_bar_tab_ungrouped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-ungrouped :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tab_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-line :underline)", expect);
}

#[test]
fn div_face_under_tab_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-line-active :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tab_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-line-inactive :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tabulated_list_fake_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tabulated-list-fake-header :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tool-bar :underline)", expect);
}

#[test]
fn div_face_under_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tooltip :underline)", expect);
}

#[test]
fn div_face_under_trailing_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'trailing-whitespace :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tty_menu_disabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-disabled-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tty_menu_enabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-enabled-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_tty_menu_selected_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-selected-face :underline)",
        expect,
    );
}

#[test]
fn div_face_under_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'underline :underline)", expect);
}

#[test]
fn div_face_under_variable_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'variable-pitch :underline)",
        expect,
    );
}

#[test]
fn div_face_under_variable_pitch_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'variable-pitch-text :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_conflict_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-conflict-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_edited_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-edited-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_ignored_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-ignored-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_locally_added_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-locally-added-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_locked_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-locked-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_missing_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-missing-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_needs_update_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-needs-update-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_removed_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-removed-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_state_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-state-base :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vc_up_to_date_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-up-to-date-state :underline)",
        expect,
    );
}

#[test]
fn div_face_under_vertical_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vertical-border :underline)",
        expect,
    );
}

#[test]
fn div_face_under_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'warning :underline)", expect);
}

#[test]
fn div_face_under_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider :underline)",
        expect,
    );
}

#[test]
fn div_face_under_window_divider_first_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-first-pixel :underline)",
        expect,
    );
}

#[test]
fn div_face_under_window_divider_last_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-last-pixel :underline)",
        expect,
    );
}
