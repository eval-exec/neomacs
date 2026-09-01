//! Per-face *face-attribute :height* matrix (all GNU faces).
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_height_abbrev_table_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'abbrev-table-name :height)",
        expect,
    );
}

#[test]
fn div_face_height_blink_matching_paren_offscreen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'blink-matching-paren-offscreen :height)",
        expect,
    );
}

#[test]
fn div_face_height_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold :height)", expect);
}

#[test]
fn div_face_height_bold_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold-italic :height)", expect);
}

#[test]
fn div_face_height_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'border :height)", expect);
}

#[test]
fn div_face_height_buffer_menu_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'buffer-menu-buffer :height)",
        expect,
    );
}

#[test]
fn div_face_height_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'button :height)", expect);
}

#[test]
fn div_face_height_child_frame_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'child-frame-border :height)",
        expect,
    );
}

#[test]
fn div_face_height_completions_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-annotations :height)",
        expect,
    );
}

#[test]
fn div_face_height_completions_common_part() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-common-part :height)",
        expect,
    );
}

#[test]
fn div_face_height_completions_first_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-first-difference :height)",
        expect,
    );
}

#[test]
fn div_face_height_completions_group_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-separator :height)",
        expect,
    );
}

#[test]
fn div_face_height_completions_group_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-title :height)",
        expect,
    );
}

#[test]
fn div_face_height_completions_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-highlight :height)",
        expect,
    );
}

#[test]
fn div_face_height_confusingly_reordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'confusingly-reordered :height)",
        expect,
    );
}

#[test]
fn div_face_height_cursor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'cursor :height)", expect);
}

#[test]
fn div_face_height_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'default :height)", expect);
}

#[test]
fn div_face_height_eldoc_highlight_function_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'eldoc-highlight-function-argument :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_ampersand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-ampersand :height)", expect);
}

#[test]
fn div_face_height_elisp_binding_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-binding-variable :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_bound_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-bound-variable :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-charset :height)", expect);
}

#[test]
fn div_face_height_elisp_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-coding :height)", expect);
}

#[test]
fn div_face_height_elisp_completion_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_completion_category_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category-definition :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-condition :height)", expect);
}

#[test]
fn div_face_height_elisp_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-constant :height)", expect);
}

#[test]
fn div_face_height_elisp_defcharset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defcharset :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_defcoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defcoding :height)", expect);
}

#[test]
fn div_face_height_elisp_defface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defface :height)", expect);
}

#[test]
fn div_face_height_elisp_deficon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-deficon :height)", expect);
}

#[test]
fn div_face_height_elisp_defmacro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defmacro :height)", expect);
}

#[test]
fn div_face_height_elisp_defoclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defoclosure :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defun :height)", expect);
}

#[test]
fn div_face_height_elisp_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defvar :height)", expect);
}

#[test]
fn div_face_height_elisp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-face :height)", expect);
}

#[test]
fn div_face_height_elisp_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-feature :height)", expect);
}

#[test]
fn div_face_height_elisp_free_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-free-variable :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-function :height)", expect);
}

#[test]
fn div_face_height_elisp_function_property_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-function-property-declaration :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-group :height)", expect);
}

#[test]
fn div_face_height_elisp_icon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-icon :height)", expect);
}

#[test]
fn div_face_height_elisp_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-macro :height)", expect);
}

#[test]
fn div_face_height_elisp_major_mode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-major-mode-name :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_nnoo_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-nnoo-backend :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_non_local_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-non-local-exit :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_oclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-oclosure :height)", expect);
}

#[test]
fn div_face_height_elisp_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-rx :height)", expect);
}

#[test]
fn div_face_height_elisp_shadowed_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowed-variable :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_shadowing_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowing-variable :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_shorthand_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shorthand-font-lock-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-slot :height)", expect);
}

#[test]
fn div_face_height_elisp_special_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-form :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_special_variable_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-variable-declaration :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_symbol_at_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-at-mouse :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_symbol_role() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_symbol_role_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role-definition :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-theme :height)", expect);
}

#[test]
fn div_face_height_elisp_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-thing :height)", expect);
}

#[test]
fn div_face_height_elisp_throw_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-throw-tag :height)", expect);
}

#[test]
fn div_face_height_elisp_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-type :height)", expect);
}

#[test]
fn div_face_height_elisp_unknown_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-unknown-call :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_variable_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-variable-at-point :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_warning_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-warning-type :height)",
        expect,
    );
}

