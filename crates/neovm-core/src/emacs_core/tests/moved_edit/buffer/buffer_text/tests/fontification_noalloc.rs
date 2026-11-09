use super::*;

use neovm_edit::buffer::buffer_text::line_index::LineIndexRole;
use neovm_value::gc_trace::GcTrace;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::rc::Rc;

use crate::emacs_core::eval::Context;
use crate::emacs_core::textprop_entries;
use neovm_edit::buffer::text::edit::TextExtent;
use neovm_value::heap_types::byte_positions::EmacsBytePos;
use neovm_value::heap_types::byte_positions::EmacsByteRange;
use neovm_value::value::Value;
use neovm_value::value::positions::CharPos0;

use neovm_edit::buffer::buffer_text::*;

use neovm_edit::buffer::{BufferTextBackendKind, LispCharPos1};

use neovm_edit::emacs_core::textprop;

#[test]
fn fontification_no_alloc_conversions_match_all_backends_after_edits() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::implemented_variants() {
        let mut expected = "aé中λ".repeat(600);
        let mut text = BufferText::try_from_str_with_backend_kind(&expected, kind).unwrap();
        assert_eq!(anchor_storage(&text), (0, 0));
        assert_conversions(&text, &expected);

        let insertion = "文Ω";
        let at = expected.char_indices().nth(700).unwrap().0;
        let extent = TextExtent::from_emacs_bytes(insertion.as_bytes(), true);
        text.insert_measured_emacs_bytes(EmacsBytePos::new(at), insertion.as_bytes(), extent);
        expected.insert_str(at, insertion);
        assert_conversions(&text, &expected);

        let start = expected.char_indices().nth(300).unwrap().0;
        let end = expected.char_indices().nth(900).unwrap().0;
        let edit = text.edit_range_for_emacs_byte_range(EmacsByteRange::from_usize(start, end));
        text.delete_measured_range(edit);
        expected.replace_range(start..end, "");
        assert_conversions(&text, &expected);
    }
}

