//! Per-face *face-documentation* matrix (all GNU faces).
//!

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_doc_abbrev_table_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used for displaying the abbrev table name in ‘edit-abbrevs-mode’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'abbrev-table-name)", expect);
}

#[test]
fn div_face_doc_blink_matching_paren_offscreen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'blink-matching-paren-offscreen)",
        expect,
    );
}

#[test]
fn div_face_doc_bold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic bold face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'bold)", expect);
}

#[test]
fn div_face_doc_bold_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic bold-italic face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'bold-italic)", expect);
}

#[test]
fn div_face_doc_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face for the frame border under X.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'border)", expect);
}

#[test]
fn div_face_doc_buffer_menu_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face for buffer names in the Buffer Menu.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'buffer-menu-buffer)", expect);
}

#[test]
fn div_face_doc_button() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Default face used for buttons.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'button)", expect);
}

#[test]
fn div_face_doc_child_frame_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for the internal border of child frames.\\nFor the internal border of non-child frames see ‘internal-border’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'child-frame-border)", expect);
}

#[test]
fn div_face_doc_completions_annotations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face to use for annotations in the *Completions* buffer.\\nThis face is only used if the strings used for completions\\ndoesn’t already specify a face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'completions-annotations)",
        expect,
    );
}

#[test]
fn div_face_doc_completions_common_part() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for the parts of completions which matched the pattern.\\nSee also the face ‘completions-first-difference’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'completions-common-part)",
        expect,
    );
}

#[test]
fn div_face_doc_completions_first_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for the first character after point in completions.\\nSee also the face ‘completions-common-part’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'completions-first-difference)",
        expect,
    );
}

#[test]
fn div_face_doc_completions_group_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used for the separator lines between the candidate groups.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'completions-group-separator)",
        expect,
    );
}

#[test]
fn div_face_doc_completions_group_title() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used for the title text of the candidate group headlines.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'completions-group-title)",
        expect,
    );
}

#[test]
fn div_face_doc_completions_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Default face for highlighting the current completion candidate.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'completions-highlight)",
        expect,
    );
}

#[test]
fn div_face_doc_confusingly_reordered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting text that was bidi-reordered in confusing ways.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'confusingly-reordered)",
        expect,
    );
}

#[test]
fn div_face_doc_cursor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for the cursor color under X.\\nCurrently, only the ‘:background’ attribute is meaningful; all\\nother attributes are ignored.  The cursor foreground color is\\ntaken from the background color of the underlying text.\\n\\nNote: Other faces cannot inherit from the cursor face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'cursor)", expect);
}

#[test]
fn div_face_doc_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic default face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'default)", expect);
}

#[test]
fn div_face_doc_eldoc_highlight_function_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used for the argument at point in a function’s argument list.\\nNote that this face has no effect unless the ‘eldoc-documentation-strategy’\\nhandles it explicitly.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'eldoc-highlight-function-argument)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_ampersand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting argument list markers, such as ‘&optional’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-ampersand)", expect);
}

#[test]
fn div_face_doc_elisp_binding_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting binding occurrences of variables in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-binding-variable)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_bound_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting bound occurrences of variables in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-bound-variable)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting charset names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-charset)", expect);
}

#[test]
fn div_face_doc_elisp_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting coding system names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-coding)", expect);
}

#[test]
fn div_face_doc_elisp_completion_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting completion category names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-completion-category)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_completion_category_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting completion category definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-completion-category-definition)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_condition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting ‘condition-case’ conditions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-condition)", expect);
}

#[test]
fn div_face_doc_elisp_constant() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting self-evaluating symbols in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-constant)", expect);
}

#[test]
fn div_face_doc_elisp_defcharset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting charset definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-defcharset)", expect);
}

#[test]
fn div_face_doc_elisp_defcoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting coding system definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-defcoding)", expect);
}

#[test]
fn div_face_doc_elisp_defface() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting face definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-defface)", expect);
}

#[test]
fn div_face_doc_elisp_deficon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting icon definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-deficon)", expect);
}

#[test]
fn div_face_doc_elisp_defmacro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting macro definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-defmacro)", expect);
}

#[test]
fn div_face_doc_elisp_defoclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting OClosure type definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-defoclosure)", expect);
}

#[test]
fn div_face_doc_elisp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting function definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-defun)", expect);
}

#[test]
fn div_face_doc_elisp_defvar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting variable definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-defvar)", expect);
}

