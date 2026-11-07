//! Oracle parity for the text-property control variables the casing /
//! expansion inheritance path reads.
//!
//! GNU's C reads go through the DEFVAR C globals that
//! `set_buffer_internal_2` (src/buffer.c:2342ff) swaps for the current
//! buffer's forwarded binding and that `specbind` writes for a dynamic
//! `let` (src/eval.c:3674ff, src/data.c find_symbol_value):
//!   text-property-default-nonsticky  DEFVAR_LISP src/textprop.c:2417,
//!     Fmake_variable_buffer_local :2430, read `Vtext_property_default_-
//!     nonsticky` src/intervals.c:878/1064/1120;
//!   char-property-alias-alist        DEFVAR_LISP src/textprop.c:2403,
//!     read src/intervals.c:1733 (made buffer-local by font-core.el);
//!   default-text-properties          DEFVAR_LISP src/textprop.c:2397,
//!     read src/intervals.c:1741-1742 (local in enriched-mode).
//! Each class gets a setq-local case, a dynamic-let case that overrides a
//! local binding, and a default-read control. The expects are GNU 31.1
//! answers recorded with NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_casing_textprop_default_nonsticky_setq_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq-local text-property-default-nonsticky '((syntax-table . nil)))
  (insert "ß")
  (put-text-property 1 2 'syntax-table (string-to-syntax "w"))
  (upcase-region 1 2)
  (list (buffer-string)
        (get-text-property 1 'syntax-table)
        (get-text-property 2 'syntax-table)))
"#;
    let expect = expect_test::expect![[r#""OK (#(\"SS\" 0 2 (syntax-table (2))) (2) (2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_casing_textprop_default_nonsticky_dynamic_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq-local text-property-default-nonsticky '((syntax-table . t)))
  (let ((text-property-default-nonsticky '((syntax-table . nil))))
    (insert "ß")
    (put-text-property 1 2 'syntax-table (string-to-syntax "w"))
    (upcase-region 1 2)
    (list (buffer-string)
          (get-text-property 1 'syntax-table)
          (get-text-property 2 'syntax-table))))
"#;
    let expect = expect_test::expect![[r#""OK (#(\"SS\" 0 2 (syntax-table (2))) (2) (2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_casing_textprop_default_nonsticky_default_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (insert "ß")
  (put-text-property 1 2 'syntax-table (string-to-syntax "w"))
  (upcase-region 1 2)
  (list (buffer-string)
        (get-text-property 1 'syntax-table)
        (get-text-property 2 'syntax-table)))
"#;
    let expect = expect_test::expect![[r#""OK (#(\"SS\" 1 2 (syntax-table (2))) nil (2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_casing_char_property_alias_alist_setq_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq-local char-property-alias-alist '((front-sticky my-front)))
  (insert "ßx")
  (put-text-property 1 2 'my-front t)
  (put-text-property 1 2 'my-prop 'val)
  (put-text-property 2 3 'my-prop 'right)
  (upcase-region 1 2)
  (list (buffer-string)
        (get-text-property 1 'my-prop)
        (get-text-property 2 'my-prop)
        (get-text-property 3 'my-prop)))
"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop my-front) my-prop val my-front t) 1 2 (my-prop val my-front t) 2 3 (my-prop right)) val val right)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_casing_char_property_alias_alist_dynamic_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq-local char-property-alias-alist nil)
  (let ((char-property-alias-alist '((front-sticky my-front))))
    (insert "ßx")
    (put-text-property 1 2 'my-front t)
    (put-text-property 1 2 'my-prop 'val)
    (put-text-property 2 3 'my-prop 'right)
    (upcase-region 1 2)
    (list (buffer-string)
          (get-text-property 1 'my-prop)
          (get-text-property 2 'my-prop)
          (get-text-property 3 'my-prop))))
"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop my-front) my-prop val my-front t) 1 2 (my-prop val my-front t) 2 3 (my-prop right)) val val right)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_casing_char_property_alias_alist_default_control() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (insert "ßx")
  (put-text-property 1 2 'my-front t)
  (put-text-property 1 2 'my-prop 'val)
  (put-text-property 2 3 'my-prop 'right)
  (upcase-region 1 2)
  (list (buffer-string)
        (get-text-property 1 'my-prop)
        (get-text-property 2 'my-prop)
        (get-text-property 3 'my-prop)))
"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"SSx\" 1 2 (my-prop val my-front t) 2 3 (my-prop right)) nil val right)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_casing_default_text_properties_setq_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq-local default-text-properties '(front-sticky t))
  (insert "ßx")
  (put-text-property 1 2 'my-prop 'val)
  (put-text-property 2 3 'my-prop 'right)
  (upcase-region 1 2)
  (list (buffer-string)
        (get-text-property 1 'my-prop)
        (get-text-property 2 'my-prop)
        (get-text-property 3 'my-prop)))
"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop) my-prop val) 1 2 (my-prop val) 2 3 (my-prop right)) val val right)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_casing_default_text_properties_dynamic_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq-local default-text-properties nil)
  (let ((default-text-properties '(front-sticky t)))
    (insert "ßx")
    (put-text-property 1 2 'my-prop 'val)
    (put-text-property 2 3 'my-prop 'right)
    (upcase-region 1 2)
    (list (buffer-string)
          (get-text-property 1 'my-prop)
          (get-text-property 2 'my-prop)
          (get-text-property 3 'my-prop))))
"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop) my-prop val) 1 2 (my-prop val) 2 3 (my-prop right)) val val right)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// defvaralias reads the base's boundness and the alias's value the way GNU
// does (`Fboundp` / `find_symbol_value`, src/eval.c:679-690): a base with a
// buffer-local binding but a void default is BOUND, so the migration into
// its default must not run.

#[test]
fn oracle_prop_defvaralias_base_buffer_local_void_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (setq gdm-alias-new 'v)
  (setq-local gdm-alias-base 'local)
  (defvaralias 'gdm-alias-new 'gdm-alias-base)
  (list (boundp 'gdm-alias-base)
        (condition-case e (default-value 'gdm-alias-base) (error (car e)))
        (symbol-value 'gdm-alias-base)
        (symbol-value 'gdm-alias-new)
        (condition-case e (default-value 'gdm-alias-new) (error (car e)))))
"#;
    let expect = expect_test::expect![[r#""OK (t void-variable local local void-variable)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_defvaralias_base_let_over_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(with-temp-buffer
  (defvar gdm-alias2-base)
  (setq gdm-alias2-new 'v)
  (setq-local gdm-alias2-base 'local)
  (let ((gdm-alias2-base 'letval))
    (defvaralias 'gdm-alias2-new 'gdm-alias2-base)
    (list (boundp 'gdm-alias2-base)
          (condition-case e (default-value 'gdm-alias2-base) (error (car e)))
          (symbol-value 'gdm-alias2-base)
          (symbol-value 'gdm-alias2-new))))
"#;
    let expect = expect_test::expect![[r#""OK (t void-variable letval letval)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
