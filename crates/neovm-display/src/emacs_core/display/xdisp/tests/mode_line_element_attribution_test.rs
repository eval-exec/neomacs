//! Strict STRING expectations from GNU 31.1 CONT30 gnu-oracle-r3 (20 cases)
//! and gnu-supplement-r1 (7 cases). No property key is projected away.

use super::*;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::emacs_core::eval::save_scratch_gc_roots;

fn context() -> Context {
    crate::test_utils::mode_line_element_attribution_test_upper_layer_helpers::context(|| {
        neovm_eval::emacs_core::eval::Context::new(crate::test_utils::unit_runtime())
    })
}

use crate::test_utils::mode_line_element_attribution_test_upper_layer_helpers::strict;

/// GNU's interval plist ORDER, recorded from the pinned 31.1 pty oracle
/// (`object-intervals` / `text-properties-at`): source plist order with
/// `mode-line-elt-no` APPENDED when it was absent, or replaced IN PLACE at
/// its existing position. `equal-including-properties` parses property
/// plists as unordered sets (GNU fns.c internal_equal, Neo matches), so this
/// order is only observable through the ordered interval table; the oracle
/// expectations below are literal GNU output, not Neo-derived.
fn attribution_value_repr(value: Value) -> String {
    if value.is_nil() {
        return "nil".to_string();
    }
    if let Some(name) = value.as_bare_symbol_name() {
        return name.to_string();
    }
    if let Some(number) = value.as_fixnum() {
        return number.to_string();
    }
    if let Some(text) = value.as_utf8_str() {
        return format!("{text:?}");
    }
    if value.is_cons() {
        let mut rendered = Vec::new();
        let mut cursor = value;
        while cursor.is_cons() {
            rendered.push(attribution_value_repr(cursor.cons_car()));
            cursor = cursor.cons_cdr();
        }
        if !cursor.is_nil() {
            rendered.push(format!(". {}", attribution_value_repr(cursor)));
        }
        return format!("({})", rendered.join(" "));
    }
    panic!("unexpected property value in the ordered oracle: {value:?}");
}

fn attribution_ordered_plists_of_value(value: Value, positions: &[usize]) -> Vec<String> {
    positions
        .iter()
        .map(|position| {
            let pairs = with_mode_line_string_properties(value, |table| {
                table.get_properties_ordered_at_char_pos(CharPos0::new(*position))
            })
            .expect("formatted mode line carries a property table");
            let rendered: Vec<String> = pairs
                .iter()
                .flat_map(|(name, value)| {
                    [
                        attribution_value_repr(*name),
                        attribution_value_repr(*value),
                    ]
                })
                .collect();
            format!("({})", rendered.join(" "))
        })
        .collect()
}

fn attribution_ordered_plists(eval: &mut Context, form: &str, positions: &[usize]) -> Vec<String> {
    let value = eval
        .eval_str(&format!("(format-mode-line {form})"))
        .expect("format-mode-line returns");
    attribution_ordered_plists_of_value(value, positions)
}