#[test]
fn div_face_doc_elisp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for highlighting face names in Emacs Lisp code.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-face)", expect);
}

#[test]
fn div_face_doc_elisp_feature() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting feature names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-feature)", expect);
}

#[test]
fn div_face_doc_elisp_free_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting free (special) variables in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-free-variable)", expect);
}

#[test]
fn div_face_doc_elisp_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting function calls in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-function)", expect);
}

#[test]
fn div_face_doc_elisp_function_property_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting function/macro property declaration type names.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-function-property-declaration)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting customization group names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-group)", expect);
}

#[test]
fn div_face_doc_elisp_icon() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for highlighting icon names in Emacs Lisp code.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-icon)", expect);
}

#[test]
fn div_face_doc_elisp_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for highlighting macro calls in Emacs Lisp code.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-macro)", expect);
}

#[test]
fn div_face_doc_elisp_major_mode_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting major mode names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-major-mode-name)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_nnoo_backend() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting ‘nnoo’ backend names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-nnoo-backend)", expect);
}

#[test]
fn div_face_doc_elisp_non_local_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting calls to functions that do not return.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-non-local-exit)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_oclosure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting OClosure type names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-oclosure)", expect);
}

#[test]
fn div_face_doc_elisp_rx() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting ‘rx’ constructs in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-rx)", expect);
}

#[test]
fn div_face_doc_elisp_shadowed_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting special variables that are shadowed by a local binding.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-shadowed-variable)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_shadowing_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting local bindings that shadow special variables.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-shadowing-variable)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_shorthand_font_lock_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for highlighting shorthands in Emacs Lisp.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-shorthand-font-lock-face)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_slot() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face for highlighting EIEIO slot names.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-slot)", expect);
}

#[test]
fn div_face_doc_elisp_special_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting special forms in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-special-form)", expect);
}

#[test]
fn div_face_doc_elisp_special_variable_declaration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting free variable declarations in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-special-variable-declaration)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_symbol_at_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting the symbol at mouse in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-symbol-at-mouse)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_symbol_role() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting symbol role names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-symbol-role)", expect);
}

#[test]
fn div_face_doc_elisp_symbol_role_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting symbol role definitions in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-symbol-role-definition)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_theme() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting custom theme names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-theme)", expect);
}

#[test]
fn div_face_doc_elisp_thing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting ‘thing-at-point’ \\\"thing\\\" names in Emacs Lisp.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-thing)", expect);
}

#[test]
fn div_face_doc_elisp_throw_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting ‘catch’/‘throw’ tags in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-throw-tag)", expect);
}

#[test]
fn div_face_doc_elisp_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting object type names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-type)", expect);
}

#[test]
fn div_face_doc_elisp_unknown_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting unknown functions/macros in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-unknown-call)", expect);
}

#[test]
fn div_face_doc_elisp_variable_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting (all occurrences of) the variable at point.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'elisp-variable-at-point)",
        expect,
    );
}

#[test]
fn div_face_doc_elisp_warning_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting byte-compilation warning type names in Emacs Lisp.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-warning-type)", expect);
}

#[test]
fn div_face_doc_elisp_widget_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting widget type names in Emacs Lisp code.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'elisp-widget-type)", expect);
}

#[test]
fn div_face_doc_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face used to highlight errors and to denote failure.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'error)", expect);
}

#[test]
fn div_face_doc_escape_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for characters displayed as sequences using ‘^’ or ‘\\\\’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'escape-glyph)", expect);
}

#[test]
fn div_face_doc_file_name_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face used by ‘file-name-shadow-mode’ for the shadow.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'file-name-shadow)", expect);
}

#[test]
fn div_face_doc_fill_column_indicator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for displaying fill column indicator.\\nThis face is used when ‘display-fill-column-indicator-mode’ is\\nnon-nil.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'fill-column-indicator)",
        expect,
    );
}

#[test]
fn div_face_doc_fixed_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"The basic fixed-pitch face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'fixed-pitch)", expect);
}

#[test]
fn div_face_doc_fixed_pitch_serif() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"The basic fixed-pitch face with serifs.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'fixed-pitch-serif)", expect);
}

