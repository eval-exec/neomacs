//! GNU automatic minibuffer height and public text extent have distinct
//! contracts for an overlay string ending in a newline.
use super::*;
use neovm_core::emacs_core::eval::RedisplayHookPolicyGuard;
use neovm_core::window::WindowLayoutQueryOutcome;
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug)]
enum CandidateOverlayProperty {
    BeforeString,
    AfterString,
}

impl CandidateOverlayProperty {
    fn name(self) -> &'static str {
        match self {
            Self::BeforeString => "before-string",
            Self::AfterString => "after-string",
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum CandidateTail {
    FinalNewline,
    FinalText,
}

#[derive(Clone, Copy, Debug)]
enum MiniLinePolicy {
    Wrap,
    TruncateVirtualTail,
    TruncateBufferNewline,
    TruncateEmptyNarrowed(EmptyNarrowedPrefix),
    TruncateTrulyEmptyUnclipped,
}

#[derive(Clone, Copy, Debug)]
enum EmptyNarrowedPrefix {
    NonNewline,
    Newline,
    AtBufferStart,
}

fn assert_mini_offset_height(
    property: CandidateOverlayProperty,
    tail: CandidateTail,
    line_policy: MiniLinePolicy,
) {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let mut eval =
        create_bootstrap_evaluator_cached_with_features(&["x", "neomacs"]).expect("bootstrap");
    apply_runtime_startup_state(&mut eval).expect("runtime startup state");
    eval.store_variable_by_name("resize-mini-windows", Value::symbol("grow-only"));
    eval.store_variable_by_name("max-mini-window-height", Value::fixnum(23));
    eval.store_variable_by_name("window-min-height", Value::fixnum(4));
    let root_buffer = eval
        .buffer_manager()
        .current_buffer()
        .expect("root buffer")
        .id();
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("mini-overlay-offset-height", 40, 23, root_buffer);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(None);
        frame.char_width = 1.0;
        frame.char_height = 1.0;
        frame.displays_chrome = false;
        frame.shrink_mini_window();
    }
    eval.eval_form(Value::list(vec![
        Value::symbol("select-frame"),
        Value::make_frame(frame_id.0),
    ]))
    .expect("select fixture frame");

    // Forty columns reserve the continuation marker in a wrapped TTY row.
    // This prompt occupies two display rows, as does each candidate below.
    let buffer_id = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-offset-height*");
    {
        let buffer = eval
            .buffer_manager_mut()
            .get_mut(buffer_id)
            .expect("mini buffer");
        buffer.insert(match line_policy {
            MiniLinePolicy::Wrap => "1/3    Switch to buffer (default *Messages*): truncation-",
            MiniLinePolicy::TruncateVirtualTail => "truncation-",
            MiniLinePolicy::TruncateBufferNewline => "truncation-\n",
            MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::NonNewline) => "A",
            MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::Newline) => "\n",
            MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::AtBufferStart)
            | MiniLinePolicy::TruncateTrulyEmptyUnclipped => "",
        });
        buffer.set_buffer_local(
            "truncate-lines",
            match line_policy {
                MiniLinePolicy::Wrap => Value::NIL,
                MiniLinePolicy::TruncateVirtualTail
                | MiniLinePolicy::TruncateBufferNewline
                | MiniLinePolicy::TruncateEmptyNarrowed(_)
                | MiniLinePolicy::TruncateTrulyEmptyUnclipped => Value::T,
            },
        );
        buffer.set_buffer_local("word-wrap", Value::NIL);
        let eob = buffer.point_max_emacs_byte_pos();
        if matches!(
            line_policy,
            MiniLinePolicy::TruncateEmptyNarrowed(_) | MiniLinePolicy::TruncateTrulyEmptyUnclipped
        ) {
            buffer.narrow_to_emacs_byte_range(neovm_core::buffer::EmacsByteRange::new(eob, eob));
            assert_eq!(
                buffer.point_min_emacs_byte_pos(),
                buffer.point_max_emacs_byte_pos()
            );
        }
        buffer.goto_emacs_byte_pos(eob);
    }
    let mini_id = eval
        .activate_minibuffer_window_for_buffer(buffer_id, LispString::from_utf8(""), None)
        .expect("activate mini")
        .expect("mini window");
    // The real-buffer newline control isolates FETCH_BYTE(ZV_BYTE - 1).
    // An EOB overlay after that newline has a different MOVE_TO_POS stop.
    if !matches!(line_policy, MiniLinePolicy::TruncateBufferNewline) {
        let candidate_width = match line_policy {
            MiniLinePolicy::TruncateTrulyEmptyUnclipped => 1,
            MiniLinePolicy::Wrap
            | MiniLinePolicy::TruncateVirtualTail
            | MiniLinePolicy::TruncateBufferNewline
            | MiniLinePolicy::TruncateEmptyNarrowed(_) => 50,
        };
        let payload = format!(
            " \n  truncation-{}\n  truncation-{}\n  truncation-{}{}",
            "a".repeat(candidate_width),
            "b".repeat(candidate_width),
            "c".repeat(candidate_width),
            match tail {
                CandidateTail::FinalNewline => "\n",
                CandidateTail::FinalText => "",
            },
        );
        let payload = Value::string(payload);
        eval.eval_form(Value::list(vec![
            Value::symbol("put-text-property"),
            Value::fixnum(0),
            Value::fixnum(1),
            Value::list(vec![Value::symbol("quote"), Value::symbol("cursor")]),
            Value::T,
            payload,
        ]))
        .expect("Vertico explicit cursor on the first virtual space");
        {
            let buffer = eval
                .buffer_manager_mut()
                .get_mut(buffer_id)
                .expect("mini buffer");
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
            buffer
                .overlays_mut()
                .overlay_put(overlay, Value::symbol(property.name()), payload)
                .expect("install candidate string");
        }
    }

    let mut engine = LayoutEngine::new_without_font_metrics();
    engine.layout_frame_rust(&mut eval, frame_id);
    let state = engine
        .last_frame_display_state
        .as_ref()
        .expect("accepted frame")
        .state();
    let entry = state
        .window_matrices
        .iter()
        .find(|entry| entry.window_id.get() == mini_id.0 as i64)
        .expect("mini matrix");
    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let mini = frame.find_window(mini_id).expect("mini");
    let allocated_height = mini.bounds().height;
    let mini_top = mini.bounds().y;
    let root_bottom = frame.root_window().bounds().bottom();
    let snapshot = frame.redisplay_snapshot(mini_id).expect("mini source rows");
    let raw_rows: Vec<_> = entry
        .matrix
        .rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let source = snapshot
                .rows
                .iter()
                .find(|source| source.row == index as i64)
                .map(|source| format!("{:?}", source.end_source));
            (
                index,
                row.enabled,
                row.pixel_y,
                row.height_px,
                row.ends_at_zv,
                row.start_charpos,
                row.end_charpos,
                source,
                glyphs_logical_text(&row.glyphs[GlyphArea::Text.index()]),
            )
        })
        .collect();
    // Match content_height_px without changing its private row-grid API.
    // An enabled zero-height placeholder remains visible in raw_rows.
    let late_content_height = entry
        .matrix
        .rows
        .iter()
        .filter(|row| row.enabled && row.height_px > 0.0)
        .map(|row| row.pixel_y + row.height_px)
        .fold(0.0_f32, f32::max);
    let presentation_rows = format!("{:?}", snapshot.rows);

    // Exercise the actual public Lisp API through its canonical frontend,
    // rather than the startup scanner or a fabricated height. Its stop/height
    // contract differs from automatic resize_mini_window after a final newline.
    let public_queries = Rc::new(RefCell::new(Vec::new()));
    let observed = public_queries.clone();
    let mut public_engine = WindowLayoutQueryEngine::new_without_font_metrics();
    eval.install_window_layout_query(move |eval, frame, window, scope| {
        let label = format!("{scope:?}");
        match public_engine.query_window_layout(eval, frame, window, scope) {
            Ok(query) => {
                observed.borrow_mut().push(format!(
                    "scope={label} rows={:?}",
                    query.geometry().map(|geometry| &geometry.rows)
                ));
                WindowLayoutQueryOutcome::Ready(query)
            }
            Err(error) => WindowLayoutQueryOutcome::Failed(error),
        }
    });
    let public_size = eval
        .eval_form(Value::list(vec![
            Value::symbol("window-text-pixel-size"),
            Value::make_window(mini_id.0),
        ]))
        .expect("actual public window-text-pixel-size");
    let public_height = public_size.cons_cdr().as_fixnum().expect("public height") as f32;

    let query = engine
        .layout_frame_rust_for_purpose_inner(
            &mut eval,
            frame_id,
            LayoutPurpose::MiniPreparation { window_id: mini_id },
            None,
            None,
        )
        .expect("canonical unbounded mini measurement");
    let measured_height = engine
        .mini_preparation_height
        .expect("mini measurement result");
    let geometry = query.into_geometry().expect("canonical measurement rows");
    let diagnostic = format!(
        "property={property:?} tail={tail:?} line_policy={line_policy:?} allocation={allocated_height} top={mini_top} \
         root_bottom={root_bottom} late_content={late_content_height} mini_measurement={measured_height}\n\
         raw(index,enabled,y,height,ends_at_zv,start,end,end_source,text)={raw_rows:?}\n\
         presentation_rows={presentation_rows}\nmeasurement_rows={:?}\n\
         public_height={public_height} public_queries={:?}",
        geometry.rows,
        public_queries.borrow(),
    );
    // GNU xdisp.c:13355–13368 suppresses last_height only with zero current
    // ascent/descent, TRUNCATE, current_x <= first_visible_x, and a non-newline
    // final BUFFER byte. A virtual final newline cannot override that byte.
    let (expected_public, expected_automatic) = match (line_policy, tail) {
        (MiniLinePolicy::Wrap, CandidateTail::FinalNewline) => (Some(8.0), 9.0),
        (MiniLinePolicy::Wrap, CandidateTail::FinalText) => (Some(8.0), 8.0),
        (MiniLinePolicy::TruncateVirtualTail, _) => (Some(4.0), 4.0),
        (MiniLinePolicy::TruncateBufferNewline, _) => (Some(1.0), 2.0),
        (MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::NonNewline), _) => (None, 4.0),
        (MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::Newline), _) => (None, 5.0),
        (MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::AtBufferStart), _) => {
            (None, 4.0)
        }
        (MiniLinePolicy::TruncateTrulyEmptyUnclipped, _) => (None, 5.0),
    };
    // Narrowed-empty public query parity is a separate existing contract.
    // Keep the query observable, preserving the seven original public checks.
    if let Some(expected_public) = expected_public {
        assert_eq!(
            public_height, expected_public,
            "public text extent: {diagnostic}"
        );
    }
    // late_content_height is retained as an observation, not forced to match
    // either API: the visible insertion-row materialization is a separate fact.
    assert_eq!(
        measured_height, expected_automatic,
        "GNU automatic mini measurement: {diagnostic}"
    );
    assert_eq!(
        allocated_height, expected_automatic,
        "late active-mini allocation: {diagnostic}"
    );
    assert_eq!(
        mini_top,
        23.0 - expected_automatic,
        "inner-frame prompt row: {diagnostic}"
    );
    assert_eq!(
        root_bottom - 1.0,
        22.0 - expected_automatic,
        "inner-frame last root row: {diagnostic}"
    );
}

