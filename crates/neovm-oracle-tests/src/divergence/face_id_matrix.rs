//! Per-face *face-id* matrix (all GNU faces).
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_id_abbrev_table_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 68""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'abbrev-table-name)", expect);
}

#[test]
fn div_face_id_blink_matching_paren_offscreen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 74""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'blink-matching-paren-offscreen)", expect);
}

#[test]
fn div_face_id_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'bold)", expect);
}

#[test]
fn div_face_id_bold_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'bold-italic)", expect);
}

#[test]
fn div_face_id_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 45""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'border)", expect);
}

#[test]
fn div_face_id_buffer_menu_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 126""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'buffer-menu-buffer)", expect);
}

#[test]
fn div_face_id_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 67""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'button)", expect);
}

#[test]
fn div_face_id_child_frame_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 40""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'child-frame-border)", expect);
}

#[test]
fn div_face_id_completions_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 77""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'completions-annotations)", expect);
}

#[test]
fn div_face_id_completions_common_part() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 80""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'completions-common-part)", expect);
}

#[test]
fn div_face_id_completions_first_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 79""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'completions-first-difference)", expect);
}

#[test]
fn div_face_id_completions_group_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 76""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'completions-group-separator)", expect);
}

#[test]
fn div_face_id_completions_group_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 75""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'completions-group-title)", expect);
}

#[test]
fn div_face_id_completions_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 78""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'completions-highlight)", expect);
}

#[test]
fn div_face_id_confusingly_reordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 70""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'confusingly-reordered)", expect);
}

#[test]
fn div_face_id_cursor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 46""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'cursor)", expect);
}

#[test]
fn div_face_id_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'default)", expect);
}

#[test]
fn div_face_id_eldoc_highlight_function_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 184""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'eldoc-highlight-function-argument)",
        expect,
    );
}

#[test]
fn div_face_id_elisp_ampersand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 157""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-ampersand)", expect);
}

#[test]
fn div_face_id_elisp_binding_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 144""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-binding-variable)", expect);
}

#[test]
fn div_face_id_elisp_bound_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 145""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-bound-variable)", expect);
}

#[test]
fn div_face_id_elisp_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 169""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-charset)", expect);
}

#[test]
fn div_face_id_elisp_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 167""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-coding)", expect);
}

#[test]
fn div_face_id_elisp_completion_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 171""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-completion-category)", expect);
}

#[test]
fn div_face_id_elisp_completion_category_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 172""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'elisp-completion-category-definition)",
        expect,
    );
}

#[test]
fn div_face_id_elisp_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 130""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-condition)", expect);
}

#[test]
fn div_face_id_elisp_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 158""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-constant)", expect);
}

#[test]
fn div_face_id_elisp_defcharset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 170""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-defcharset)", expect);
}

#[test]
fn div_face_id_elisp_defcoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 168""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-defcoding)", expect);
}

#[test]
fn div_face_id_elisp_defface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 162""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-defface)", expect);
}

#[test]
fn div_face_id_elisp_deficon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 164""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-deficon)", expect);
}

#[test]
fn div_face_id_elisp_defmacro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 160""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-defmacro)", expect);
}

#[test]
fn div_face_id_elisp_defoclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 166""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-defoclosure)", expect);
}

#[test]
fn div_face_id_elisp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 159""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-defun)", expect);
}

#[test]
fn div_face_id_elisp_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 161""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-defvar)", expect);
}

#[test]
fn div_face_id_elisp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 132""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-face)", expect);
}

#[test]
fn div_face_id_elisp_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 141""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-feature)", expect);
}

#[test]
fn div_face_id_elisp_free_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 128""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-free-variable)", expect);
}

#[test]
fn div_face_id_elisp_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 135""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-function)", expect);
}

#[test]
fn div_face_id_elisp_function_property_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 150""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'elisp-function-property-declaration)",
        expect,
    );
}

#[test]
fn div_face_id_elisp_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 155""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-group)", expect);
}

#[test]
fn div_face_id_elisp_icon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 163""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-icon)", expect);
}

#[test]
fn div_face_id_elisp_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 138""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-macro)", expect);
}

#[test]
fn div_face_id_elisp_major_mode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 131""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-major-mode-name)", expect);
}

#[test]
fn div_face_id_elisp_nnoo_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 156""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-nnoo-backend)", expect);
}

#[test]
fn div_face_id_elisp_non_local_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 136""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-non-local-exit)", expect);
}

#[test]
fn div_face_id_elisp_oclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 165""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-oclosure)", expect);
}

