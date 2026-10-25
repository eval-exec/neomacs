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

fn assert_wrapped_mini_offset_height(property: CandidateOverlayProperty, tail: CandidateTail) {
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
        buffer.insert("1/3    Switch to buffer (default *Messages*): truncation-");
        buffer.set_buffer_local("truncate-lines", Value::NIL);
        buffer.set_buffer_local("word-wrap", Value::NIL);
        let eob = buffer.point_max_emacs_byte_pos();
        buffer.goto_emacs_byte_pos(eob);
    }
    let mini_id = eval
        .activate_minibuffer_window_for_buffer(buffer_id, LispString::from_utf8(""), None)
        .expect("activate mini")
        .expect("mini window");
    let payload = format!(
        " \n  truncation-{}\n  truncation-{}\n  truncation-{}{}",
        "a".repeat(50),
        "b".repeat(50),
        "c".repeat(50),
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
        "property={property:?} tail={tail:?} allocation={allocated_height} top={mini_top} \
         root_bottom={root_bottom} late_content={late_content_height} mini_measurement={measured_height}\n\
         raw(index,enabled,y,height,ends_at_zv,start,end,end_source,text)={raw_rows:?}\n\
         presentation_rows={presentation_rows}\nmeasurement_rows={:?}\n\
         public_height={public_height} public_queries={:?}",
        geometry.rows,
        public_queries.borrow(),
    );
    // GNU resize_mini_window adds last_height only after returning zero
    // current ascent/descent, unless the explicit TRUNCATE exception applies.
    // This wrapped unit-cell fixture predicts automatic9 after the final
    // virtual newline, while the public extent and positive rows remain8.
    let expected_automatic = match tail {
        CandidateTail::FinalNewline => 9.0,
        CandidateTail::FinalText => 8.0,
    };
    assert_eq!(public_height, 8.0, "public text extent: {diagnostic}");
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
    assert_wrapped_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalNewline,
    );
}

#[test]
fn active_mini_wrapped_after_string_final_newline_keeps_its_offset_row_height() {
    assert_wrapped_mini_offset_height(
        CandidateOverlayProperty::AfterString,
        CandidateTail::FinalNewline,
    );
}

#[test]
fn active_mini_wrapped_string_without_final_newline_has_no_extra_offset_row() {
    assert_wrapped_mini_offset_height(
        CandidateOverlayProperty::BeforeString,
        CandidateTail::FinalText,
    );
}