#[test]
fn active_mini_wrapped_before_string_final_newline_keeps_its_offset_row_height() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::Wrap,
    );
}

#[test]
fn active_mini_wrapped_after_string_final_newline_keeps_its_offset_row_height() {
    assert_mini_offset_height(
        CandidateOverlayProperty::AfterString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::Wrap,
    );
}

#[test]
fn active_mini_wrapped_string_without_final_newline_has_no_extra_offset_row() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalText,
        MiniLinePolicy::Wrap,
    );
}

#[test]
fn active_mini_truncated_before_string_virtual_newline_does_not_add_offset_height() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::TruncateVirtualTail,
    );
}

#[test]
fn active_mini_truncated_after_string_virtual_newline_does_not_add_offset_height() {
    assert_mini_offset_height(
        CandidateOverlayProperty::AfterString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::TruncateVirtualTail,
    );
}

#[test]
fn active_mini_truncated_virtual_final_text_keeps_its_current_row_height() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalText,
        MiniLinePolicy::TruncateVirtualTail,
    );
}

#[test]
fn active_mini_truncated_buffer_newline_keeps_its_empty_source_row_height() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::TruncateBufferNewline,
    );
}

// Both excluded GNU probe orders observed automatic heights 4/5/4 for
// hidden non-LF / hidden LF / true empty. Narrowed-empty public query parity
// is separate and is recorded without asserting GNU equivalence here.
#[test]
fn active_mini_empty_narrowed_after_nonnewline_uses_preceding_zv_byte() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::NonNewline),
    );
}

#[test]
fn active_mini_empty_narrowed_after_newline_keeps_its_offset_height() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::Newline),
    );
}

#[test]
fn active_mini_truly_empty_buffer_does_not_invent_a_preceding_zv_byte() {
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::TruncateEmptyNarrowed(EmptyNarrowedPrefix::AtBufferStart),
    );
}

#[test]
fn active_mini_truly_empty_unclipped_virtual_newline_keeps_its_offset_height() {
    // Excluded GNU short-line probe: same final newline/true ZV_BYTE==1,
    // but no MOVE_LINE_TRUNCATED, so automatic height is five, not four.
    assert_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
        MiniLinePolicy::TruncateTrulyEmptyUnclipped,
    );
}