#[test]
fn div_face_id_elisp_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 142""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-rx)", expect);
}

#[test]
fn div_face_id_elisp_shadowed_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 147""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-shadowed-variable)", expect);
}

#[test]
fn div_face_id_elisp_shadowing_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 146""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-shadowing-variable)", expect);
}

#[test]
fn div_face_id_elisp_shorthand_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 183""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-shorthand-font-lock-face)", expect);
}

#[test]
fn div_face_id_elisp_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 152""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-slot)", expect);
}

#[test]
fn div_face_id_elisp_special_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 139""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-special-form)", expect);
}

#[test]
fn div_face_id_elisp_special_variable_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 129""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'elisp-special-variable-declaration)",
        expect,
    );
}

#[test]
fn div_face_id_elisp_symbol_at_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 127""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-symbol-at-mouse)", expect);
}

#[test]
fn div_face_id_elisp_symbol_role() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 133""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-symbol-role)", expect);
}

#[test]
fn div_face_id_elisp_symbol_role_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 134""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-symbol-role-definition)", expect);
}

#[test]
fn div_face_id_elisp_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 143""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-theme)", expect);
}

#[test]
fn div_face_id_elisp_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 151""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-thing)", expect);
}

#[test]
fn div_face_id_elisp_throw_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 140""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-throw-tag)", expect);
}

#[test]
fn div_face_id_elisp_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 154""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-type)", expect);
}

#[test]
fn div_face_id_elisp_unknown_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 137""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-unknown-call)", expect);
}

#[test]
fn div_face_id_elisp_variable_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 148""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-variable-at-point)", expect);
}

#[test]
fn div_face_id_elisp_warning_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 149""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-warning-type)", expect);
}

#[test]
fn div_face_id_elisp_widget_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 153""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'elisp-widget-type)", expect);
}

#[test]
fn div_face_id_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 57""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'error)", expect);
}

#[test]
fn div_face_id_escape_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 21""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'escape-glyph)", expect);
}

#[test]
fn div_face_id_file_name_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 116""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'file-name-shadow)", expect);
}

#[test]
fn div_face_id_fill_column_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 20""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'fill-column-indicator)", expect);
}

#[test]
fn div_face_id_fixed_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'fixed-pitch)", expect);
}

#[test]
fn div_face_id_fixed_pitch_serif() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'fixed-pitch-serif)", expect);
}

#[test]
fn div_face_id_font_lock_bracket_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 107""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-bracket-face)", expect);
}

#[test]
fn div_face_id_font_lock_builtin_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 88""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-builtin-face)", expect);
}

#[test]
fn div_face_id_font_lock_comment_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 83""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'font-lock-comment-delimiter-face)",
        expect,
    );
}

#[test]
fn div_face_id_font_lock_comment_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 82""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-comment-face)", expect);
}

#[test]
fn div_face_id_font_lock_constant_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-constant-face)", expect);
}

#[test]
fn div_face_id_font_lock_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 108""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-delimiter-face)", expect);
}

#[test]
fn div_face_id_font_lock_doc_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 85""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-doc-face)", expect);
}

#[test]
fn div_face_id_font_lock_doc_markup_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 86""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-doc-markup-face)", expect);
}

#[test]
fn div_face_id_font_lock_escape_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 101""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-escape-face)", expect);
}

#[test]
fn div_face_id_font_lock_function_call_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 90""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-function-call-face)", expect);
}

#[test]
fn div_face_id_font_lock_function_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 89""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-function-name-face)", expect);
}

#[test]
fn div_face_id_font_lock_keyword_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 87""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-keyword-face)", expect);
}

#[test]
fn div_face_id_font_lock_misc_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 109""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'font-lock-misc-punctuation-face)",
        expect,
    );
}

#[test]
fn div_face_id_font_lock_negation_char_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-negation-char-face)", expect);
}

#[test]
fn div_face_id_font_lock_number_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 102""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-number-face)", expect);
}

#[test]
fn div_face_id_font_lock_operator_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 103""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-operator-face)", expect);
}

#[test]
fn div_face_id_font_lock_preprocessor_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 97""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-preprocessor-face)", expect);
}

#[test]
fn div_face_id_font_lock_property_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 104""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-property-name-face)", expect);
}

#[test]
fn div_face_id_font_lock_property_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 105""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-property-use-face)", expect);
}

#[test]
fn div_face_id_font_lock_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 106""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-punctuation-face)", expect);
}