/// GNU 31.1 pty oracle, recorded 2026-11-07 (probe files
/// plist-order-result.txt / plist-order-result2.txt):
///   A #("AB" 0 2 (p 1 q 2 r 3))            -> (p 1 q 2 r 3 mode-line-elt-no 2)
///   B #("%l" 0 2 (p 1 q 2 r 3))            -> (p 1 q 2 r 3 mode-line-elt-no 2)
///   H (:propertize "AB" p 1 q 2)           -> (p 1 q 2 mode-line-elt-no 3)
///   G nested propertize with inner elt-no  -> (mode-line-elt-no 4 ep30-tag inner)
///   I percent with existing elt-no 700 p 1 -> positions 0/1/2:
///       (mode-line-elt-no 2) / (mode-line-elt-no 4 p 1) / (mode-line-elt-no 5)
///   J #("XY" 0 2 (mode-line-elt-no 40 p 1 q 2))
///                                          -> (mode-line-elt-no 2 p 1 q 2)
///   K ("A" (:eval "EV") "Z")               -> output offsets 0/1/3:
///       (mode-line-elt-no 2) / (mode-line-elt-no 4) / (mode-line-elt-no 5)
///   L ((:eval "EV"))                       -> (mode-line-elt-no 3)
#[test]
fn mode_line_element_attribution_interval_plist_order_matches_gnu() {
    let mut eval = context();
    let mut mismatches: Vec<String> = Vec::new();
    for (form, positions, expected) in [
        (
            r#"'(#("AB" 0 2 (p 1 q 2 r 3)))"#,
            &[0usize][..],
            &["(p 1 q 2 r 3 mode-line-elt-no 2)"][..],
        ),
        (
            r#"'(#("%l" 0 2 (p 1 q 2 r 3)))"#,
            &[0usize][..],
            &["(p 1 q 2 r 3 mode-line-elt-no 2)"][..],
        ),
        (
            r#"'((:propertize "AB" p 1 q 2))"#,
            &[0usize][..],
            &["(p 1 q 2 mode-line-elt-no 3)"][..],
        ),
        (
            r#"'((:propertize (:propertize "P" mode-line-elt-no 900 ep30-tag inner) mode-line-elt-no 800 ep30-tag outer))"#,
            &[0usize][..],
            &["(mode-line-elt-no 4 ep30-tag inner)"][..],
        ),
        (
            r#"'("A" (:propertize "%l" mode-line-elt-no 700 p 1) "C")"#,
            &[0usize, 1, 2][..],
            &[
                "(mode-line-elt-no 2)",
                "(mode-line-elt-no 4 p 1)",
                "(mode-line-elt-no 5)",
            ][..],
        ),
        (
            r#"'(#("XY" 0 2 (mode-line-elt-no 40 p 1 q 2)))"#,
            &[0usize][..],
            &["(mode-line-elt-no 2 p 1 q 2)"][..],
        ),
        (
            r#"'("A" (:eval "EV") "Z")"#,
            &[0usize, 1, 3][..],
            &[
                "(mode-line-elt-no 2)",
                "(mode-line-elt-no 4)",
                "(mode-line-elt-no 5)",
            ][..],
        ),
        (
            r#"'((:eval "EV"))"#,
            &[0usize][..],
            &["(mode-line-elt-no 3)"][..],
        ),
    ] {
        let actual = attribution_ordered_plists(&mut eval, form, positions);
        if actual != expected {
            mismatches.push(format!("{form}\n  GNU: {expected:?}\n  Neo: {actual:?}"));
        }
    }
    assert!(
        mismatches.is_empty(),
        "interval plist order differs from GNU:\n{}",
        mismatches.join("\n")
    );
}