#[test]
fn div_face_height_elisp_widget_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-widget-type :height)",
        expect,
    );
}

#[test]
fn div_face_height_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'error :height)", expect);
}

#[test]
fn div_face_height_escape_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'escape-glyph :height)", expect);
}

#[test]
fn div_face_height_file_name_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'file-name-shadow :height)",
        expect,
    );
}

#[test]
fn div_face_height_fill_column_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fill-column-indicator :height)",
        expect,
    );
}

#[test]
fn div_face_height_fixed_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fixed-pitch :height)", expect);
}

#[test]
fn div_face_height_fixed_pitch_serif() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fixed-pitch-serif :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_bracket_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-bracket-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_builtin_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-builtin-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_comment_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-delimiter-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_comment_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_constant_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-constant-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-delimiter-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_doc_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_doc_markup_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-markup-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_escape_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-escape-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_function_call_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-call-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_function_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-name-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_keyword_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-keyword-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_misc_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-misc-punctuation-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_negation_char_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-negation-char-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_number_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-number-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_operator_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-operator-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_preprocessor_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-preprocessor-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_property_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-name-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_property_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-use-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-punctuation-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_regexp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_regexp_grouping_backslash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-backslash :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_regexp_grouping_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-construct :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_string_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-string-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_type_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-type-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_variable_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-name-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_variable_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-use-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_font_lock_warning_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-warning-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fringe :height)", expect);
}

#[test]
fn div_face_height_glyphless_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'glyphless-char :height)", expect);
}

#[test]
fn div_face_height_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'header-line :height)", expect);
}

#[test]
fn div_face_height_header_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-active :height)",
        expect,
    );
}

#[test]
fn div_face_height_header_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-highlight :height)",
        expect,
    );
}

#[test]
fn div_face_height_header_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-inactive :height)",
        expect,
    );
}

#[test]
fn div_face_height_help_argument_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-argument-name :height)",
        expect,
    );
}

#[test]
fn div_face_height_help_for_help_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1.26""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-for-help-header :height)",
        expect,
    );
}

#[test]
fn div_face_height_help_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-key-binding :height)",
        expect,
    );
}

#[test]
fn div_face_height_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'highlight :height)", expect);
}

#[test]
fn div_face_height_homoglyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'homoglyph :height)", expect);
}

#[test]
fn div_face_height_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'internal-border :height)", expect);
}

#[test]
fn div_face_height_isearch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch :height)", expect);
}

#[test]
fn div_face_height_isearch_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch-fail :height)", expect);
}

#[test]
fn div_face_height_isearch_group_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch-group-1 :height)", expect);
}

#[test]
fn div_face_height_isearch_group_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch-group-2 :height)", expect);
}

#[test]
fn div_face_height_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'italic :height)", expect);
}

#[test]
fn div_face_height_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'lazy-highlight :height)", expect);
}

#[test]
fn div_face_height_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'line-number :height)", expect);
}

#[test]
fn div_face_height_line_number_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-current-line :height)",
        expect,
    );
}

#[test]
fn div_face_height_line_number_major_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-major-tick :height)",
        expect,
    );
}

#[test]
fn div_face_height_line_number_minor_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-minor-tick :height)",
        expect,
    );
}

#[test]
fn div_face_height_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'link :height)", expect);
}

#[test]
fn div_face_height_link_visited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'link-visited :height)", expect);
}

#[test]
fn div_face_height_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'margin :height)", expect);
}

#[test]
fn div_face_height_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'match :height)", expect);
}

#[test]
fn div_face_height_menu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'menu :height)", expect);
}

#[test]
fn div_face_height_minibuffer_nonselected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-nonselected :height)",
        expect,
    );
}

#[test]
fn div_face_height_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-prompt :height)",
        expect,
    );
}

#[test]
fn div_face_height_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mode-line :height)", expect);
}

#[test]
fn div_face_height_mode_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-active :height)",
        expect,
    );
}

#[test]
fn div_face_height_mode_line_buffer_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-buffer-id :height)",
        expect,
    );
}

#[test]
fn div_face_height_mode_line_emphasis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-emphasis :height)",
        expect,
    );
}

#[test]
fn div_face_height_mode_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-highlight :height)",
        expect,
    );
}

#[test]
fn div_face_height_mode_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-inactive :height)",
        expect,
    );
}

#[test]
fn div_face_height_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mouse :height)", expect);
}

#[test]
fn div_face_height_mouse_drag_and_drop_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mouse-drag-and-drop-region :height)",
        expect,
    );
}