#[test]
fn div_face_id_font_lock_regexp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 98""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-regexp-face)", expect);
}

#[test]
fn div_face_id_font_lock_regexp_grouping_backslash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'font-lock-regexp-grouping-backslash)",
        expect,
    );
}

#[test]
fn div_face_id_font_lock_regexp_grouping_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 100""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-id 'font-lock-regexp-grouping-construct)",
        expect,
    );
}

#[test]
fn div_face_id_font_lock_string_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 84""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-string-face)", expect);
}

#[test]
fn div_face_id_font_lock_type_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 93""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-type-face)", expect);
}

#[test]
fn div_face_id_font_lock_variable_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 91""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-variable-name-face)", expect);
}

#[test]
fn div_face_id_font_lock_variable_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 92""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-variable-use-face)", expect);
}

#[test]
fn div_face_id_font_lock_warning_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 95""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'font-lock-warning-face)", expect);
}

#[test]
fn div_face_id_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 43""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'fringe)", expect);
}

#[test]
fn div_face_id_glyphless_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 56""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'glyphless-char)", expect);
}

#[test]
fn div_face_id_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 31""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'header-line)", expect);
}

#[test]
fn div_face_id_header_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 33""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'header-line-active)", expect);
}

#[test]
fn div_face_id_header_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 32""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'header-line-highlight)", expect);
}

#[test]
fn div_face_id_header_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 34""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'header-line-inactive)", expect);
}

#[test]
fn div_face_id_help_argument_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 54""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'help-argument-name)", expect);
}

#[test]
fn div_face_id_help_for_help_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 69""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'help-for-help-header)", expect);
}

#[test]
fn div_face_id_help_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 55""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'help-key-binding)", expect);
}

#[test]
fn div_face_id_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 12""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'highlight)", expect);
}

#[test]
fn div_face_id_homoglyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 22""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'homoglyph)", expect);
}

#[test]
fn div_face_id_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 39""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'internal-border)", expect);
}

#[test]
fn div_face_id_isearch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 111""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'isearch)", expect);
}

#[test]
fn div_face_id_isearch_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 112""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'isearch-fail)", expect);
}

#[test]
fn div_face_id_isearch_group_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 114""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'isearch-group-1)", expect);
}

#[test]
fn div_face_id_isearch_group_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 115""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'isearch-group-2)", expect);
}

#[test]
fn div_face_id_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'italic)", expect);
}

#[test]
fn div_face_id_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 113""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'lazy-highlight)", expect);
}

#[test]
fn div_face_id_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 16""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'line-number)", expect);
}

#[test]
fn div_face_id_line_number_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 17""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'line-number-current-line)", expect);
}

#[test]
fn div_face_id_line_number_major_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 18""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'line-number-major-tick)", expect);
}

#[test]
fn div_face_id_line_number_minor_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 19""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'line-number-minor-tick)", expect);
}

#[test]
fn div_face_id_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'link)", expect);
}

#[test]
fn div_face_id_link_visited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 11""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'link-visited)", expect);
}

#[test]
fn div_face_id_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'margin)", expect);
}

#[test]
fn div_face_id_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 124""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'match)", expect);
}

#[test]
fn div_face_id_menu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 53""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'menu)", expect);
}

#[test]
fn div_face_id_minibuffer_nonselected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 81""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'minibuffer-nonselected)", expect);
}

#[test]
fn div_face_id_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 41""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'minibuffer-prompt)", expect);
}

#[test]
fn div_face_id_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 25""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mode-line)", expect);
}

#[test]
fn div_face_id_mode_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 26""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mode-line-active)", expect);
}

#[test]
fn div_face_id_mode_line_buffer_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 30""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mode-line-buffer-id)", expect);
}

#[test]
fn div_face_id_mode_line_emphasis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 29""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mode-line-emphasis)", expect);
}

#[test]
fn div_face_id_mode_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 28""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mode-line-highlight)", expect);
}

#[test]
fn div_face_id_mode_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 27""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mode-line-inactive)", expect);
}

#[test]
fn div_face_id_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 47""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mouse)", expect);
}

#[test]
fn div_face_id_mouse_drag_and_drop_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 110""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'mouse-drag-and-drop-region)", expect);
}

#[test]
fn div_face_id_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 71""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'next-error)", expect);
}

#[test]
fn div_face_id_next_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 72""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'next-error-message)", expect);
}

#[test]
fn div_face_id_nobreak_hyphen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 24""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'nobreak-hyphen)", expect);
}

#[test]
fn div_face_id_nobreak_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 23""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'nobreak-space)", expect);
}