#[test]
fn div_face_doc_font_lock_bracket_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight brackets, braces, and parens.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-bracket-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_builtin_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight builtins.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-builtin-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_comment_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight comment delimiters.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-comment-delimiter-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_comment_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight comments.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-comment-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_constant_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight constants and labels.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-constant-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_delimiter_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight delimiters.\\nWhat exactly is a delimiter depends on the major mode, but usually\\nthese are characters like comma, colon, and semi-colon.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-delimiter-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_doc_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight documentation embedded in program code.\\nIt is typically used for special documentation comments or strings.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'font-lock-doc-face)", expect);
}

#[test]
fn div_face_doc_font_lock_doc_markup_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight embedded documentation mark-up.\\nIt is meant for mark-up elements in text that uses ‘font-lock-doc-face’, such\\nas the constructs of Haddock, Javadoc and similar systems.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-doc-markup-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_escape_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight escape sequences in strings.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-escape-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_function_call_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight function calls.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-function-call-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_function_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight function names.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-function-name-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_keyword_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight keywords.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-keyword-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_misc_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight miscellaneous punctuation.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-misc-punctuation-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_negation_char_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight easy to overlook negation.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-negation-char-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_number_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight numbers.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-number-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_operator_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight operators.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-operator-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_preprocessor_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight preprocessor directives.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-preprocessor-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_property_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight properties of an object.\\nFor example, the declaration of fields in a struct.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-property-name-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_property_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight property references.\\nFor example, property lookup of fields in a struct.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-property-use-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_punctuation_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight punctuation characters.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-punctuation-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_regexp_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight regexp literals.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-regexp-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_regexp_grouping_backslash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face for backslashes in Lisp regexp grouping constructs.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-regexp-grouping-backslash)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_regexp_grouping_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight grouping constructs in Lisp regexps.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-regexp-grouping-construct)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_string_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight strings.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-string-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_type_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight type and class names.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'font-lock-type-face)", expect);
}

#[test]
fn div_face_doc_font_lock_variable_name_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight variable names.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-variable-name-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_variable_use_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Font Lock mode face used to highlight variable references.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-variable-use-face)",
        expect,
    );
}

#[test]
fn div_face_doc_font_lock_warning_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Font Lock mode face used to highlight warnings.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'font-lock-warning-face)",
        expect,
    );
}

#[test]
fn div_face_doc_fringe() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for the fringes to the left and right of windows under X.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'fringe)", expect);
}

#[test]
fn div_face_doc_glyphless_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for displaying non-graphic characters (e.g. U+202A (LRE)).\\nIt is used for characters of no fonts too.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'glyphless-char)", expect);
}

#[test]
fn div_face_doc_header_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic header-line face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'header-line)", expect);
}

#[test]
fn div_face_doc_header_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for the selected header line.\\nThis inherits from the ‘header-line’ face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'header-line-active)", expect);
}

#[test]
fn div_face_doc_header_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic header line face for highlighting.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'header-line-highlight)",
        expect,
    );
}

#[test]
fn div_face_doc_header_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Basic header line face for non-selected windows.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'header-line-inactive)",
        expect,
    );
}

#[test]
fn div_face_doc_help_argument_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face to highlight argument names in *Help* buffers.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'help-argument-name)", expect);
}

#[test]
fn div_face_doc_help_for_help_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face used for headers in the ‘help-for-help’ buffer.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'help-for-help-header)",
        expect,
    );
}

#[test]
fn div_face_doc_help_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for keybindings in *Help* buffers.\\n\\nThis face is added by ‘substitute-command-keys’, which see.\\n\\nNote that this face will also be used for key bindings in\\ntooltips.  This means that, for example, changing the :height of\\nthis face will increase the height of any tooltip containing key\\nbindings.  See also the face ‘tooltip’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'help-key-binding)", expect);
}

#[test]
fn div_face_doc_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face for highlighting.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'highlight)", expect);
}

#[test]
fn div_face_doc_homoglyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face for lookalike characters.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'homoglyph)", expect);
}

#[test]
fn div_face_doc_internal_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for the internal border.\\nFor the internal border of child frames see ‘child-frame-border’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'internal-border)", expect);
}

#[test]
fn div_face_doc_isearch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face for highlighting Isearch matches.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'isearch)", expect);
}

#[test]
fn div_face_doc_isearch_fail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting failed part in Isearch echo-area message.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'isearch-fail)", expect);
}

#[test]
fn div_face_doc_isearch_group_1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for highlighting Isearch the odd group matches.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'isearch-group-1)", expect);
}

