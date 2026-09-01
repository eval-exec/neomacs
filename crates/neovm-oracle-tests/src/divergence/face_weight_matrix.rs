//! Per-face *face-attribute :weight* matrix.
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_weight_abbrev_table_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'abbrev-table-name :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_blink_matching_paren_offscreen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'blink-matching-paren-offscreen :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold :weight)", expect);
}

#[test]
fn div_face_weight_bold_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold-italic :weight)", expect);
}

#[test]
fn div_face_weight_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'border :weight)", expect);
}

#[test]
fn div_face_weight_buffer_menu_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'buffer-menu-buffer :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'button :weight)", expect);
}

#[test]
fn div_face_weight_child_frame_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'child-frame-border :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_completions_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-annotations :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_completions_common_part() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-common-part :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_completions_first_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-first-difference :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_completions_group_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-separator :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_completions_group_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-title :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_completions_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-highlight :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_confusingly_reordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'confusingly-reordered :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_cursor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'cursor :weight)", expect);
}

#[test]
fn div_face_weight_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK normal""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'default :weight)", expect);
}

#[test]
fn div_face_weight_eldoc_highlight_function_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'eldoc-highlight-function-argument :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_ampersand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-ampersand :weight)", expect);
}

#[test]
fn div_face_weight_elisp_binding_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-binding-variable :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_bound_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-bound-variable :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-charset :weight)", expect);
}

#[test]
fn div_face_weight_elisp_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-coding :weight)", expect);
}

#[test]
fn div_face_weight_elisp_completion_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_completion_category_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category-definition :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-condition :weight)", expect);
}

#[test]
fn div_face_weight_elisp_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-constant :weight)", expect);
}

#[test]
fn div_face_weight_elisp_defcharset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defcharset :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_defcoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defcoding :weight)", expect);
}

#[test]
fn div_face_weight_elisp_defface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defface :weight)", expect);
}

#[test]
fn div_face_weight_elisp_deficon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-deficon :weight)", expect);
}

#[test]
fn div_face_weight_elisp_defmacro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defmacro :weight)", expect);
}

#[test]
fn div_face_weight_elisp_defoclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defoclosure :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defun :weight)", expect);
}

#[test]
fn div_face_weight_elisp_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defvar :weight)", expect);
}

#[test]
fn div_face_weight_elisp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-face :weight)", expect);
}

#[test]
fn div_face_weight_elisp_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-feature :weight)", expect);
}

#[test]
fn div_face_weight_elisp_free_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-free-variable :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-function :weight)", expect);
}

#[test]
fn div_face_weight_elisp_function_property_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-function-property-declaration :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-group :weight)", expect);
}

#[test]
fn div_face_weight_elisp_icon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-icon :weight)", expect);
}

#[test]
fn div_face_weight_elisp_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-macro :weight)", expect);
}

#[test]
fn div_face_weight_elisp_major_mode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-major-mode-name :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_nnoo_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-nnoo-backend :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_non_local_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-non-local-exit :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_oclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-oclosure :weight)", expect);
}

#[test]
fn div_face_weight_elisp_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-rx :weight)", expect);
}

#[test]
fn div_face_weight_elisp_shadowed_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowed-variable :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_shadowing_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowing-variable :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_shorthand_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shorthand-font-lock-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-slot :weight)", expect);
}

#[test]
fn div_face_weight_elisp_special_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-form :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_special_variable_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-variable-declaration :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_symbol_at_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-at-mouse :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_symbol_role() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_symbol_role_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role-definition :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-theme :weight)", expect);
}

#[test]
fn div_face_weight_elisp_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-thing :weight)", expect);
}

#[test]
fn div_face_weight_elisp_throw_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-throw-tag :weight)", expect);
}

#[test]
fn div_face_weight_elisp_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-type :weight)", expect);
}

#[test]
fn div_face_weight_elisp_unknown_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-unknown-call :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_variable_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-variable-at-point :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_warning_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-warning-type :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_elisp_widget_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-widget-type :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'error :weight)", expect);
}

#[test]
fn div_face_weight_escape_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'escape-glyph :weight)", expect);
}

#[test]
fn div_face_weight_file_name_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'file-name-shadow :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_fill_column_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK normal""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fill-column-indicator :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_fixed_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fixed-pitch :weight)", expect);
}

#[test]
fn div_face_weight_fixed_pitch_serif() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fixed-pitch-serif :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_bracket_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-bracket-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_builtin_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-builtin-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_comment_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-delimiter-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_comment_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_constant_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-constant-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-delimiter-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_doc_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_doc_markup_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-markup-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_escape_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-escape-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_function_call_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-call-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_function_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-name-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_keyword_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-keyword-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_misc_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-misc-punctuation-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_negation_char_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-negation-char-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_number_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-number-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_operator_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-operator-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_preprocessor_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-preprocessor-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_property_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-name-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_property_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-use-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-punctuation-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_regexp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_regexp_grouping_backslash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-backslash :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_regexp_grouping_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-construct :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_string_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-string-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_type_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-type-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_variable_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-name-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_variable_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-use-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_font_lock_warning_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-warning-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fringe :weight)", expect);
}

#[test]
fn div_face_weight_glyphless_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'glyphless-char :weight)", expect);
}