#[test]
fn div_face_id_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 123""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'query-replace)", expect);
}

#[test]
fn div_face_id_read_multiple_choice_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 60""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'read-multiple-choice-face)", expect);
}

#[test]
fn div_face_id_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 13""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'region)", expect);
}

#[test]
fn div_face_id_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 44""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'scroll-bar)", expect);
}

#[test]
fn div_face_id_secondary_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 14""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'secondary-selection)", expect);
}

#[test]
fn div_face_id_separator_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 73""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'separator-line)", expect);
}

#[test]
fn div_face_id_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 9""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'shadow)", expect);
}

#[test]
fn div_face_id_show_paren_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 64""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'show-paren-match)", expect);
}

#[test]
fn div_face_id_show_paren_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 65""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'show-paren-match-expression)", expect);
}

#[test]
fn div_face_id_show_paren_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 66""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'show-paren-mismatch)", expect);
}

#[test]
fn div_face_id_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 59""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'success)", expect);
}

#[test]
fn div_face_id_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 49""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-bar)", expect);
}

#[test]
fn div_face_id_tab_bar_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 117""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-bar-tab)", expect);
}

#[test]
fn div_face_id_tab_bar_tab_group_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 119""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-bar-tab-group-current)", expect);
}

#[test]
fn div_face_id_tab_bar_tab_group_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 120""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-bar-tab-group-inactive)", expect);
}

#[test]
fn div_face_id_tab_bar_tab_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 122""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-bar-tab-highlight)", expect);
}

#[test]
fn div_face_id_tab_bar_tab_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 118""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-bar-tab-inactive)", expect);
}

#[test]
fn div_face_id_tab_bar_tab_ungrouped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 121""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-bar-tab-ungrouped)", expect);
}

#[test]
fn div_face_id_tab_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 50""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-line)", expect);
}

#[test]
fn div_face_id_tab_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 51""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-line-active)", expect);
}

#[test]
fn div_face_id_tab_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 52""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tab-line-inactive)", expect);
}

#[test]
fn div_face_id_tabulated_list_fake_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 125""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tabulated-list-fake-header)", expect);
}

#[test]
fn div_face_id_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 48""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tool-bar)", expect);
}

#[test]
fn div_face_id_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 185""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tooltip)", expect);
}

#[test]
fn div_face_id_trailing_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 15""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'trailing-whitespace)", expect);
}

#[test]
fn div_face_id_tty_menu_disabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 62""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tty-menu-disabled-face)", expect);
}

#[test]
fn div_face_id_tty_menu_enabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 61""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tty-menu-enabled-face)", expect);
}

#[test]
fn div_face_id_tty_menu_selected_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 63""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'tty-menu-selected-face)", expect);
}

#[test]
fn div_face_id_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'underline)", expect);
}

#[test]
fn div_face_id_variable_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'variable-pitch)", expect);
}

#[test]
fn div_face_id_variable_pitch_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 8""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'variable-pitch-text)", expect);
}

#[test]
fn div_face_id_vc_conflict_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 178""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-conflict-state)", expect);
}

#[test]
fn div_face_id_vc_edited_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 181""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-edited-state)", expect);
}

#[test]
fn div_face_id_vc_ignored_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 182""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-ignored-state)", expect);
}

#[test]
fn div_face_id_vc_locally_added_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 177""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-locally-added-state)", expect);
}

#[test]
fn div_face_id_vc_locked_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 176""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-locked-state)", expect);
}

#[test]
fn div_face_id_vc_missing_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 180""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-missing-state)", expect);
}

#[test]
fn div_face_id_vc_needs_update_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 175""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-needs-update-state)", expect);
}

#[test]
fn div_face_id_vc_removed_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 179""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-removed-state)", expect);
}

#[test]
fn div_face_id_vc_state_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 173""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-state-base)", expect);
}

#[test]
fn div_face_id_vc_up_to_date_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 174""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vc-up-to-date-state)", expect);
}

#[test]
fn div_face_id_vertical_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 35""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'vertical-border)", expect);
}

#[test]
fn div_face_id_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 58""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'warning)", expect);
}

#[test]
fn div_face_id_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 36""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'window-divider)", expect);
}

#[test]
fn div_face_id_window_divider_first_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 37""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'window-divider-first-pixel)", expect);
}

#[test]
fn div_face_id_window_divider_last_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 38""#]];
    crate::common::assert_oracle_parity_expect("(face-id 'window-divider-last-pixel)", expect);
}
