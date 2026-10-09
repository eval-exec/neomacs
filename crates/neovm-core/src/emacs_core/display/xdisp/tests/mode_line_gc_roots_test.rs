//! Accumulated mode-line intervals must remain roots across later `:eval`.

use super::*;
use crate::emacs_core::Context;

#[derive(Clone, Copy)]
enum PropertySource {
    PropertizeElement,
    CopiedString,
    NestedEval,
    FreshEvalString,
    DetachedFormatElements,
}

#[derive(Clone, Copy)]
enum FormatEntry {
    LispString,
    Display,
}

fn assert_accumulated_property_survives(
    generational: bool,
    source: PropertySource,
    entry: FormatEntry,
) {
    // Nextest runs each case in a fresh process. The constructor reads this
    // knob once; the explicit collection inside :eval is the trigger here.
    unsafe {
        std::env::set_var(
            "NEOVM_GC_GENERATIONAL",
            if generational { "1" } else { "0" },
        );
    }
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    eval.set_variable("noninteractive", Value::NIL);
    eval.gc_collect_exact();

    // Keep the input in the actual obarray, independently of the formatter's
    // Rust locals. The copied-string case also keeps the ORIGINAL plist alive;
    // only the freshly copied destination spine needs an additional root.
    let setup = match source {
        PropertySource::PropertizeElement => {
            "(setq u34-mode-line-gc-format
                    '((:propertize \"A\" u34-mode-line-gc-property 73)
                      (:eval (progn (garbage-collect) \"B\"))))"
        }
        PropertySource::CopiedString => {
            "(setq u34-mode-line-gc-format
                    (list (propertize \"A\" 'u34-mode-line-gc-property 73)
                          '(:eval (progn (garbage-collect) \"B\"))))"
        }
        PropertySource::NestedEval => {
            "(setq u34-mode-line-gc-format
                    '((:propertize \"A\" u34-mode-line-gc-property 73)
                      (1 (:propertize
                            ((:eval (progn (garbage-collect) \"B\")))
                            face bold))))"
        }
        PropertySource::FreshEvalString => {
            // Add no source span after collection: a missing source root must
            // reach the ownership probe before any source-value comparison.
            "(setq u34-mode-line-gc-format
                    '((:eval (propertize (concat \"A\")
                                         'u34-mode-line-gc-property 73))
                      (:eval (progn (garbage-collect) \"\"))))"
        }
        PropertySource::DetachedFormatElements => {
            "(setq u34-mode-line-gc-format
                    (list '(:propertize \"A\" u34-mode-line-gc-property 73)
                          '(:eval (progn
                                    (setcdr (cdr u34-mode-line-gc-format) nil)
                                    (garbage-collect)
                                    \"\"))
                          (concat \"B\")))"
        }
    };
    eval.eval_str(setup)
        .expect("install rooted mode-line format");
    let format = eval
        .obarray
        .symbol_value_copied("u34-mode-line-gc-format")
        .expect("installed mode-line format");
    let before = eval.gc_count;
    let (rendered, sources) = match entry {
        FormatEntry::LispString => {
            // The Lisp entry runs the same evaluator-backed recursive walk.
            let value = eval
                .eval_str("(format-mode-line u34-mode-line-gc-format)")
                .expect("format-mode-line with a collecting :eval");
            (value, Vec::new())
        }
        FormatEntry::Display => {
            let output = format_mode_line_for_display_with_sources(
                &mut eval,
                format,
                Value::NIL,
                Value::NIL,
                80,
            );
            let sources = output
                .source_spans()
                .iter()
                .map(|span| span.source())
                .collect::<Vec<_>>();
            (output.value(), sources)
        }
    };
    assert_eq!(eval.gc_count, before + 1, "the later :eval must collect");
    let expected = if matches!(
        source,
        PropertySource::DetachedFormatElements | PropertySource::FreshEvalString
    ) {
        "A"
    } else {
        "AB"
    };
    assert_eq!(rendered.as_utf8_str(), Some(expected));
    let string = rendered.as_lisp_string().expect("rendered string");
    let runs = string
        .intervals()
        .object_interval_plist_runs_for_char_len(crate::buffer::CharLen::new(expected.len()));
    let first = runs.first().expect("retained first interval");
    assert_eq!(first.start(), crate::buffer::CharPos0::ZERO);
    let plist = first.plist();
    assert!(plist.is_cons(), "the property spine must be retained");

    // A freed cons has car == Value::DEAD (bits 4). Compare raw bits before
    // any property-name/string access so the regression reports its cause
    // instead of dereferencing that null-string poison, as chrome memo did.
    let name = plist.cons_car();
    assert_eq!(
        name.bits(),
        Value::symbol("u34-mode-line-gc-property").bits(),
        "the accumulated property's cons was swept during the later :eval"
    );
    let rest = plist.cons_cdr();
    assert!(rest.is_cons(), "the property's value cell must be retained");
    assert_eq!(rest.cons_car().bits(), Value::fixnum(73).bits());
    // A previous :eval can contribute a fresh source string that is retained
    // only in the display sidecar after its temporary evaluator root ends.
    // Check the arena's allocation bitmap without reading the string payload.
    // The final rendered string is the only string allocation after :eval's
    // collection; reject its address too, in case it reused a swept source.
    for source_value in sources {
        assert_ne!(
            source_value.bits(),
            rendered.bits(),
            "a swept mode-line source was reused for the rendered string"
        );
        assert!(
            eval.tagged_heap.owns_heap_value_for_test(source_value),
            "the accumulated source string was swept during the later :eval"
        );
    }
}

#[test]
fn gc_mode_line_propertize_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::PropertizeElement,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_propertize_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::PropertizeElement,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_copied_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::CopiedString,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_copied_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::CopiedString,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_display_propertize_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::PropertizeElement,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_propertize_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::PropertizeElement,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_copied_spine_survives_later_eval_gen0() {
    assert_accumulated_property_survives(false, PropertySource::CopiedString, FormatEntry::Display);
}

#[test]
fn gc_mode_line_display_copied_spine_survives_later_eval_gen1() {
    assert_accumulated_property_survives(true, PropertySource::CopiedString, FormatEntry::Display);
}

#[test]
fn gc_mode_line_parent_spine_survives_nested_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::NestedEval,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_parent_spine_survives_nested_eval_gen1() {
    assert_accumulated_property_survives(true, PropertySource::NestedEval, FormatEntry::LispString);
}

#[test]
fn gc_mode_line_display_parent_spine_survives_nested_eval_gen0() {
    assert_accumulated_property_survives(false, PropertySource::NestedEval, FormatEntry::Display);
}

#[test]
fn gc_mode_line_display_parent_spine_survives_nested_eval_gen1() {
    assert_accumulated_property_survives(true, PropertySource::NestedEval, FormatEntry::Display);
}

#[test]
fn gc_mode_line_display_fresh_eval_source_survives_later_eval_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::FreshEvalString,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_fresh_eval_source_survives_later_eval_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::FreshEvalString,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_detached_tail_is_not_rendered_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::DetachedFormatElements,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_display_detached_tail_is_not_rendered_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::DetachedFormatElements,
        FormatEntry::Display,
    );
}

#[test]
fn gc_mode_line_detached_tail_is_not_rendered_gen0() {
    assert_accumulated_property_survives(
        false,
        PropertySource::DetachedFormatElements,
        FormatEntry::LispString,
    );
}

#[test]
fn gc_mode_line_detached_tail_is_not_rendered_gen1() {
    assert_accumulated_property_survives(
        true,
        PropertySource::DetachedFormatElements,
        FormatEntry::LispString,
    );
}
