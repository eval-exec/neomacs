use super::*;
use crate::window::display::WindowDisplayContextExt;
use neovm_eval::emacs_core::eval::Context;
use std::borrow::Cow;

fn fixture() -> (Context, FrameId, WindowId, BufferId) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("native-freshness", 160, 160, buffer);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    (eval, frame, window, buffer)
}

#[test]
fn possible_lisp_always_recaptures_the_complete_token() {
    let (mut eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("changed source");
    let after = eval
        .window_layout_attempt_freshness_after_span(
            &before,
            WindowLayoutLispAdmission::MayHaveEntered,
        )
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn native_character_table_initialization_refuses_borrowed_inputs() {
    let (eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    crate::window::note_char_table_layout_mutation();
    let after = eval
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly)
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn native_dispatch_publication_refuses_borrowed_inputs() {
    let (mut eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.obarray_mut()
        .set_symbol_function("native-freshness-dispatch", Value::T);
    let after = eval
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly)
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn previously_captured_fontset_generation_refuses_native_borrow() {
    let (eval, frame, window, buffer) = fixture();
    let mut before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    // Represent a token from immediately before an independent registry write,
    // without mutating process-global font definitions used by other fixtures.
    before.fontset_generation = before.fontset_generation.wrapping_sub(1);
    let after = eval
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly)
        .unwrap();
    assert!(matches!(after, Cow::Owned(_)));
    assert_ne!(after.as_ref(), &before);
    assert_eq!(
        after.as_ref(),
        &eval
            .window_layout_attempt_freshness(frame, window, buffer)
            .unwrap()
    );
}

#[test]
fn native_span_cannot_borrow_another_contexts_input_token() {
    let (eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    let (other, _, _, _) = fixture();
    let after = other
        .window_layout_attempt_freshness_after_span(&before, WindowLayoutLispAdmission::NativeOnly);
    assert!(!matches!(after, Some(Cow::Borrowed(_))));
}

#[test]
fn late_chrome_source_mutations_preserve_completed_body_admission_only() {
    let (mut eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("late source edit");
    crate::window::note_char_table_layout_mutation();
    let mut after = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    // Both records describe the same live source here; property/overlay and
    // fontifier changes are also late source inputs, not owner/resource changes.
    for source in [&mut after.live_buffer, &mut after.source_buffer] {
        source.props_modified_tick += 1;
        source.overlay_modified_tick += 1;
        source.fontification_functions =
            FontificationHookIdentity(Some(WindowLayoutValueIdentity::of(Value::T)));
    }
    after.window.point = LispCharPos1::new(3);
    after.window.window_start = LispCharPos1::new(2);
    assert_ne!(
        before, after,
        "the original producer certificate stays stale"
    );
    assert!(!before.remains_valid_across(&after, WindowLayoutLispBoundary::BufferBody));
    assert!(before.remains_valid_across(&after, WindowLayoutLispBoundary::WindowChrome));

    // Every source variable, including face-remapping-alist, can change future
    // body layout. Realized face/font resources remain independently guarded.
    use strum::VariantArray;
    for variable in WindowLayoutVariable::VARIANTS {
        let mut changed = before.clone();
        let index = *variable as usize;
        changed.source_layout_variables.values[index] = Some(WindowLayoutValueIdentity::of(
            if before.source_layout_variables.values[index]
                == Some(WindowLayoutValueIdentity::of(Value::T))
            {
                Value::NIL
            } else {
                Value::T
            },
        ));
        assert!(
            !before.remains_valid_across(&changed, WindowLayoutLispBoundary::BufferBody),
            "{variable:?}"
        );
        assert!(
            before.remains_valid_across(&changed, WindowLayoutLispBoundary::WindowChrome),
            "{variable:?}"
        );
    }
}

#[test]
fn late_chrome_keeps_owner_geometry_partition_and_resource_checks() {
    let (eval, frame, window, buffer) = fixture();
    let before = eval
        .window_layout_attempt_freshness(frame, window, buffer)
        .unwrap();
    let changes: &[(&str, fn(&mut WindowLayoutAttemptFreshness))] = &[
        ("context", |t| t.context_instance_id += 1),
        ("topology", |t| t.window_topology_generation += 1),
        ("frame owner", |t| t.frame.id = FrameId(999_999)),
        ("frame geometry", |t| t.frame.geometry.0 += 1),
        ("native scale", |t| t.frame.device_scale_bits ^= 1),
        ("visibility", |t| t.frame.visible = !t.frame.visible),
        ("frame chrome", |t| {
            t.frame.displays_chrome = !t.frame.displays_chrome
        }),
        ("backend", |t| {
            t.frame.has_window_system = !t.frame.has_window_system
        }),
        ("opened font minimum", |t| {
            t.frame.smallest_opened_font_height = Some(3)
        }),
        ("window owner", |t| t.window.id = WindowId(999_999)),
        ("window buffer", |t| t.window.buffer_id = BufferId(999_999)),
        ("live buffer owner", |t| {
            t.live_buffer.id = BufferId(999_999)
        }),
        ("source buffer owner", |t| {
            t.source_buffer.id = BufferId(999_999)
        }),
        ("window bounds", |t| t.window.bounds.0 += 1),
        ("horizontal scroll", |t| t.window.hscroll += 1),
        ("vertical scroll", |t| t.window.vscroll += 1),
        ("preserve vertical scroll", |t| {
            t.window.preserve_vscroll_p = !t.window.preserve_vscroll_p
        }),
        ("margins", |t| {
            t.window.margins = neovm_edit::emacs_core::display_metrics::WindowMargins::new(3, 4)
        }),
        ("left fringe", |t| t.window.left_fringe_width += 1),
        ("right fringe", |t| t.window.right_fringe_width += 1),
        ("fringe partition", |t| {
            t.window.fringes_outside_margins = !t.window.fringes_outside_margins
        }),
        ("glyph capacity", |t| {
            let mut scale =
                neomacs_display_protocol::glyph_matrix_capacity::GlyphMatrixRowScale::default();
            scale.activate();
            scale.grow_if(std::num::NonZeroU32::MIN).unwrap();
            t.window.glyph_matrix_row_scale = scale.snapshot();
        }),
        ("selection", |t| {
            t.selection.frame_selected_window = WindowId(999_999)
        }),
        ("fontset resource", |t| t.fontset_generation += 1),
        ("face resource", |t| t.face_change_count += 1),
        ("media resource", |t| t.media_generation += 1),
        ("explicit body request", |t| {
            t.body_redisplay = BodyRedisplayRevision::new(1, 2, 3)
        }),
    ];
    for (name, change) in changes {
        let mut after = before.clone();
        change(&mut after);
        assert_ne!(before, after, "control must change {name}");
        for boundary in [
            WindowLayoutLispBoundary::BufferBody,
            WindowLayoutLispBoundary::WindowChrome,
        ] {
            assert!(
                !before.remains_valid_across(&after, boundary),
                "{boundary:?} admitted {name}"
            );
        }
    }
}
