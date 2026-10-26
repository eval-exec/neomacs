//! Strict STRING expectations from GNU 31.1 CONT30 gnu-oracle-r3 (20 cases)
//! and gnu-supplement-r1 (7 cases). No property key is projected away.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::eval::save_scratch_gc_roots;

fn context() -> Context {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    eval.set_variable("noninteractive", Value::NIL);
    let buffer = eval.buffers.current_buffer_id().expect("current buffer");
    let frame = eval
        .frames
        .create_frame("element-attribution", 800, 600, buffer);
    assert!(eval.frames.select_frame(frame));
    eval.eval_str(
        r#"(defvar ep30-events nil)
           (defvar ep30-tail "OLD")
           (defvar ep30-live-list nil)
           (defvar ep30-effects 0)
           (defvar ep30-inner nil)
           (defvar ep30-inner-format '("i" "j"))
           (defvar ep30-format-string nil)
           (defvar ep30-collect nil)
           (fset 'ep30-callback '(lambda (kind)
             (setq ep30-events (cons (list 'enter kind) ep30-events))
             (prog1
                 (cond
                  ((eq kind 'nested) "EV")
                  ((eq kind 'mutate-value) (setq ep30-tail "NEW")
                   (if ep30-collect (garbage-collect)) "M")
                  ((eq kind 'mutate-spine)
                   (setcar (cdr (cdr ep30-live-list)) "NEW")
                   (if ep30-collect (garbage-collect)) "M")
                  ((eq kind 'reenter)
                   (setq ep30-inner (format-mode-line ep30-inner-format)) "E")
                  ((eq kind 'reenter-noprop)
                   (setq ep30-inner (format-mode-line '("i" "j") 0)) "E")
                  ((eq kind 'signal) (signal 'error '("EP30 expected signal")))
                  ((eq kind 'throw) (throw 'ep30-escape 'ep30-thrown))
                  ((eq kind 'reenter-throw)
                   (format-mode-line '("i" (:eval (ep30-callback 'throw)) "j")))
                  ((eq kind 'reenter-caught-throw)
                   (setq ep30-events
                         (cons
                          (list 'inner-caught
                                (catch 'ep30-inner
                                  (format-mode-line
                                   '("i" "j" "k" (:eval (throw 'ep30-inner 'escaped)) "l"))))
                          ep30-events))
                   "E")
                  ((eq kind 'count) (setq ep30-effects (1+ ep30-effects)) "Q")
                  (t (error "Unknown attribution callback: %S" kind)))
               (setq ep30-events (cons (list 'return kind) ep30-events)))))"#,
    )
    .expect("GNU callback fixture");
    eval
}

fn strict(eval: &mut Context, expression: &str, gnu: &str) {
    let roots = save_scratch_gc_roots();
    eval.eval_str(&format!("(setq ep30-actual {expression})"))
        .expect("format-mode-line returns");
    assert_eq!(
        save_scratch_gc_roots(),
        roots,
        "temporary roots: {expression}"
    );
    let equal = eval
        .eval_str(&format!("(equal-including-properties ep30-actual '{gnu})"))
        .expect("strict property comparison");
    assert!(
        equal.is_t(),
        "GNU properties differ for {expression}: {:?}",
        eval.eval_str("(prin1-to-string ep30-actual)")
            .map(|value| value.as_utf8_str().map(str::to_owned))
    );
}

#[test]
fn mode_line_element_attribution_literal_symbol_numeric_and_percent_are_strict() {
    let mut eval = context();
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
fn mode_line_element_attribution_nested_properties_and_global_recursion_match_gnu() {
    let mut eval = context();
    strict(
        &mut eval,
        r#"(format-mode-line '("A" (:propertize (:eval (ep30-callback 'nested)) face bold ep30-tag nested) "Z"))"#,
        r#"#("AEVZ" 0 1 (mode-line-elt-no 2) 1 3 (face bold ep30-tag nested mode-line-elt-no 5) 3 4 (mode-line-elt-no 6))"#,
    );
    strict(
        &mut eval,
        r#"(format-mode-line '("A" (:propertize (:propertize "P" mode-line-elt-no 900 ep30-tag inner) mode-line-elt-no 800 ep30-tag outer) "Z"))"#,
        r#"#("APZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 5 ep30-tag inner) 2 3 (mode-line-elt-no 6))"#,
    );
    strict(
        &mut eval,
        r#"(let ((global-mode-string '("G" (:eval (ep30-callback 'nested)) "H"))) (format-mode-line "[%M]"))"#,
        r#"#("[GEVH]" 0 1 (mode-line-elt-no 1) 1 2 (mode-line-elt-no 3) 2 4 (mode-line-elt-no 5) 4 5 (mode-line-elt-no 6) 5 6 (mode-line-elt-no 6))"#,
    );
}

#[test]
fn mode_line_element_attribution_live_mutations_survive_collection() {
    let mut eval = context();
    for collect in ["nil", "t"] {
        eval.eval_str(&format!("(setq ep30-collect {collect} ep30-tail \"OLD\")"))
            .unwrap();
        let gc = eval.gc_count;
        strict(
            &mut eval,
            r#"(format-mode-line '("A" (:eval (ep30-callback 'mutate-value)) ep30-tail "Z"))"#,
            r#"#("AMNEWZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 4) 2 5 (mode-line-elt-no 6) 5 6 (mode-line-elt-no 7))"#,
        );
        eval.eval_str(
            r#"(setq ep30-live-list (list "A" '(:eval (ep30-callback 'mutate-spine)) "OLD" "Z"))"#,
        )
        .unwrap();
        strict(
            &mut eval,
            "(format-mode-line ep30-live-list)",
            r#"#("AMNEWZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 4) 2 5 (mode-line-elt-no 5) 5 6 (mode-line-elt-no 6))"#,
        );
        if collect == "t" {
            assert_eq!(eval.gc_count, gc + 2, "both callbacks must collect");
        }
    }
}

#[test]
fn mode_line_element_attribution_reentry_shares_counter_and_noprop_resets_it() {
    let mut eval = context();
    strict(
        &mut eval,
        r#"(format-mode-line '("A" (:eval (ep30-callback 'reenter)) "Z"))"#,
        r#"#("AEZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 4) 2 3 (mode-line-elt-no 5))"#,
    );
    strict(
        &mut eval,
        "ep30-inner",
        r#"#("ij" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 3))"#,
    );
    eval.eval_str(r#"(setq ep30-inner-format '("i" "j" "k" "l"))"#)
        .unwrap();
    strict(
        &mut eval,
        r#"(format-mode-line '("A" (:eval (ep30-callback 'reenter)) "Z"))"#,
        r#"#("AEZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 6) 2 3 (mode-line-elt-no 7))"#,
    );
    strict(
        &mut eval,
        r#"(format-mode-line '("A" (:eval (ep30-callback 'reenter-noprop)) "Z"))"#,
        r#"#("AEZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 1) 2 3 (mode-line-elt-no 2))"#,
    );
    strict(&mut eval, "ep30-inner", r#""ij""#);
}

#[test]
fn mode_line_element_attribution_signal_throw_and_recovery_restore_scopes() {
    let mut eval = context();
    strict(
        &mut eval,
        r#"(format-mode-line '("A" (:eval (ep30-callback 'signal)) "Z"))"#,
        r#"#("AZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 5))"#,
    );
    for kind in ["throw", "reenter-throw"] {
        let roots = save_scratch_gc_roots();
        let specpdl = eval.specpdl.len();
        let value = eval.eval_str(&format!(
            r#"(catch 'ep30-escape (format-mode-line '("A" (:eval (ep30-callback '{kind})) "Z")))"#)).unwrap();
        assert_eq!(value.as_bare_symbol_name(), Some("ep30-thrown"));
        assert_eq!(eval.specpdl.len(), specpdl);
        assert_eq!(save_scratch_gc_roots(), roots);
        strict(
            &mut eval,
            r#"(format-mode-line '("R" "S"))"#,
            r#"#("RS" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 3))"#,
        );
    }
}

#[test]
fn mode_line_element_attribution_precision_stops_before_callbacks_like_gnu() {
    let mut eval = context();
    for kind in ["count", "throw"] {
        eval.eval_str("(setq ep30-effects 0 ep30-events nil)")
            .unwrap();
        strict(
            &mut eval,
            &format!(r#"(format-mode-line '("B" (-1 "A" (:eval (ep30-callback '{kind}))) "Z"))"#),
            r#"#("BAZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 5) 2 3 (mode-line-elt-no 6))"#,
        );
        assert!(
            eval.eval_str("(and (= ep30-effects 0) (null ep30-events))")
                .unwrap()
                .is_t()
        );
    }
}

#[test]
fn mode_line_element_attribution_direct_string_precision_matches_gnu_columns() {
    let mut eval = context();
    // GNU precision-r2: the direct string scanner bounds columns, then the
    // list walker accounts the admitted characters rather than their columns.
    for (form, expected) in [
        (r#"'(-1 "文a")"#, r#""""#),
        (r#"'(-2 "文a")"#, r#"#("文" 0 1 (mode-line-elt-no 3))"#),
        (r#"'(-3 "文a")"#, r#"#("文a" 0 2 (mode-line-elt-no 3))"#),
        (
            r#"'(-3 "文a" "X")"#,
            r#"#("文aX" 0 2 (mode-line-elt-no 3) 2 3 (mode-line-elt-no 4))"#,
        ),
    ] {
        strict(&mut eval, &format!("(format-mode-line {form})"), expected);
    }
}

#[test]
fn mode_line_element_attribution_symbol_precision_uses_gnu_character_cap() {
    let mut eval = context();
    eval.eval_str(r#"(setq ep30-tail "文a")"#).unwrap();
    for (form, expected) in [
        ("'(-1 . ep30-tail)", r#"#("文" 0 1 (mode-line-elt-no 3))"#),
        ("'(-2 . ep30-tail)", r#"#("文a" 0 2 (mode-line-elt-no 3))"#),
        ("'(-3 . ep30-tail)", r#"#("文a" 0 2 (mode-line-elt-no 3))"#),
    ] {
        strict(&mut eval, &format!("(format-mode-line {form})"), expected);
    }
}

#[test]
fn mode_line_element_attribution_caught_inner_throw_keeps_live_counter() {
    let mut eval = context();
    // GNU precision-r2 catches the nested formatter's nonlocal exit inside
    // the outer :eval, which resumes with the shared inner ordinal.
    strict(
        &mut eval,
        r#"(format-mode-line '("A" (:eval (ep30-callback 'reenter-caught-throw)) "Z"))"#,
        r#"#("AEZ" 0 1 (mode-line-elt-no 2) 1 2 (mode-line-elt-no 6) 2 3 (mode-line-elt-no 7))"#,
    );
    assert!(
        eval.eval_str(
            "(equal ep30-events '((return reenter-caught-throw)
               (inner-caught escaped) (enter reenter-caught-throw)))"
        )
        .unwrap()
        .is_t()
    );
}

#[test]
fn mode_line_element_attribution_global_recursion_reads_mutated_string_tail() {
    let mut eval = context();
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
fn mode_line_element_attribution_string_and_noprop_use_gnu_accounting_units() {
    let mut eval = context();
    eval.eval_str(r#"(setq ep30-tail "界a")"#).unwrap();
    // GNU accounting-r1: face=nil stores character extents and attribution;
    // face=0 stores column extents with no properties. Every expected output
    // below is copied from that oracle's full property-bearing result.
    for (form, string, noprop) in [
        (
            r#"'(-2 "界" "X")"#,
            r#"#("界X" 0 1 (mode-line-elt-no 3) 1 2 (mode-line-elt-no 4))"#,
            r#""界""#,
        ),
        (
            r#"'(4 "界")"#,
            r#"#("界   " 0 1 (mode-line-elt-no 3))"#,
            r#""界  ""#,
        ),
        (
            r#"'(-3 "界" "X")"#,
            r#"#("界X" 0 1 (mode-line-elt-no 3) 1 2 (mode-line-elt-no 4))"#,
            r#""界X""#,
        ),
        (
            r#""界a""#,
            r#"#("界a" 0 2 (mode-line-elt-no 1))"#,
            r#""界a""#,
        ),
        (
            "'(-1 . ep30-tail)",
            r#"#("界" 0 1 (mode-line-elt-no 3))"#,
            r#""""#,
        ),
        (
            "'(-2 . ep30-tail)",
            r#"#("界a" 0 2 (mode-line-elt-no 3))"#,
            r#""界""#,
        ),
        (
            "'(-3 . ep30-tail)",
            r#"#("界a" 0 2 (mode-line-elt-no 3))"#,
            r#""界a""#,
        ),
        (
            r#"'(4 (-2 "界" "X"))"#,
            r#"#("界X  " 0 1 (mode-line-elt-no 5) 1 2 (mode-line-elt-no 6))"#,
            r#""界  ""#,
        ),
        (
            r#"'(-2 (4 "界"))"#,
            r#"#("界 " 0 1 (mode-line-elt-no 5))"#,
            r#""界""#,
        ),
    ] {
        strict(&mut eval, &format!("(format-mode-line {form})"), string);
        strict(&mut eval, &format!("(format-mode-line {form} 0)"), noprop);
    }
}

#[test]
fn mode_line_element_attribution_nul_literal_and_c_string_stores_match_gnu() {
    let mut eval = context();
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
    let mut eval = context();
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
