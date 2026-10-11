//! Consume real MiniPreparation rows rather than fabricated row snapshots.
//! One exclusively owned Context supplies every source and overlay object;
//! no process environment or thread-local Lisp state is changed by the fixture.
use super::super::{LayoutEngine, LayoutPurpose};
use super::*;
use neovm_core::emacs_core::{Context, Value};
use neovm_core::heap_types::LispString;
use neovm_core::window::WindowLayoutQueryScope;

fn canonical_rows(content: &str, presentation: bool) -> (WindowDisplaySnapshot, LispCharPos1, i64) {
    let mut eval = Context::new();
    eval.obarray_mut()
        .set_symbol_value("resize-mini-windows", Value::T);
    eval.obarray_mut()
        .set_symbol_value("max-mini-window-height", Value::fixnum(3));
    let root = eval
        .buffer_manager()
        .current_buffer()
        .expect("root buffer")
        .id();
    let buffer = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-source-start*");
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .expect("mini buffer")
        .insert(content);
    let frame = eval
        .frame_manager_mut()
        .create_frame("source-start-rows", 120, 40, root);
    {
        let frame = eval.frame_manager_mut().get_mut(frame).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
    }
    let window = eval
        .activate_minibuffer_window_for_buffer(buffer, LispString::from_utf8("probe: "), None)
        .expect("real mini owner")
        .expect("mini window");
    let end = {
        let buffer = eval.buffer_manager_mut().get_mut(buffer).expect("buffer");
        let eob = buffer.point_max_emacs_byte_pos().get();
        let overlay = Value::make_overlay(neovm_core::heap_types::OverlayDataInit {
            serial: 0,
            plist: Value::NIL,
            buffer: Some(buffer.id()),
            start: eob,
            end: eob,
            front_advance: false,
            rear_advance: false,
        });
        buffer.overlays_mut().insert_overlay(overlay);
        let _ = buffer.overlays_mut().overlay_put(
            overlay,
            Value::symbol("after-string"),
            Value::string("\nfirst\nsecond\nthird\nfourth\nfifth"),
        );
        buffer.goto_emacs_byte_pos(neovm_core::buffer::EmacsBytePos::new(eob));
        buffer.point_lisp_char_pos()
    };
    eval.sync_runtime_faces_for_frame(frame);
    let mut engine = LayoutEngine::new_without_font_metrics();
    let query = engine
        .layout_frame_rust_for_purpose_inner(
            &mut eval,
            frame,
            if presentation {
                LayoutPurpose::SynchronousQuery {
                    window_id: window,
                    scope: WindowLayoutQueryScope::Rows {
                        start: LispCharPos1::ONE,
                        count: std::num::NonZeroUsize::new(10).expect("positive row count"),
                    },
                }
            } else {
                LayoutPurpose::MiniPreparation { window_id: window }
            },
        )
        .expect("canonical ToEnd mini producer");
    let snapshot = query
        .into_geometry()
        .expect("complete canonical row geometry");
    let target_y = snapshot
        .rows
        .last()
        .expect("last ToEnd row")
        .y
        .saturating_sub(2);
    tracing::info!(rows = ?snapshot.rows, ?end, target_y, "canonical mini source-start rows");
    (snapshot, end, target_y)
}

#[test]
fn mini_preparation_after_string_normalizes_to_prompt_screen_line() {
    let (rows, end, target_y) = canonical_rows("probe: ", false);
    assert_eq!(end, LispCharPos1::new(8));
    assert!(
        rows.rows.len() >= 6,
        "the full after-string height is still measured: {:?}",
        rows.rows
    );
    assert_eq!(
        aligned_after_string_start(&rows, target_y, end),
        Some(LispCharPos1::ONE),
        "GNU live overlay-mini fixture keeps the prompt's source screen line: {:?}",
        rows.rows
    );
}

#[test]
fn mini_preparation_final_newline_keeps_distinct_empty_source_screen_line() {
    // Presentation still renders the virtual tail. Its actual hard-newline
    // provenance guards alignment independently of GNU ToEnd's earlier stop.
    let content = "probe: input\n";
    let (rows, end, target_y) = canonical_rows(content, true);
    let newline = LispCharPos1::new(content.chars().count() as i64);
    let source_line = rows
        .rows
        .iter()
        .find(|row| row.end_buffer_pos == Some(newline))
        .expect("hard buffer newline has its own source row");
    assert_eq!(
        source_line.end_source,
        DisplayRowEndSource::Buffer,
        "a hard buffer newline cannot be mistaken for an after-string row end: {:?}",
        rows.rows
    );
    assert!(
        source_line.end_buffer_pos.expect("hard newline") < end,
        "terminator owns the newline, not the following empty EOB source line"
    );
    assert_eq!(
        aligned_after_string_start(&rows, target_y, end),
        Some(end),
        "the EOB source row is distinct from the previous physical line: {:?}",
        rows.rows
    );
}

#[test]
fn mini_preparation_wrapped_source_tail_keeps_a_buffer_screen_line_start() {
    let content = format!("probe: {}", "a".repeat(260));
    let (rows, end, target_y) = canonical_rows(&content, false);
    let start = aligned_after_string_start(&rows, target_y, end).expect("screen-line source start");
    assert!(
        LispCharPos1::ONE < start && start < end,
        "screen-line alignment retains the actual wrapped buffer continuation, not a virtual EOB offset: {:?}",
        rows.rows
    );
}