#[test]
fn div_face_height_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'next-error :height)", expect);
}

#[test]
fn div_face_height_next_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'next-error-message :height)",
        expect,
    );
}

#[test]
fn div_face_height_nobreak_hyphen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'nobreak-hyphen :height)", expect);
}

#[test]
fn div_face_height_nobreak_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'nobreak-space :height)", expect);
}

#[test]
fn div_face_height_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'query-replace :height)", expect);
}

#[test]
fn div_face_height_read_multiple_choice_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'read-multiple-choice-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'region :height)", expect);
}

#[test]
fn div_face_height_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'scroll-bar :height)", expect);
}

#[test]
fn div_face_height_secondary_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'secondary-selection :height)",
        expect,
    );
}

#[test]
fn div_face_height_separator_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'separator-line :height)", expect);
}

#[test]
fn div_face_height_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'shadow :height)", expect);
}

#[test]
fn div_face_height_show_paren_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match :height)",
        expect,
    );
}

#[test]
fn div_face_height_show_paren_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match-expression :height)",
        expect,
    );
}

#[test]
fn div_face_height_show_paren_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-mismatch :height)",
        expect,
    );
}

#[test]
fn div_face_height_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'success :height)", expect);
}

#[test]
fn div_face_height_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar :height)", expect);
}

#[test]
fn div_face_height_tab_bar_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar-tab :height)", expect);
}

#[test]
fn div_face_height_tab_bar_tab_group_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-current :height)",
        expect,
    );
}

#[test]
fn div_face_height_tab_bar_tab_group_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-inactive :height)",
        expect,
    );
}

#[test]
fn div_face_height_tab_bar_tab_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-highlight :height)",
        expect,
    );
}

#[test]
fn div_face_height_tab_bar_tab_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-inactive :height)",
        expect,
    );
}

#[test]
fn div_face_height_tab_bar_tab_ungrouped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-ungrouped :height)",
        expect,
    );
}

#[test]
fn div_face_height_tab_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-line :height)", expect);
}

#[test]
fn div_face_height_tab_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-line-active :height)", expect);
}

#[test]
fn div_face_height_tab_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-line-inactive :height)",
        expect,
    );
}

#[test]
fn div_face_height_tabulated_list_fake_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tabulated-list-fake-header :height)",
        expect,
    );
}

#[test]
fn div_face_height_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tool-bar :height)", expect);
}

#[test]
fn div_face_height_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tooltip :height)", expect);
}

#[test]
fn div_face_height_trailing_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'trailing-whitespace :height)",
        expect,
    );
}

#[test]
fn div_face_height_tty_menu_disabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-disabled-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_tty_menu_enabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-enabled-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_tty_menu_selected_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-selected-face :height)",
        expect,
    );
}

#[test]
fn div_face_height_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'underline :height)", expect);
}

#[test]
fn div_face_height_variable_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'variable-pitch :height)", expect);
}

#[test]
fn div_face_height_variable_pitch_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1.1""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'variable-pitch-text :height)",
        expect,
    );
}

#[test]
fn div_face_height_vc_conflict_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-conflict-state :height)",
        expect,
    );
}

#[test]
fn div_face_height_vc_edited_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vc-edited-state :height)", expect);
}

#[test]
fn div_face_height_vc_ignored_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-ignored-state :height)",
        expect,
    );
}

#[test]
fn div_face_height_vc_locally_added_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-locally-added-state :height)",
        expect,
    );
}

#[test]
fn div_face_height_vc_locked_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vc-locked-state :height)", expect);
}

#[test]
fn div_face_height_vc_missing_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-missing-state :height)",
        expect,
    );
}

#[test]
fn div_face_height_vc_needs_update_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-needs-update-state :height)",
        expect,
    );
}

#[test]
fn div_face_height_vc_removed_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-removed-state :height)",
        expect,
    );
}

#[test]
fn div_face_height_vc_state_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vc-state-base :height)", expect);
}

#[test]
fn div_face_height_vc_up_to_date_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-up-to-date-state :height)",
        expect,
    );
}

#[test]
fn div_face_height_vertical_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vertical-border :height)", expect);
}

#[test]
fn div_face_height_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'warning :height)", expect);
}

#[test]
fn div_face_height_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'window-divider :height)", expect);
}

#[test]
fn div_face_height_window_divider_first_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-first-pixel :height)",
        expect,
    );
}

#[test]
fn div_face_height_window_divider_last_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-last-pixel :height)",
        expect,
    );
}