#[test]
fn fontification_no_alloc_point_and_resolved_boundary_match_public_queries() {
    crate::test_utils::init_test_tracing();
    let mut ctx = crate::test_utils::new_context();
    ctx.eval_str(
        "(progn (insert (make-string 2400 ?中))
          (put-text-property 2 800 'fontified t)
          (put-text-property 800 1600 'fontified 'defer)
          (put-text-property 1600 2200 'fontified nil)
          (put-text-property 2200 2401 'category 'fontification-noalloc-category)
          (put 'fontification-noalloc-category 'fontified 'category-value)
          (let ((o (make-overlay 1000 1800))) (overlay-put o 'fontified 'overlay-value)))",
    )
    .unwrap();
    for setup in [
        "nil",
        "(narrow-to-region 400 2300)",
        "(progn (widen) (goto-char 900) (insert \"éλ\") (delete-region 1700 1750))",
    ] {
        ctx.eval_str(setup).unwrap();
        let buffer = ctx.buffer_manager().current_buffer().unwrap();
        let min = buffer.point_min_lisp_char_pos().as_i64();
        let max = buffer.point_max_lisp_char_pos().as_i64();
        for position in [min - 1, min, 700, 1200, 1800, max, max + 1] {
            let before = current_anchor_storage(&ctx);
            let native = textprop::fontified_property_at_position_in_state(
                ctx.variables(),
                ctx.buffer_manager(),
                LispCharPos1::new(position),
            );
            assert_eq!(current_anchor_storage(&ctx), before);
            let public = textprop_entries::builtin_get_char_property_3(
                &mut ctx,
                Value::fixnum(position),
                Value::symbol("fontified"),
                Value::NIL,
            );
            assert_eq!(format!("{native:?}"), format!("{public:?}"));
            for end in [min - 1, position, max, max + 30] {
                let before = current_anchor_storage(&ctx);
                let native = textprop::resolved_fontified_property_boundary_in_state(
                    ctx.variables(),
                    ctx.buffer_manager(),
                    LispCharPos1::new(position),
                    LispCharPos1::new(end),
                );
                assert_eq!(current_anchor_storage(&ctx), before);
                if min <= position && position < end && end <= max {
                    let admission = textprop::next_fontified_property_boundary_in_state(
                        ctx.variables(),
                        ctx.buffer_manager(),
                        LispCharPos1::new(position),
                        LispCharPos1::new(end),
                        textprop::FontificationBoundaryMode::On,
                    )
                    .unwrap();
                    let textprop::FontificationBoundaryAdmission::Resolved(next) = admission else {
                        panic!("category-bearing tree must use the resolved path")
                    };
                    assert_eq!(Value::fixnum(next.as_i64()), *native.as_ref().unwrap());
                    assert_eq!(current_anchor_storage(&ctx), before);
                }
                let public = textprop::builtin_next_single_property_change_4(
                    &mut ctx,
                    Value::fixnum(position),
                    Value::symbol("fontified"),
                    Value::NIL,
                    Value::fixnum(end),
                );
                assert_eq!(format!("{native:?}"), format!("{public:?}"));
            }
        }
    }
}

#[test]
fn fontification_no_alloc_raw_and_resolved_boundaries_leave_anchors_unchanged() {
    let mut ctx = crate::test_utils::new_context();
    ctx.eval_str(
        "(progn (insert (make-string 2400 ?中))
           (put-text-property 1 1500 'fontified t)
           (put-text-property 1500 2401 'fontified 'defer))",
    )
    .unwrap();
    for (position, end) in [(1, 2401), (700, 1200), (700, 2000), (1500, 2401)] {
        let before = current_anchor_storage(&ctx);
        let raw = textprop::next_fontified_property_boundary_in_state(
            ctx.variables(),
            ctx.buffer_manager(),
            LispCharPos1::new(position),
            LispCharPos1::new(end),
            textprop::FontificationBoundaryMode::On,
        )
        .unwrap();
        assert!(matches!(
            raw,
            textprop::FontificationBoundaryAdmission::Raw(_)
        ));
        assert_eq!(current_anchor_storage(&ctx), before);
        let public = textprop::builtin_next_single_property_change_4(
            &mut ctx,
            Value::fixnum(position),
            Value::symbol("fontified"),
            Value::NIL,
            Value::fixnum(end),
        )
        .unwrap();
        let textprop::FontificationBoundaryAdmission::Raw(next) = raw else {
            unreachable!()
        };
        assert_eq!(Value::fixnum(next.as_i64()), public);
    }
}

fn anchor_storage(text: &BufferText) -> (usize, usize) {
    text.anchor_storage_snapshot()
}

fn assert_conversions(text: &BufferText, expected: &str) {
    let before = anchor_storage(text);
    for char_pos in [
        0,
        1,
        511,
        1025,
        expected.chars().count() - 1,
        expected.chars().count(),
    ] {
        let byte_pos = expected
            .char_indices()
            .nth(char_pos)
            .map_or(expected.len(), |(byte, _)| byte);
        assert_eq!(
            text.char_pos_to_emacs_byte_pos_no_alloc(CharPos0::new(char_pos)),
            EmacsBytePos::new(byte_pos)
        );
        assert_eq!(
            text.emacs_byte_pos_to_char_pos_no_alloc(EmacsBytePos::new(byte_pos)),
            CharPos0::new(char_pos)
        );
    }
    assert_eq!(
        anchor_storage(text),
        before,
        "native conversions grew anchor storage"
    );
    // The existing cached conversion remains a separate reference, outside the
    // native-query storage observation above.
    for char_pos in [1, 511, 1025, expected.chars().count() - 1] {
        let position = CharPos0::new(char_pos);
        assert_eq!(
            text.char_pos_to_emacs_byte_pos_no_alloc(position),
            text.char_pos_to_emacs_byte_pos(position)
        );
    }
}

fn current_anchor_storage(ctx: &Context) -> (usize, usize) {
    ctx.buffer_manager()
        .current_buffer()
        .unwrap()
        .anchor_storage_snapshot_for_test()
}