/// GNU 31.1 pty oracle, recorded 2026-11-07 with the FACE argument
/// (`object-intervals` on a real `-nw` frame; shapes F1-F14/G1-G8).
///
/// `format-mode-line`'s FACE reaches the interval plist through
/// `store_mode_line_string` (src/xdisp.c:28924-28934, 28945-28971): it takes
/// the source plist, `plist_put`s `mode-line-elt-no` (append, or replace in
/// place at its existing position), then `plist_put`s `face` with
/// `list2 (source_face, default_face)` when a face was already there.  The
/// observable order is therefore source order, then `mode-line-elt-no`,
/// then `face` LAST.  `t` maps to `mode-line-active` when the WINDOW
/// argument is the selected window and to `mode-line-inactive` otherwise
/// (src/xdisp.c:29059-29062, compared before `Fselect_window`),
/// `nil`/`default` add no face, and an integer suppresses text properties
/// entirely (29024).
#[test]
fn mode_line_face_argument_interval_plist_order_matches_gnu() {
    use crate::window::{SplitDirection, SplitPlacement};

    let mut eval = context();
    let frame_id = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame")
        .id;
    let selected = eval
        .frame_manager()
        .selected_frame()
        .expect("selected frame")
        .selected_window;
    let buffer = eval.edit.buffers.current_buffer_id().expect("buffer");
    let other = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            selected,
            SplitDirection::Vertical,
            buffer,
            Some(3),
            SplitPlacement::AfterTarget,
        )
        .expect("second window");
    eval.frame_manager_mut()
        .apply_staged_split_sizes(
            frame_id,
            other,
            Some(3),
            Value::NIL,
            SplitDirection::Vertical,
        )
        .expect("applied split sizes");
    assert_eq!(
        eval.frame_manager()
            .selected_frame()
            .expect("selected frame")
            .selected_window,
        selected,
        "GNU's split-window leaves the selected window selected"
    );
    eval.set_variable("ep30-other-window", Value::make_window(other.0));

    let mut mismatches: Vec<String> = Vec::new();
    for (form, face, window, positions, expected) in [
        // F1: percent field, source plist then elt-no then FACE.
        (
            r#"'(#("%l" 0 2 (p 1 q 2)))"#,
            "'bold",
            None,
            &[0usize][..],
            &["(p 1 q 2 mode-line-elt-no 2 face bold)"][..],
        ),
        // F2: literal propertized string.
        (
            r#"'(#("AB" 0 2 (p 1 q 2)))"#,
            "'bold",
            None,
            &[0usize][..],
            &["(p 1 q 2 mode-line-elt-no 2 face bold)"][..],
        ),
        // F3: plain string, FACE appended after elt-no.
        (
            r#"'("AB")"#,
            "'bold",
            None,
            &[0usize][..],
            &["(mode-line-elt-no 2 face bold)"][..],
        ),
        // F4: :propertize element.
        (
            r#"'((:propertize "AB" p 1 q 2))"#,
            "'bold",
            None,
            &[0usize][..],
            &["(p 1 q 2 mode-line-elt-no 3 face bold)"][..],
        ),
        // F5: source face stays at its source position; the default face
        // is appended to its value list, not to the key position.
        (
            r#"'(#("AB" 0 2 (face italic p 1)))"#,
            "'bold",
            None,
            &[0usize][..],
            &["(face (italic bold) p 1 mode-line-elt-no 2)"][..],
        ),
        // F7: same merge through :propertize.
        (
            r#"'((:propertize "AB" face italic p 1))"#,
            "'bold",
            None,
            &[0usize][..],
            &["(face (italic bold) p 1 mode-line-elt-no 3)"][..],
        ),
        // F10: an existing mode-line-elt-no is replaced in place.
        (
            r#"'(#("%l" 0 2 (mode-line-elt-no 700 p 1)))"#,
            "'bold",
            None,
            &[0usize][..],
            &["(mode-line-elt-no 2 p 1 face bold)"][..],
        ),
        // F11: FACE t on the selected window.
        (
            r#"'(#("AB" 0 2 (p 1)))"#,
            "t",
            None,
            &[0usize][..],
            &["(p 1 mode-line-elt-no 2 face mode-line-active)"][..],
        ),
        // F13: FACE nil adds nothing; the min-width padding is bare.
        (
            r#"'((6 #("AB" 0 2 (p 1))))"#,
            "nil",
            None,
            &[0usize, 2][..],
            &["(p 1 mode-line-elt-no 4)", "()"][..],
        ),
        // G1: `default` means no face property at all.
        (
            r#"'(#("AB" 0 2 (p 1)))"#,
            "'default",
            None,
            &[0usize][..],
            &["(p 1 mode-line-elt-no 2)"][..],
        ),
        // G2: FACE t, selected window by default.
        (
            r#"'("AB")"#,
            "t",
            None,
            &[0usize][..],
            &["(mode-line-elt-no 2 face mode-line-active)"][..],
        ),
        // G3: FACE t for a WINDOW that is not selected.
        (
            r#"'("AB")"#,
            "t",
            Some("ep30-other-window"),
            &[0usize][..],
            &["(mode-line-elt-no 2 face mode-line-inactive)"][..],
        ),
        // G4: the inactive face named explicitly.
        (
            r#"'("AB")"#,
            "'mode-line-inactive",
            None,
            &[0usize][..],
            &["(mode-line-elt-no 2 face mode-line-inactive)"][..],
        ),
        // G5: any face designator is installed verbatim.
        (
            r#"'("AB")"#,
            "'header-line-active",
            None,
            &[0usize][..],
            &["(mode-line-elt-no 2 face header-line-active)"][..],
        ),
        // G6: a list of faces is installed as the face value.
        (
            r#"'("AB")"#,
            "'(bold italic)",
            None,
            &[0usize][..],
            &["(mode-line-elt-no 2 face (bold italic))"][..],
        ),
    ] {
        let window_arg = window
            .map(|window| format!(" {window}"))
            .unwrap_or_default();
        let expression = format!("(format-mode-line {form} {face}{window_arg})");
        let value = eval
            .eval_str(&expression)
            .expect("format-mode-line returns");
        let actual = attribution_ordered_plists_of_value(value, positions);
        if actual != expected {
            mismatches.push(format!(
                "{expression}\n  GNU: {expected:?}\n  Neo: {actual:?}"
            ));
        }
    }

    // G7: an integer FACE means the value string has no text properties.
    let no_props = eval
        .eval_str(r#"(format-mode-line '(#("AB" 0 2 (p 1))) 1)"#)
        .expect("format-mode-line returns");
    if with_mode_line_string_properties(no_props, |_| ()).is_some() {
        mismatches.push(
            "G7 integer FACE: GNU returns a property-free string, Neo attached properties".into(),
        );
    }

    assert!(
        mismatches.is_empty(),
        "FACE argument plist order differs from GNU:\n{}",
        mismatches.join("\n")
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
        let gc = eval.gc_count_for_test();
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
            assert_eq!(
                eval.gc_count_for_test(),
                gc + 2,
                "both callbacks must collect"
            );
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
        let specpdl = eval.specpdl().len();
        let value = eval.eval_str(&format!(
            r#"(catch 'ep30-escape (format-mode-line '("A" (:eval (ep30-callback '{kind})) "Z")))"#)).unwrap();
        assert_eq!(value.as_bare_symbol_name(), Some("ep30-thrown"));
        assert_eq!(eval.specpdl().len(), specpdl);
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
