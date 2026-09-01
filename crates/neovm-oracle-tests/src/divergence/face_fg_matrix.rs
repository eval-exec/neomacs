//! Per-face *face-attribute :foreground* matrix (all GNU faces).
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_fg_abbrev_table_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'abbrev-table-name :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_blink_matching_paren_offscreen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'blink-matching-paren-offscreen :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold :foreground)", expect);
}

#[test]
fn div_face_fg_bold_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'bold-italic :foreground)", expect);
}

#[test]
fn div_face_fg_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'border :foreground)", expect);
}

#[test]
fn div_face_fg_buffer_menu_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'buffer-menu-buffer :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'button :foreground)", expect);
}

#[test]
fn div_face_fg_child_frame_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'child-frame-border :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_completions_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-annotations :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_completions_common_part() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-common-part :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_completions_first_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-first-difference :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_completions_group_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-separator :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_completions_group_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-group-title :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_completions_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'completions-highlight :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_confusingly_reordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'confusingly-reordered :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_cursor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'cursor :foreground)", expect);
}

#[test]
fn div_face_fg_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"unspecified-fg\"""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'default :foreground)", expect);
}

#[test]
fn div_face_fg_eldoc_highlight_function_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'eldoc-highlight-function-argument :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_ampersand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-ampersand :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_binding_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-binding-variable :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_bound_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK reset""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-bound-variable :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-charset :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-coding :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_completion_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_completion_category_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-completion-category-definition :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"red\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-condition :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-constant :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_defcharset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defcharset :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_defcoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defcoding :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_defface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defface :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_deficon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-deficon :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_defmacro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defmacro :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_defoclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defoclosure :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-defun :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-defvar :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-face :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-feature :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_free_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK reset""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-free-variable :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-function :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_function_property_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-function-property-declaration :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-group :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_icon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-icon :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-macro :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_major_mode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#4ade80\"""##]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-major-mode-name :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_nnoo_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-nnoo-backend :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_non_local_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-non-local-exit :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_oclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-oclosure :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#5c9cff\"""##]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-rx :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_shadowed_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowed-variable :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_shadowing_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shadowing-variable :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_shorthand_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"cyan\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-shorthand-font-lock-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-slot :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_special_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-form :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_special_variable_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-special-variable-declaration :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_symbol_at_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-at-mouse :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_symbol_role() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#5c9cff\"""##]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_symbol_role_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#5c9cff\"""##]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-symbol-role-definition :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-theme :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-thing :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_throw_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-throw-tag :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'elisp-type :foreground)", expect);
}

#[test]
fn div_face_fg_elisp_unknown_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#7fa9a9\"""##]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-unknown-call :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_variable_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-variable-at-point :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_warning_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-warning-type :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_elisp_widget_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'elisp-widget-type :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'error :foreground)", expect);
}

#[test]
fn div_face_fg_escape_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"cyan\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'escape-glyph :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_file_name_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'file-name-shadow :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_fill_column_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fill-column-indicator :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_fixed_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fixed-pitch :foreground)", expect);
}

#[test]
fn div_face_fg_fixed_pitch_serif() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'fixed-pitch-serif :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_bracket_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-bracket-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_builtin_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-builtin-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_comment_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-delimiter-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_comment_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-comment-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_constant_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-constant-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-delimiter-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_doc_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_doc_markup_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-doc-markup-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_escape_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-escape-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_function_call_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-call-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_function_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-function-name-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_keyword_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-keyword-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_misc_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-misc-punctuation-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_negation_char_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-negation-char-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_number_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-number-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_operator_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-operator-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_preprocessor_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-preprocessor-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_property_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-name-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_property_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-property-use-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-punctuation-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_regexp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_regexp_grouping_backslash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-backslash :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_regexp_grouping_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-regexp-grouping-construct :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_string_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-string-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_type_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-type-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_variable_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-name-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_variable_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-variable-use-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_font_lock_warning_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'font-lock-warning-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'fringe :foreground)", expect);
}

#[test]
fn div_face_fg_glyphless_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'glyphless-char :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'header-line :foreground)", expect);
}

#[test]
fn div_face_fg_header_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-active :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_header_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-highlight :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_header_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'header-line-inactive :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_help_argument_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-argument-name :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_help_for_help_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-for-help-header :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_help_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'help-key-binding :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'highlight :foreground)", expect);
}