#[test]
fn div_face_weight_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'header-line :weight)", expect);
}

#[test]
fn div_face_weight_header_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-active :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_header_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-highlight :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_header_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-inactive :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_help_argument_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-argument-name :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_help_for_help_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-for-help-header :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_help_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-key-binding :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'highlight :weight)", expect);
}

#[test]
fn div_face_weight_homoglyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'homoglyph :weight)", expect);
}

#[test]
fn div_face_weight_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'internal-border :weight)", expect);
}

#[test]
fn div_face_weight_isearch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch :weight)", expect);
}

#[test]
fn div_face_weight_isearch_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch-fail :weight)", expect);
}

#[test]
fn div_face_weight_isearch_group_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch-group-1 :weight)", expect);
}

#[test]
fn div_face_weight_isearch_group_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch-group-2 :weight)", expect);
}

#[test]
fn div_face_weight_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'italic :weight)", expect);
}

#[test]
fn div_face_weight_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'lazy-highlight :weight)", expect);
}

#[test]
fn div_face_weight_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'line-number :weight)", expect);
}

#[test]
fn div_face_weight_line_number_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-current-line :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_line_number_major_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-major-tick :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_line_number_minor_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-minor-tick :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'link :weight)", expect);
}

#[test]
fn div_face_weight_link_visited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'link-visited :weight)", expect);
}

#[test]
fn div_face_weight_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'margin :weight)", expect);
}

#[test]
fn div_face_weight_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'match :weight)", expect);
}

#[test]
fn div_face_weight_menu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'menu :weight)", expect);
}

#[test]
fn div_face_weight_minibuffer_nonselected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-nonselected :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-prompt :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mode-line :weight)", expect);
}

#[test]
fn div_face_weight_mode_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-active :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_mode_line_buffer_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-buffer-id :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_mode_line_emphasis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-emphasis :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_mode_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-highlight :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_mode_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-inactive :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mouse :weight)", expect);
}

#[test]
fn div_face_weight_mouse_drag_and_drop_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mouse-drag-and-drop-region :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'next-error :weight)", expect);
}

#[test]
fn div_face_weight_next_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'next-error-message :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_nobreak_hyphen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'nobreak-hyphen :weight)", expect);
}

#[test]
fn div_face_weight_nobreak_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'nobreak-space :weight)", expect);
}

#[test]
fn div_face_weight_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'query-replace :weight)", expect);
}

#[test]
fn div_face_weight_read_multiple_choice_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'read-multiple-choice-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'region :weight)", expect);
}

#[test]
fn div_face_weight_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'scroll-bar :weight)", expect);
}

#[test]
fn div_face_weight_secondary_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'secondary-selection :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_separator_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'separator-line :weight)", expect);
}

#[test]
fn div_face_weight_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'shadow :weight)", expect);
}

#[test]
fn div_face_weight_show_paren_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_show_paren_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match-expression :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_show_paren_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-mismatch :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'success :weight)", expect);
}

#[test]
fn div_face_weight_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar :weight)", expect);
}

#[test]
fn div_face_weight_tab_bar_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar-tab :weight)", expect);
}

#[test]
fn div_face_weight_tab_bar_tab_group_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-current :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tab_bar_tab_group_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-inactive :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tab_bar_tab_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-highlight :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tab_bar_tab_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-inactive :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tab_bar_tab_ungrouped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-ungrouped :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tab_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-line :weight)", expect);
}

#[test]
fn div_face_weight_tab_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-line-active :weight)", expect);
}

#[test]
fn div_face_weight_tab_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-line-inactive :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tabulated_list_fake_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tabulated-list-fake-header :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tool-bar :weight)", expect);
}

#[test]
fn div_face_weight_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tooltip :weight)", expect);
}

#[test]
fn div_face_weight_trailing_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'trailing-whitespace :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tty_menu_disabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-disabled-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tty_menu_enabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-enabled-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_tty_menu_selected_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-selected-face :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'underline :weight)", expect);
}

#[test]
fn div_face_weight_variable_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'variable-pitch :weight)", expect);
}

#[test]
fn div_face_weight_variable_pitch_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'variable-pitch-text :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vc_conflict_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-conflict-state :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vc_edited_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vc-edited-state :weight)", expect);
}

#[test]
fn div_face_weight_vc_ignored_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-ignored-state :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vc_locally_added_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-locally-added-state :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vc_locked_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vc-locked-state :weight)", expect);
}

#[test]
fn div_face_weight_vc_missing_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-missing-state :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vc_needs_update_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-needs-update-state :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vc_removed_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-removed-state :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vc_state_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vc-state-base :weight)", expect);
}

#[test]
fn div_face_weight_vc_up_to_date_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-up-to-date-state :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_vertical_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'vertical-border :weight)", expect);
}

#[test]
fn div_face_weight_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'warning :weight)", expect);
}

#[test]
fn div_face_weight_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'window-divider :weight)", expect);
}

#[test]
fn div_face_weight_window_divider_first_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-first-pixel :weight)",
        expect,
    );
}

#[test]
fn div_face_weight_window_divider_last_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-last-pixel :weight)",
        expect,
    );
}
