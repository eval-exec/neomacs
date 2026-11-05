use crate::emacs_core::eval::Context;

use neovm_edit::buffer::buffer_text::*;

use neovm_edit::buffer::{BufferTextBackendKind, LispCharPos1};

use neovm_edit::emacs_core::textprop;

use neovm_edit::emacs_core::textprop_entries;

#[test]
fn fontification_no_alloc_point_and_resolved_boundary_match_public_queries() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
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
    let mut ctx = Context::new();
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

fn current_anchor_storage(ctx: &Context) -> (usize, usize) {
    anchor_storage(&ctx.buffer_manager().current_buffer().unwrap().text)
}

fn anchor_storage(text: &BufferText) -> (usize, usize) {
    let storage = text.storage.borrow();
    let anchors = storage.anchor_cache.borrow();
    (anchors.len(), anchors.capacity())
}