#[test]
fn div_face_doc_isearch_group_2() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for highlighting Isearch the even group matches.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'isearch-group-2)", expect);
}

#[test]
fn div_face_doc_italic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic italic face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'italic)", expect);
}

#[test]
fn div_face_doc_lazy_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for lazy highlighting of matches other than the current one.\\nUsed in Isearch when ‘isearch-lazy-highlight’ is non-nil,\\nand in ‘query-replace’ when ‘query-replace-lazy-highlight’ is non-nil.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'lazy-highlight)", expect);
}

#[test]
fn div_face_doc_line_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for displaying line numbers.\\nThis face is used when ‘display-line-numbers’ is non-nil.\\n\\nIf you customize the font of this face, make sure it is a\\nmonospaced font, otherwise line numbers will not line up,\\nand text lines might move horizontally as you move through\\nthe buffer.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'line-number)", expect);
}

#[test]
fn div_face_doc_line_number_current_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for displaying the current line number.\\nThis face is used when ‘display-line-numbers’ is non-nil.\\n\\nIf you customize the font of this face, make sure it is a\\nmonospaced font, otherwise line numbers will not line up,\\nand text lines might move horizontally as you move through\\nthe buffer.  Similarly, making this face’s font different\\nfrom that of the ‘line-number’ face could produce such\\nunwanted effects.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'line-number-current-line)",
        expect,
    );
}

#[test]
fn div_face_doc_line_number_major_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting \\\"major ticks\\\" (as in a ruler).\\nWhen ‘display-line-numbers-major-tick’ is positive, highlight\\nthe line numbers of lines which are a multiple of its value.\\nThis face is used when ‘display-line-numbers’ is non-nil.\\n\\nIf you customize the font of this face, make sure it is a\\nmonospaced font, otherwise line numbers will not line up,\\nand text lines might move horizontally as you move through\\nthe buffer.  Similarly, making this face’s font different\\nfrom that of the ‘line-number’ face could produce such\\nunwanted effects.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'line-number-major-tick)",
        expect,
    );
}

#[test]
fn div_face_doc_line_number_minor_tick() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting \\\"minor ticks\\\" (as in a ruler).\\nWhen ‘display-line-numbers-minor-tick’ is positive, highlight\\nthe line numbers of lines which are a multiple of its value.\\nThis face is used when ‘display-line-numbers’ is non-nil.\\n\\nIf you customize the font of this face, make sure it is a\\nmonospaced font, otherwise line numbers will not line up,\\nand text lines might move horizontally as you move through\\nthe buffer.  Similarly, making this face’s font different\\nfrom that of the ‘line-number’ face could produce such\\nunwanted effects.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'line-number-minor-tick)",
        expect,
    );
}

#[test]
fn div_face_doc_link() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face for unvisited links.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'link)", expect);
}

#[test]
fn div_face_doc_link_visited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face for visited links.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'link-visited)", expect);
}

#[test]
fn div_face_doc_margin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for window margins (both left and right).\\nThis face is used to customize the appearance of the margin areas.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'margin)", expect);
}

#[test]
fn div_face_doc_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face used to highlight matches permanently.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'match)", expect);
}

#[test]
fn div_face_doc_menu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for the font and colors of the menu bar and popup menus.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'menu)", expect);
}

#[test]
fn div_face_doc_minibuffer_nonselected() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting contents of non-selected minibuffer window.\\nUsed by ‘minibuffer-nonselected-mode’ for the contents of the minibuffer\\nwindow when the minibuffer remains active but its window is currently\\nnot selected.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'minibuffer-nonselected)",
        expect,
    );
}

#[test]
fn div_face_doc_minibuffer_prompt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for minibuffer prompts.\\nBy default, Emacs automatically adds this face to the value of\\n‘minibuffer-prompt-properties’, which is a list of text properties\\nused to display the prompt text.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'minibuffer-prompt)", expect);
}

#[test]
fn div_face_doc_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for the mode lines as well as header lines.\\nSee ‘mode-line-active’ and ‘mode-line-inactive’ for the faces\\nused on mode lines.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'mode-line)", expect);
}

#[test]
fn div_face_doc_mode_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for the selected mode line.\\nThis inherits from the ‘mode-line’ face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'mode-line-active)", expect);
}

#[test]
fn div_face_doc_mode_line_buffer_id() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used for buffer identification parts of the mode line.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'mode-line-buffer-id)", expect);
}