#[test]
fn div_face_fg_homoglyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"cyan\"""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'homoglyph :foreground)", expect);
}

#[test]
fn div_face_fg_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'internal-border :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_isearch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'isearch :foreground)", expect);
}

#[test]
fn div_face_fg_isearch_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'isearch-fail :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_isearch_group_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'isearch-group-1 :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_isearch_group_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'isearch-group-2 :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'italic :foreground)", expect);
}

#[test]
fn div_face_fg_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'lazy-highlight :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'line-number :foreground)", expect);
}

#[test]
fn div_face_fg_line_number_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-current-line :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_line_number_major_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-major-tick :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_line_number_minor_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'line-number-minor-tick :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'link :foreground)", expect);
}

#[test]
fn div_face_fg_link_visited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'link-visited :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'margin :foreground)", expect);
}

#[test]
fn div_face_fg_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'match :foreground)", expect);
}

#[test]
fn div_face_fg_menu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'menu :foreground)", expect);
}

#[test]
fn div_face_fg_minibuffer_nonselected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"dark red\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-nonselected :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"cyan\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'minibuffer-prompt :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mode-line :foreground)", expect);
}

#[test]
fn div_face_fg_mode_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-active :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_mode_line_buffer_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-buffer-id :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_mode_line_emphasis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-emphasis :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_mode_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-highlight :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_mode_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mode-line-inactive :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'mouse :foreground)", expect);
}

#[test]
fn div_face_fg_mouse_drag_and_drop_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'mouse-drag-and-drop-region :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'next-error :foreground)", expect);
}

#[test]
fn div_face_fg_next_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'next-error-message :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_nobreak_hyphen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"cyan\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'nobreak-hyphen :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_nobreak_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'nobreak-space :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'query-replace :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_read_multiple_choice_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'read-multiple-choice-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'region :foreground)", expect);
}

#[test]
fn div_face_fg_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'scroll-bar :foreground)", expect);
}

#[test]
fn div_face_fg_secondary_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'secondary-selection :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_separator_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ForestGreen\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'separator-line :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'shadow :foreground)", expect);
}

#[test]
fn div_face_fg_show_paren_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_show_paren_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-match-expression :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_show_paren_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'show-paren-mismatch :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'success :foreground)", expect);
}

#[test]
fn div_face_fg_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar :foreground)", expect);
}

#[test]
fn div_face_fg_tab_bar_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-bar-tab :foreground)", expect);
}

#[test]
fn div_face_fg_tab_bar_tab_group_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-current :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tab_bar_tab_group_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-group-inactive :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tab_bar_tab_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-highlight :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tab_bar_tab_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-inactive :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tab_bar_tab_ungrouped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-bar-tab-ungrouped :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tab_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tab-line :foreground)", expect);
}

#[test]
fn div_face_fg_tab_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-line-active :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tab_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tab-line-inactive :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tabulated_list_fake_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tabulated-list-fake-header :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"black\"""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tool-bar :foreground)", expect);
}

#[test]
fn div_face_fg_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'tooltip :foreground)", expect);
}

#[test]
fn div_face_fg_trailing_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'trailing-whitespace :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tty_menu_disabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-disabled-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tty_menu_enabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-enabled-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_tty_menu_selected_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'tty-menu-selected-face :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'underline :foreground)", expect);
}

#[test]
fn div_face_fg_variable_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'variable-pitch :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_variable_pitch_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'variable-pitch-text :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_conflict_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-conflict-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_edited_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-edited-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_ignored_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-ignored-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_locally_added_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-locally-added-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_locked_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-locked-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_missing_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-missing-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_needs_update_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-needs-update-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_removed_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-removed-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_state_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-state-base :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vc_up_to_date_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vc-up-to-date-state :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_vertical_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'vertical-border :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK unspecified""#]];
    crate::common::assert_oracle_parity_expect("(face-attribute 'warning :foreground)", expect);
}

#[test]
fn div_face_fg_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"gray60\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_window_divider_first_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"gray80\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-first-pixel :foreground)",
        expect,
    );
}

#[test]
fn div_face_fg_window_divider_last_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"gray40\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-attribute 'window-divider-last-pixel :foreground)",
        expect,
    );
}
