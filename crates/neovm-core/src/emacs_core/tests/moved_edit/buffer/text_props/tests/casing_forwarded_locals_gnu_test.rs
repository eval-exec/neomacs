//! In-process twin of `neovm-oracle-tests` `textprop_control_vars.rs`:
//! the casing / expansion inheritance path must read
//! `text-property-default-nonsticky`, `char-property-alias-alist` and
//! `default-text-properties` the way GNU's C code reads them — the current
//! buffer's binding plus the live dynamic binding, never the default.
//!
//! GNU source: DEFVAR_LISP src/textprop.c:2417 + Fmake_variable_buffer_local
//! :2430 (text-property-default-nonsticky), src/textprop.c:2403
//! (char-property-alias-alist), src/textprop.c:2397 (default-text-properties);
//! C reads at src/intervals.c:878, 1733, 1741-1742. The expects are the GNU
//! 31.1 `--batch -Q` answers (the same forms and literals are recorded in the
//! oracle module, where `NEOVM_ORACLE_MODE=refresh` writes them from GNU).

use crate::test_utils::{oracle_expect_transcript, runtime_startup_eval_one};

#[test]
fn casing_textprop_default_nonsticky_reads_the_current_buffer() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK (#(\"SS\" 0 2 (syntax-table (2))) (2) (2))""#)
    );
}

#[test]
fn casing_textprop_default_nonsticky_sees_a_dynamic_let_over_a_local() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(r#""OK (#(\"SS\" 0 2 (syntax-table (2))) (2) (2))""#)
    );
}

#[test]
fn casing_char_property_alias_alist_reads_the_current_buffer() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop my-front) my-prop val my-front t) 1 2 (my-prop val my-front t) 2 3 (my-prop right)) val val right)""#
        )
    );
}

#[test]
fn casing_char_property_alias_alist_sees_a_dynamic_let_over_a_local() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop my-front) my-prop val my-front t) 1 2 (my-prop val my-front t) 2 3 (my-prop right)) val val right)""#
        )
    );
}

#[test]
fn casing_default_text_properties_reads_the_current_buffer() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop) my-prop val) 1 2 (my-prop val) 2 3 (my-prop right)) val val right)""#
        )
    );
}

#[test]
fn casing_default_text_properties_sees_a_dynamic_let_over_a_local() {
    crate::test_utils::init_test_tracing();
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
    assert_eq!(
        runtime_startup_eval_one(form),
        oracle_expect_transcript(
            r#""OK (#(\"SSx\" 0 1 (front-sticky (my-prop) my-prop val) 1 2 (my-prop val) 2 3 (my-prop right)) val val right)""#
        )
    );
}