#[test]
fn div_face_doc_mode_line_emphasis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used to emphasize certain mode line features.\\nUse the face ‘mode-line-highlight’ for features that can be selected.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'mode-line-emphasis)", expect);
}

#[test]
fn div_face_doc_mode_line_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic mode line face for highlighting.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'mode-line-highlight)", expect);
}

#[test]
fn div_face_doc_mode_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Basic mode line face for non-selected windows.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'mode-line-inactive)", expect);
}

#[test]
fn div_face_doc_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face for the mouse color under X.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'mouse)", expect);
}

#[test]
fn div_face_doc_mouse_drag_and_drop_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face to highlight original text during dragging.\\nThis face is used by ‘mouse-drag-and-drop-region’ to temporarily\\nhighlight the original region when\\n‘mouse-drag-and-drop-region-show-cursor’ is non-nil.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'mouse-drag-and-drop-region)",
        expect,
    );
}

#[test]
fn div_face_doc_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face used to highlight next error locus.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'next-error)", expect);
}

#[test]
fn div_face_doc_next_error_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used to highlight the current error message in the ‘next-error’ buffer.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'next-error-message)", expect);
}

#[test]
fn div_face_doc_nobreak_hyphen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face for displaying nobreak hyphens.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'nobreak-hyphen)", expect);
}

#[test]
fn div_face_doc_nobreak_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face for displaying nobreak space.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'nobreak-space)", expect);
}

#[test]
fn div_face_doc_query_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for highlighting query replacement matches.\\nUsed in ‘query-replace’ and ‘query-replace-regexp’\\nwhen ‘query-replace-highlight’ is non-nil\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'query-replace)", expect);
}

#[test]
fn div_face_doc_read_multiple_choice_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for the symbol name in ‘read-multiple-choice’ output.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'read-multiple-choice-face)",
        expect,
    );
}

#[test]
fn div_face_doc_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face for highlighting the region.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'region)", expect);
}

#[test]
fn div_face_doc_scroll_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Basic face for the scroll bar colors under X.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'scroll-bar)", expect);
}

#[test]
fn div_face_doc_secondary_selection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Basic face for displaying the secondary selection.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'secondary-selection)", expect);
}

#[test]
fn div_face_doc_separator_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face for separator lines.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'separator-line)", expect);
}

#[test]
fn div_face_doc_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face for shadowed text.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'shadow)", expect);
}

#[test]
fn div_face_doc_show_paren_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face used for a matching paren.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'show-paren-match)", expect);
}

#[test]
fn div_face_doc_show_paren_match_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face used for a matching paren when highlighting the whole expression.\\nThis face is used by ‘show-paren-mode’.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'show-paren-match-expression)",
        expect,
    );
}

#[test]
fn div_face_doc_show_paren_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face used for a mismatching paren.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'show-paren-mismatch)", expect);
}

#[test]
fn div_face_doc_success() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Basic face used to indicate successful operation.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'success)", expect);
}

#[test]
fn div_face_doc_tab_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Tab bar face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'tab-bar)", expect);
}

#[test]
fn div_face_doc_tab_bar_tab() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Tab bar face for selected tab.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'tab-bar-tab)", expect);
}

#[test]
fn div_face_doc_tab_bar_tab_group_current() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Tab bar face for current group tab.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tab-bar-tab-group-current)",
        expect,
    );
}

#[test]
fn div_face_doc_tab_bar_tab_group_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Tab bar face for inactive group tab.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tab-bar-tab-group-inactive)",
        expect,
    );
}

#[test]
fn div_face_doc_tab_bar_tab_highlight() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Tab bar face for highlighting.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tab-bar-tab-highlight)",
        expect,
    );
}

#[test]
fn div_face_doc_tab_bar_tab_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Tab bar face for non-selected tab.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tab-bar-tab-inactive)",
        expect,
    );
}

#[test]
fn div_face_doc_tab_bar_tab_ungrouped() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Tab bar face for ungrouped tab when tab groups are used.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tab-bar-tab-ungrouped)",
        expect,
    );
}

#[test]
fn div_face_doc_tab_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic tab line face.\\nSee ‘tab-line-active’ and ‘tab-line-inactive’ for the faces\\nused on tab lines.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'tab-line)", expect);
}

#[test]
fn div_face_doc_tab_line_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for the selected tab line.\\nThis inherits from the ‘tab-line’ face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'tab-line-active)", expect);
}

#[test]
fn div_face_doc_tab_line_inactive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Basic tab line face for non-selected windows.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'tab-line-inactive)", expect);
}

