//! Active GNU `display_mode_element` catches signals, but lets throws escape.

use super::*;
use crate::emacs_core::Context;
use crate::emacs_core::eval::save_scratch_gc_roots;

fn context() -> Context {
    // nextest runs each test in its own process. This process knob contains
    // no Lisp state and is shared read-only by every mutator after first use.
    if std::env::var_os("NEOVM_MODE_LINE_FLOW").is_none() {
        unsafe { std::env::set_var("NEOVM_MODE_LINE_FLOW", "1") };
    }
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.set_variable("noninteractive", Value::NIL);
    eval
}

#[test]
fn mode_line_throw_reaches_enclosing_catch() {
    let mut eval = context();
    let roots = save_scratch_gc_roots();
    let specpdl = eval.specpdl.len();
    // Captured from active GNU 31.1 in the peer's review-oracle-active-result.txt.
    let value = eval
        .eval_str(r#"(catch 'escaped (format-mode-line '("A" (:eval (throw 'escaped "escaped")) "B") 0))"#)
        .expect("mode-line throw reaches catch");
    assert_eq!(value.as_utf8_str(), Some("escaped"));
    assert_eq!(save_scratch_gc_roots(), roots);
    assert_eq!(eval.specpdl.len(), specpdl);
}

#[test]
fn mode_line_split_throw_restores_buffer_and_roots() {
    let mut eval = context();
    let saved_buffer = eval.buffers.current_buffer_id();
    let target = eval.buffers.create_buffer("fx2-target");
    let format = eval.eval_str("'(:eval nil)").expect("format");
    let roots = save_scratch_gc_roots();
    let result = finish_format_mode_line_in_state_with_eval(
        crate::emacs_core::eval::VariableView::new(&eval.obarray, &eval.owner),
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        &[
            format,
            Value::fixnum(0),
            Value::NIL,
            Value::make_buffer(target),
        ],
        |_, _| {
            Err(Flow::throw(
                Value::symbol("escaped"),
                Value::string("escaped"),
            ))
        },
    );
    assert!(result.is_err());
    assert_eq!(eval.buffers.current_buffer_id(), saved_buffer);
    assert_eq!(save_scratch_gc_roots(), roots);
}

#[test]
fn mode_line_split_signal_is_demoted_like_gnu() {
    let mut eval = context();
    let format = eval
        .eval_str(r#"'("before" (:eval nil) "after")"#)
        .expect("format");
    let result = finish_format_mode_line_in_state_with_eval(
        crate::emacs_core::eval::VariableView::new(&eval.obarray, &eval.owner),
        &[],
        &eval.frames,
        &mut eval.buffers,
        &eval.processes,
        &[format, Value::fixnum(0)],
        |_, _| {
            Err(signal(
                LispCondition::Error,
                vec![Value::string("fx2 signal")],
            ))
        },
    )
    .expect("GNU catches signals in :eval and continues formatting");
    // Active GNU oracle: format-signal-outer-condition-case in results.el.
    assert_eq!(result.as_utf8_str(), Some("beforeafter"));
}

#[test]
fn mode_line_nested_collecting_throw_restores_all_root_scopes() {
    let mut eval = context();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    let roots = save_scratch_gc_roots();
    let specpdl = eval.specpdl.len();
    let buffer = eval.buffers.current_buffer_id();
    let value = eval
        .eval_str(
            r#"
        (catch 'escaped
          (format-mode-line
            '("%%" (:propertize
                       ("prefix" (-8 (:eval (progn
                              (garbage-collect)
                              (throw 'escaped (concat "esc" "aped"))))))
                       face bold)
                    "after") 0))
    "#,
        )
        .expect("nested collecting throw");
    assert_eq!(value.as_utf8_str(), Some("escaped"));
    assert_eq!(eval.buffers.current_buffer_id(), buffer);
    assert_eq!(save_scratch_gc_roots(), roots);
    assert_eq!(eval.specpdl.len(), specpdl);
}

#[test]
fn mode_line_display_restored_match_string_survives_collecting_eval() {
    let mut eval = context();
    eval.gc_stress = false;
    eval.tagged_heap.set_gc_threshold(usize::MAX);
    eval.eval_str(r#"(string-match "ab" (concat "ab" "cd"))"#)
        .expect("saved match");
    let format = eval
        .eval_str(r#"'(:eval (progn (string-match "xy" "xyz") (garbage-collect) "ok"))"#)
        .expect("collecting display format");
    let roots = save_scratch_gc_roots();
    let output = try_format_mode_line_for_display_with_sources(
        &mut eval,
        format,
        Value::NIL,
        Value::NIL,
        80,
    )
    .expect("display evaluation");
    assert_eq!(output.value().as_utf8_str(), Some("ok"));
    let searched = eval
        .match_data
        .as_ref()
        .and_then(crate::emacs_core::regex::MatchData::searched_string)
        .and_then(crate::emacs_core::regex::SearchedString::as_lisp_string)
        .expect("restored heap match string");
    assert_eq!(searched.as_utf8_str(), Some("abcd"));
    assert_eq!(
        eval.eval_str("(match-beginning 0)")
            .expect("restored match beginning")
            .as_fixnum(),
        Some(0)
    );
    assert_eq!(
        eval.eval_str("(match-end 0)")
            .expect("restored match end")
            .as_fixnum(),
        Some(2)
    );
    assert_eq!(save_scratch_gc_roots(), roots);
}

#[test]
fn mode_line_frame_title_throw_restores_edit_adjusted_point() {
    let mut eval = context();
    eval.gc_stress = false;
    let bid = eval.buffers.current_buffer_id().expect("current buffer");
    eval.frames.create_frame("fx2-title", 80, 24, bid);
    eval.eval_str("(insert \"abcdefghij\") (goto-char 3)")
        .expect("initial point");
    let format = eval.eval_str(r#"'(:eval (progn (goto-char 1) (insert "XY") (goto-char 7) (garbage-collect) (throw 'escaped "escaped")))"#)
        .expect("frame title format");
    eval.set_variable("fx2-title-format", format);
    eval.redisplay_fn = Some(Box::new(|eval| {
        let frame = eval.frames.selected_frame().expect("selected frame");
        let window = Value::make_window(frame.selected_window.0);
        let format = eval
            .obarray
            .symbol_value_copied("fx2-title-format")
            .expect("format");
        let result = try_format_frame_title_for_display(eval, format, window, Value::NIL, 80);
        if let Err(flow) = result {
            eval.defer_mode_line_display_flow(flow);
        }
    }));
    let roots = save_scratch_gc_roots();
    let specpdl = eval.specpdl.len();
    let result = eval
        .eval_str("(catch 'escaped (redisplay t))")
        .expect("title throw");
    assert_eq!(result.as_utf8_str(), Some("escaped"));
    // GNU title-point-results.el: saved point3 tracks preceding insertion to5.
    assert_eq!(
        eval.eval_str("(point)")
            .expect("restored point")
            .as_fixnum(),
        Some(5)
    );
    assert_eq!(eval.buffers.current_buffer_id(), Some(bid));
    assert_eq!(save_scratch_gc_roots(), roots);
    assert_eq!(eval.specpdl.len(), specpdl);
}

#[test]
fn mode_line_snapshot_hook_preserves_original_nonlocal_exit() {
    let mut eval = context();
    let bid = eval.buffers.current_buffer_id().expect("current buffer");
    eval.frames.create_frame("fx2-snapshot", 80, 24, bid);
    eval.frame_snapshot_fn = Some(Box::new(|eval, _| {
        eval.defer_mode_line_display_flow(Flow::throw(
            Value::symbol("escaped"),
            Value::string("escaped"),
        ));
        Err("layout aborted after a mode-line throw".to_string())
    }));
    let result = eval
        .eval_str("(catch 'escaped (neomacs--frame-snapshot))")
        .expect("snapshot hook throw");
    assert_eq!(result.as_utf8_str(), Some("escaped"));
    assert!(eval.frame_snapshot_fn.is_some());
    assert!(!eval.has_mode_line_display_flow());
}

#[test]
fn mode_line_frame_title_uses_noprop_fields_without_point_queries() {
    let mut eval = context();
    let bid = eval.buffers.current_buffer_id().expect("current buffer");
    let frame_id = eval.frames.create_frame("title-fields", 80, 24, bid);
    assert!(eval.frames.select_frame(frame_id));
    let window = Value::make_window(
        eval.frames
            .selected_frame()
            .expect("frame")
            .selected_window
            .0,
    );
    // GNU xdisp.c decode_mode_spec ignores these point-dependent fields for
    // MODE_LINE_TITLE. Parsed minimum widths still pad the empty fields.
    let rendered = try_format_frame_title_for_display(
        &mut eval,
        Value::string("%l|%3c|%C"),
        window,
        Value::NIL,
        80,
    )
    .expect("title fields");
    assert_eq!(rendered.as_utf8_str(), Some("|   |"));
    let format = eval
        .eval_str(r#"'(:propertize "X" face bold mode-line-elt-no 999)"#)
        .expect("property-bearing title input");
    let plain = try_format_frame_title_for_display(&mut eval, format, window, Value::NIL, 80)
        .expect("title drops all properties like store_mode_line_noprop");
    eval.set_variable("ep30-title-result", plain);
    assert_eq!(plain.as_utf8_str(), Some("X"));
    assert!(
        eval.eval_str("(text-properties-at 0 ep30-title-result)")
            .expect("title properties")
            .is_nil()
    );
    // FRAME_MESSAGE_BUF_SIZE is 4*FRAME_COLS; GNU's fallback fills size-1.
    let dashes =
        try_format_frame_title_for_display(&mut eval, Value::string("%-"), window, Value::NIL, 80)
            .expect("title dash fallback");
    assert_eq!(dashes.as_utf8_str(), Some("-".repeat(319).as_str()));
    // A positive field fitting lots_of_dashes selects its complete140 bytes.
    let dashes =
        try_format_frame_title_for_display(&mut eval, Value::string("%1-"), window, Value::NIL, 80)
            .expect("title dash literal");
    assert_eq!(dashes.as_utf8_str(), Some("-".repeat(140).as_str()));
}
