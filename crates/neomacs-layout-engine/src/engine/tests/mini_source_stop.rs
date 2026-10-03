//! Exercise the real mini source producer. Each probe exclusively owns its
//! Context until it returns numeric observations; no Lisp value survives that
//! owner's drop. Each isolated process pins GNU hooks ON before its OnceLock
//! is initialized. The tests never change process environment or shared TLS.
use super::super::{LayoutEngine, LayoutPurpose};
use neovm_core::buffer::{EmacsBytePos, EmacsByteRange, LispCharPos1};
use neovm_core::emacs_core::{Context, Value};
use neovm_core::heap_types::LispString;
use neovm_core::window::WindowLayoutQueryScope;

#[derive(Debug)]
struct Measurement {
    height: i64,
    rows: usize,
    conditions: i64,
    presentation_conditions: Option<i64>,
}

fn measure(
    content: &str,
    after_string: bool,
    transform_newline: bool,
    query_presentation: bool,
) -> Measurement {
    let mut eval = Context::new();
    assert!(
        eval.gnu_redisplay_hooks_policy_enabled(),
        "run this isolate with NEOMACS_REDISPLAY_GNU_HOOKS=on"
    );
    eval.obarray_mut()
        .set_symbol_value("resize-mini-windows", Value::T);
    eval.obarray_mut()
        .set_symbol_value("max-mini-window-height", Value::fixnum(3));
    eval.eval_str("(setq d5-mini-condition-count 0)")
        .expect("condition counter");
    let root = eval
        .buffer_manager()
        .current_buffer()
        .expect("root buffer")
        .id();
    let buffer_id = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-source-stop*");
    eval.buffer_manager_mut()
        .get_mut(buffer_id)
        .expect("mini buffer")
        .insert(content);
    let frame = eval
        .frame_manager_mut()
        .create_frame("source-stop", 120, 40, root);
    {
        let frame = eval.frame_manager_mut().get_mut(frame).expect("frame");
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.shrink_mini_window();
    }
    let window = eval
        .activate_minibuffer_window_for_buffer(buffer_id, LispString::from_utf8("probe: "), None)
        .expect("real mini owner")
        .expect("mini window");
    if transform_newline {
        assert!(content.ends_with('\n'));
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        let eob = buffer.point_max_emacs_byte_pos().get();
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::new(eob - 1), EmacsBytePos::new(eob)),
            Value::symbol("display"),
            Value::string("x"),
        );
    }
    if after_string {
        let string = eval
            .eval_str(
                r#"(let ((s "\nfirst\nsecond\nthird\nfourth\nfifth"))
                     (put-text-property 1 2 'display
                       '(when (progn (setq d5-mini-condition-count
                                           (1+ d5-mini-condition-count)) nil)
                          . "REPLACED") s)
                     s)"#,
            )
            .expect("conditional after-string");
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
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
        let _ = buffer
            .overlays_mut()
            .overlay_put(overlay, Value::symbol("after-string"), string);
    }
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("buffer");
        let eob = buffer.point_max_emacs_byte_pos();
        buffer.goto_emacs_byte_pos(eob);
    }
    eval.sync_runtime_faces_for_frame(frame);
    let mut engine = LayoutEngine::new_without_font_metrics();
    let rows = engine
        .layout_frame_rust_for_purpose_inner(
            &mut eval,
            frame,
            LayoutPurpose::MiniPreparation { window_id: window },
        )
        .expect("canonical ToEnd mini producer")
        .into_geometry()
        .expect("complete measured geometry");
    let height = rows
        .rows
        .last()
        .map(|row| row.y.saturating_add(row.height))
        .expect("last measured row");
    let conditions = eval
        .eval_str("d5-mini-condition-count")
        .expect("read condition counter")
        .as_fixnum()
        .expect("numeric counter");
    let presentation_conditions = query_presentation.then(|| {
        eval.eval_str("(setq d5-mini-condition-count 0)")
            .expect("reset presentation counter");
        let mut presentation = LayoutEngine::new_without_font_metrics();
        presentation
            .layout_frame_rust_for_purpose_inner(
                &mut eval,
                frame,
                LayoutPurpose::SynchronousQuery {
                    window_id: window,
                    scope: WindowLayoutQueryScope::Rows {
                        start: LispCharPos1::ONE,
                        count: std::num::NonZeroUsize::new(10).expect("positive rows"),
                    },
                },
            )
            .expect("ordinary presentation query");
        eval.eval_str("d5-mini-condition-count")
            .expect("read presentation counter")
            .as_fixnum()
            .expect("numeric presentation counter")
    });
    Measurement {
        height,
        rows: rows.rows.len(),
        conditions,
        presentation_conditions,
    }
}

#[test]
fn gnu_mini_to_end_hard_newline_does_not_measure_eob_after_string() {
    let source = measure("probe: input\n", false, false, false);
    let with_overlay = measure("probe: input\n", true, false, false);
    assert_eq!(
        (with_overlay.height, with_overlay.rows),
        (source.height, source.rows),
        "GNU move_it_to(ZV) stops before the virtual tail after a source hard newline: {with_overlay:?}, {source:?}"
    );
}

#[test]
fn gnu_mini_to_end_hard_newline_skips_after_string_condition_until_presentation() {
    let measured = measure("probe: input\n", true, false, true);
    assert_eq!(
        measured.conditions, 0,
        "GNU resize checkpoint observes zero after-string FORM calls during this measurement: {measured:?}"
    );
    assert!(
        measured
            .presentation_conditions
            .expect("presentation control")
            > 0,
        "an ordinary presentation query still reaches this after-string: {measured:?}"
    );
}

#[test]
fn gnu_mini_to_end_without_final_newline_keeps_after_string() {
    let source = measure("probe: input", false, false, false);
    let with_overlay = measure("probe: input", true, false, false);
    assert!(
        with_overlay.height > source.height && with_overlay.rows > source.rows,
        "a same-screen-line EOB after-string remains part of the ToEnd walk: {with_overlay:?}, {source:?}"
    );
    assert!(with_overlay.conditions > 0, "reachable FORM still runs");
}

#[test]
fn gnu_mini_to_end_transformed_final_newline_keeps_inclusive_fallback() {
    let source = measure("probe: input\n", false, true, false);
    let with_overlay = measure("probe: input\n", true, true, false);
    assert!(
        with_overlay.height > source.height && with_overlay.rows > source.rows,
        "a replacing display string removes the proved source hard newline: {with_overlay:?}, {source:?}"
    );
    assert!(
        with_overlay.conditions > 0,
        "an unsupported terminal-source transform retains inclusive condition admission"
    );
}