#[test]
fn div_face_doc_tabulated_list_fake_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Face used on fake header lines.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tabulated-list-fake-header)",
        expect,
    );
}

#[test]
fn div_face_doc_tool_bar() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic tool-bar face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'tool-bar)", expect);
}

#[test]
fn div_face_doc_tooltip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for tooltips.\\n\\nWhen using the GTK toolkit, NS, or Haiku, this face will only\\nbe used if ‘use-system-tooltips’ is nil.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'tooltip)", expect);
}

#[test]
fn div_face_doc_trailing_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Basic face for highlighting trailing whitespace.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'trailing-whitespace)", expect);
}

#[test]
fn div_face_doc_tty_menu_disabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for displaying disabled items in TTY menus.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tty-menu-disabled-face)",
        expect,
    );
}

#[test]
fn div_face_doc_tty_menu_enabled_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for displaying enabled items in TTY menus.\"""#]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tty-menu-enabled-face)",
        expect,
    );
}

#[test]
fn div_face_doc_tty_menu_selected_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for displaying the currently selected item in TTY menus.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'tty-menu-selected-face)",
        expect,
    );
}

#[test]
fn div_face_doc_underline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic underlined face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'underline)", expect);
}

#[test]
fn div_face_doc_variable_pitch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"The basic variable-pitch face.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'variable-pitch)", expect);
}

#[test]
fn div_face_doc_variable_pitch_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"The proportional face used for longer texts.\\nThis is like the ‘variable-pitch’ face, but is slightly bigger by\\ndefault.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'variable-pitch-text)", expect);
}

#[test]
fn div_face_doc_vc_conflict_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for VC modeline state when the file contains merge conflicts.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-conflict-state)", expect);
}

#[test]
fn div_face_doc_vc_edited_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for VC modeline state when the file is edited.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-edited-state)", expect);
}

#[test]
fn div_face_doc_vc_ignored_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for VC modeline state when the file is registered, but ignored.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-ignored-state)", expect);
}

#[test]
fn div_face_doc_vc_locally_added_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for VC modeline state when the file is locally added.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'vc-locally-added-state)",
        expect,
    );
}

#[test]
fn div_face_doc_vc_locked_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face for VC modeline state when the file locked.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-locked-state)", expect);
}

#[test]
fn div_face_doc_vc_missing_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for VC modeline state when the file is missing from the file system.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-missing-state)", expect);
}

#[test]
fn div_face_doc_vc_needs_update_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for VC modeline state when the file needs update.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'vc-needs-update-state)",
        expect,
    );
}

#[test]
fn div_face_doc_vc_removed_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for VC modeline state when the file was removed from the VC system.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-removed-state)", expect);
}

#[test]
fn div_face_doc_vc_state_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Base face for VC state indicator.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-state-base)", expect);
}

#[test]
fn div_face_doc_vc_up_to_date_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Face for VC modeline state when the file is up to date.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vc-up-to-date-state)", expect);
}

#[test]
fn div_face_doc_vertical_border() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"Face used for vertical window dividers on ttys.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'vertical-border)", expect);
}

#[test]
fn div_face_doc_warning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic face used to highlight warnings.\"""#]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'warning)", expect);
}

#[test]
fn div_face_doc_window_divider() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for window dividers.\\nWhen a divider is less than 3 pixels wide, it is drawn solidly\\nwith the foreground of this face.  For larger dividers this face\\nis used for the inner part while the first pixel line/column is\\ndrawn with the ‘window-divider-first-pixel’ face and the last\\npixel line/column with the ‘window-divider-last-pixel’ face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect("(face-documentation 'window-divider)", expect);
}

#[test]
fn div_face_doc_window_divider_first_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for first pixel line/column of window dividers.\\nWhen a divider is at least 3 pixels wide, its first pixel\\nline/column is drawn with the foreground of this face.  If you do\\nnot want to accentuate the first pixel line/column, set this to\\nthe same as ‘window-divider’ face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'window-divider-first-pixel)",
        expect,
    );
}

#[test]
fn div_face_doc_window_divider_last_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"Basic face for last pixel line/column of window dividers.\\nWhen a divider is at least 3 pixels wide, its last pixel\\nline/column is drawn with the foreground of this face.  If you do\\nnot want to accentuate the last pixel line/column, set this to\\nthe same as ‘window-divider’ face.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(face-documentation 'window-divider-last-pixel)",
        expect,
    );
}
