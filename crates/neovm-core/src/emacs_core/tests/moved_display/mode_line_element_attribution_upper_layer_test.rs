//! mode_line_element_attribution_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::xdisp::*;
use neovm_display::test_support::authority::{
    mode_line_element_attribution_context_for_test as context,
    mode_line_element_attribution_strict_for_test as strict,
};
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::save_scratch_gc_roots;
use neovm_value::value::Value;

#[test]
fn mode_line_element_attribution_literal_symbol_numeric_and_percent_are_strict() {
    let mut eval = context(crate::test_utils::new_context);
    for (form, expected) in [
        (r#""LITERAL""#, r#"#("LITERAL" 0 7 (mode-line-elt-no 1))"#),
        (r#"'(8 "ab")"#, r#"#("ab      " 0 2 (mode-line-elt-no 3))"#),
        (r#"'(-3 "abcdef")"#, r#"#("abc" 0 3 (mode-line-elt-no 3))"#),
        (
            r#"'(7 (-3 "abcdef"))"#,
            r#"#("abc    " 0 3 (mode-line-elt-no 5))"#,
        ),
    ] {
        strict(&mut eval, &format!("(format-mode-line {form})"), expected);
    }
    eval.eval_str("(setq ep30-tail \"%l\")").unwrap();
    strict(
        &mut eval,
        "(format-mode-line 'ep30-tail)",
        r#"#("%l" 0 2 (mode-line-elt-no 2))"#,
    );
    // The literal percent is character 16 of the captured GNU percent case.
    // Use its complete attributed substring, not a new hand-derived ID.
    eval.eval_str(
        r#"(setq ep30-percent-reference
          (substring '#("[510|21|50%|22k|%]" 16 17 (mode-line-elt-no 1)) 16 17))"#,
    )
    .unwrap();
    eval.eval_str(r#"(setq ep30-actual (format-mode-line "%%"))"#)
        .unwrap();
    assert!(
        eval.eval_str("(equal-including-properties ep30-actual ep30-percent-reference)")
            .unwrap()
            .is_t()
    );
}

#[test]
fn mode_line_element_attribution_global_recursion_reads_mutated_string_tail() {
    let mut eval = context(crate::test_utils::new_context);
    eval.eval_str(
        r#"(progn (insert "x")
             (setq ep30-format-string (copy-sequence "A%Mzz")))"#,
    )
    .unwrap();
    // GNU mutation-r1: %M changes the unconsumed literal zz into a %*
    // selector. The selector must read the live source and shared ordinal.
    strict(
        &mut eval,
        r#"(let ((global-mode-string
                   '(:eval (progn (aset ep30-format-string 3 ?%)
                                  (aset ep30-format-string 4 ?*) "G"))))
             (format-mode-line ep30-format-string))"#,
        r#"#("AG*" 0 1 (mode-line-elt-no 1) 1 2 (mode-line-elt-no 3) 2 3 (mode-line-elt-no 3))"#,
    );
    assert!(
        eval.eval_str(r#"(equal ep30-format-string "A%M%*")"#)
            .unwrap()
            .is_t()
    );
}

#[test]
fn mode_line_element_attribution_nul_literal_and_c_string_stores_match_gnu() {
    let mut eval = context(crate::test_utils::new_context);
    eval.eval_str(r#"(setq ep30-tail (concat "A" (string 0) "Z"))"#)
        .unwrap();
    // All twenty exact outputs are from GNU nul-r1. Runtime NUL characters
    // in Rust expectations preserve the oracle values, including attribution.
    for (form, string, noprop) in [
        (
            "'ep30-tail",
            "#(\"A\0Z\" 0 3 (mode-line-elt-no 2))",
            r#""A""#,
        ),
        (
            "'(-1 . ep30-tail)",
            r#"#("A" 0 1 (mode-line-elt-no 3))"#,
            r#""A""#,
        ),
        (
            "'(-2 . ep30-tail)",
            "#(\"A\0\" 0 2 (mode-line-elt-no 3))",
            r#""A""#,
        ),
        (
            "'(-3 . ep30-tail)",
            "#(\"A\0Z\" 0 3 (mode-line-elt-no 3))",
            r#""A""#,
        ),
        (
            "'(5 . ep30-tail)",
            "#(\"A\0Z  \" 0 3 (mode-line-elt-no 3))",
            r#""A    ""#,
        ),
        ("ep30-tail", r#"#("A" 0 1 (mode-line-elt-no 1))"#, r#""A""#),
        (
            "(list -1 ep30-tail)",
            r#"#("A" 0 1 (mode-line-elt-no 3))"#,
            r#""A""#,
        ),
        (
            "(list -2 ep30-tail)",
            r#"#("A" 0 1 (mode-line-elt-no 3))"#,
            r#""A""#,
        ),
        (
            "(list -3 ep30-tail)",
            r#"#("A" 0 1 (mode-line-elt-no 3))"#,
            r#""A""#,
        ),
        (
            "(list 5 ep30-tail)",
            r#"#("A    " 0 1 (mode-line-elt-no 3))"#,
            r#""A    ""#,
        ),
    ] {
        strict(&mut eval, &format!("(format-mode-line {form})"), string);
        strict(&mut eval, &format!("(format-mode-line {form} 0)"), noprop);
    }
}

#[test]
fn mode_line_element_attribution_compact_string_contract_and_display_are_distinct() {
    let mut eval = context(crate::test_utils::new_context);
    for compact in ["t", "'long"] {
        strict(
            &mut eval,
            &format!(
                r#"(let ((mode-line-compact {compact})) (format-mode-line '("AA   BB" "  CC   " (:propertize "DD  EE" ep30-tag compact))))"#
            ),
            r#"#("AA   BB  CC   DD  EE" 0 7 (mode-line-elt-no 2) 7 14 (mode-line-elt-no 3) 14 20 (ep30-tag compact mode-line-elt-no 5))"#,
        );
    }
    let format = eval
        .eval_str(r#"'("A" (:eval "B") (:propertize "C" ep30-tag display))"#)
        .unwrap();
    let output =
        format_mode_line_for_display_with_sources(&mut eval, format, Value::NIL, Value::NIL, 80);
    eval.set_variable("ep30-display-output", output.value());
    assert!(
        eval.eval_str(
            "(and (null (get-text-property 0 'mode-line-elt-no ep30-display-output))
              (null (get-text-property 1 'mode-line-elt-no ep30-display-output))
              (null (get-text-property 2 'mode-line-elt-no ep30-display-output))
              (eq (get-text-property 2 'ep30-tag ep30-display-output) 'display))"
        )
        .unwrap()
        .is_t()
    );
}
