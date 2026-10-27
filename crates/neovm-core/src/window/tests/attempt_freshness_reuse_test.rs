//! Request-local buffer capture reuse preserves owned freshness inputs.
use crate::buffer::BufferId;
use crate::emacs_core::{Context, Value};
use crate::window::{FrameId, WindowId};

fn setup() -> (Context, BufferId, FrameId, WindowId) {
    let mut eval = Context::new();
    let live = eval.buffer_manager().current_buffer().unwrap().id();
    let frame = eval
        .frame_manager_mut()
        .create_frame("attempt-reuse", 800, 600, live);
    let window = eval.frame_manager().get(frame).unwrap().selected_window;
    (eval, live, frame, window)
}

#[test]
fn same_buffer_attempt_captures_fontifier_identity_once() {
    let (eval, live, frame, window) = setup();
    // Initialize the existing symbol table and all first-read metadata before
    // isolating this request's capture budget. This is not an Ir measurement.
    eval.window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    crate::emacs_core::intern::reset_intern_calls();
    let namespace_before = crate::emacs_core::intern::symbol_registry_epoch_value();
    let captured = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    let namespace_after = crate::emacs_core::intern::symbol_registry_epoch_value();
    let names = crate::emacs_core::intern::intern_call_names();
    let reads = names
        .iter()
        .filter(|name| name.as_str() == "fontification-functions")
        .count();
    if namespace_before == namespace_after {
        assert_eq!(reads, 1, "stable namespace must retain the one-read budget");
    } else {
        // Another test can legitimately unintern a process-global name. Its
        // publication may be outside this request's guarded interval.
        assert!(
            (1..=2).contains(&reads),
            "unexpected capture count: {reads}"
        );
    }
    assert_eq!(captured.live_buffer, captured.source_buffer);
    assert_eq!(
        captured.live_buffer,
        eval.buffer_layout_inputs(live).unwrap()
    );
}

#[test]
fn same_buffer_attempt_owns_mutable_prefix_table_and_fontifier_inputs() {
    let (mut eval, live, frame, window) = setup();
    eval.eval_str(
        r#"(progn
          (setq ttykey-scroll-capture-called 0
                ttykey-scroll-prefix (copy-sequence "ab")
                ttykey-scroll-glyph (vector ?A)
                buffer-display-table (make-char-table 'display-table nil)
                line-prefix ttykey-scroll-prefix
                wrap-prefix ttykey-scroll-prefix
                fontification-functions nil)
          (put-text-property
           0 1 'display
           '(when (progn
                    (setq ttykey-scroll-capture-called
                          (1+ ttykey-scroll-capture-called))
                    t) . "x")
           ttykey-scroll-prefix)
          (aset buffer-display-table ?a ttykey-scroll-glyph))"#,
    )
    .unwrap();
    let before = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    assert_eq!(before.live_buffer, before.source_buffer);
    let old_debug = format!("{:?}", before);
    eval.eval_str(
        r#"(progn
          (aset ttykey-scroll-prefix 0 ?z)
          (aset ttykey-scroll-glyph 0 ?B)
          (setq fontification-functions (list 'ignore)))"#,
    )
    .unwrap();
    let after = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    assert_eq!(after.live_buffer, after.source_buffer);
    // Both clone members are immutable owned snapshots; mutating the original
    // Lisp objects must not retroactively change either captured member.
    assert_eq!(format!("{:?}", before), old_debug);
    assert_ne!(before.live_buffer.prefixes, after.live_buffer.prefixes);
    assert_ne!(
        before.live_buffer.display_table,
        after.live_buffer.display_table
    );
    assert_ne!(
        before.live_buffer.fontification_functions,
        after.live_buffer.fontification_functions
    );
    assert_eq!(
        before.live_buffer.modified_tick,
        after.live_buffer.modified_tick
    );
    assert_eq!(
        before.live_buffer.chars_modified_tick,
        after.live_buffer.chars_modified_tick
    );
    assert_eq!(
        before.live_buffer.props_modified_tick,
        after.live_buffer.props_modified_tick
    );
    assert_eq!(
        before.live_buffer.overlay_modified_tick,
        after.live_buffer.overlay_modified_tick
    );
    // Metadata capture must not evaluate the embedded display condition.
    assert_eq!(
        eval.eval_str("ttykey-scroll-capture-called").unwrap(),
        Value::fixnum(0)
    );
}

#[test]
fn distinct_buffer_attempt_keeps_two_independent_projections() {
    let (mut eval, live, frame, window) = setup();
    let source = eval
        .buffer_manager_mut()
        .create_buffer("*ttykey-scroll-source*");
    eval.eval_str(
        r#"(progn
          (setq line-prefix "live" fontification-functions nil)
          (set-buffer "*ttykey-scroll-source*")
          (setq line-prefix "source" fontification-functions (list 'ignore)))"#,
    )
    .unwrap();
    eval.window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    crate::emacs_core::intern::reset_intern_calls();
    let captured = eval
        .window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    let names = crate::emacs_core::intern::intern_call_names();
    assert_eq!(
        names
            .iter()
            .filter(|name| name.as_str() == "fontification-functions")
            .count(),
        2
    );
    assert_eq!(captured.live_buffer.id, live);
    assert_eq!(captured.source_buffer.id, source);
    assert_ne!(
        captured.live_buffer.prefixes,
        captured.source_buffer.prefixes
    );
    assert_ne!(
        captured.live_buffer.fontification_functions,
        captured.source_buffer.fontification_functions
    );
    assert_eq!(
        captured.live_buffer,
        eval.buffer_layout_inputs(live).unwrap()
    );
    assert_eq!(
        captured.source_buffer,
        eval.buffer_layout_inputs(source).unwrap()
    );
}

#[test]
fn same_buffer_attempt_rechecks_after_fontifier_namespace_change() {
    // Removing this real canonical name must not disturb another test's
    // process-global registry. Follow the marker-spawned child pattern.
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "window::display::attempt_freshness_reuse_tests::same_buffer_namespace_change_child",
            "--test-threads",
            "1",
        ])
        .env(NAMESPACE_CHANGE_CHILD_ENV, "1")
        .env_remove("RUST_LOG")
        .output()
        .expect("run isolated namespace scenario");
    assert!(
        output.status.success(),
        "namespace child failed: {}\nstdout:\n{}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Marker for the fresh process `same_buffer_attempt_rechecks_after_fontifier_namespace_change`
/// spawns; without it the child body returns at once.
const NAMESPACE_CHANGE_CHILD_ENV: &str = "NEOVM_TEST_NAMESPACE_CHANGE_CHILD";

#[test]
fn same_buffer_namespace_change_child() {
    if std::env::var_os(NAMESPACE_CHANGE_CHILD_ENV).is_none() {
        return;
    }
    use crate::emacs_core::display_when::DisplayWhenSite;
    use crate::emacs_core::intern::{intern, intern_call_names, reset_intern_calls};
    use crate::emacs_core::xdisp::{RedisplayFontificationOutcome, ensure_fontified_for_redisplay};
    use crate::window::WindowLayoutLispBoundary;

    let (mut eval, live, frame, window) = setup();
    // A second owner has only its own Lisp graph. It removes the shared name,
    // not any A-owned hook/list/string, after the real first projection.
    let mut other = Context::new();
    eval.eval_str(
        r#"(progn
          (insert "abcd")
          (setq ttykey-namespace-calls 0
                ttykey-namespace-saved-hook
                (list (lambda (start)
                        (setq ttykey-namespace-calls (1+ ttykey-namespace-calls))
                        (put-text-property start (point-max) 'display "X\nY")
                        (put-text-property start (point-max) 'fontified t)))
                fontification-functions ttykey-namespace-saved-hook
                ttykey-namespace-when
                '(when (progn
                         (set (intern "fontification-functions")
                              ttykey-namespace-saved-hook)
                         nil) . "unused")))"#,
    )
    .expect("install prebuilt hook and dynamic-key WHEN");
    let old_key = intern("fontification-functions");
    assert!(other.obarray().is_global_member(old_key));
    let property = eval.eval_str("ttykey-namespace-when").unwrap();
    let sites = [DisplayWhenSite {
        property,
        object: Value::make_buffer(live),
        position: 1,
        buffer_position: 1,
    }];
    // Warm native metadata and normal WHEN bindings before the evidence
    // interval, so the callback need not load or define a function later.
    eval.evaluate_display_when_sites(live, None, &sites, |spec| spec.is_string())
        .expect("warm WHEN machinery");
    eval.window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    let function_epoch = eval.obarray().function_epoch();
    let original_inputs = eval.buffer_layout_inputs(live).unwrap();
    assert!(original_inputs.fontification_functions.0.is_some());
    reset_intern_calls();
    let mut reads = 0;
    let before = eval
        .window_layout_attempt_freshness_with_capture(frame, window, live, |ctx, buffer| {
            let captured = ctx.buffer_layout_inputs(buffer);
            reads += 1;
            if reads == 1 {
                assert!(other.obarray_mut().unintern_id(old_key));
            }
            captured
        })
        .unwrap();
    assert_eq!(
        reads, 2,
        "namespace change must restore the original second read"
    );
    assert_eq!(
        intern_call_names()
            .iter()
            .filter(|name| name.as_str() == "fontification-functions")
            .count(),
        2
    );
    assert_eq!(before.live_buffer, original_inputs);
    assert_eq!(before.source_buffer.fontification_functions.0, None);
    let fresh_key = intern("fontification-functions");
    assert_ne!(fresh_key, old_key);
    assert_eq!(eval.obarray().function_epoch(), function_epoch);
    assert_eq!(
        ensure_fontified_for_redisplay(&mut eval, live, 0, 4).unwrap(),
        RedisplayFontificationOutcome::Unchanged
    );
    assert_eq!(
        eval.eval_str("ttykey-namespace-calls").unwrap(),
        Value::fixnum(0),
        "fresh canonical key initially has no hook"
    );
    assert!(
        eval.eval_str("(get-text-property 1 'display)")
            .unwrap()
            .is_nil()
    );
    eval.evaluate_display_when_sites(live, None, &sites, |spec| spec.is_string())
        .expect("install exactly the saved F under fresh T through normal WHEN");
    assert!(
        eval.eval_str(
            "(eq (symbol-value (intern \"fontification-functions\")) ttykey-namespace-saved-hook)"
        )
        .unwrap()
        .is_truthy()
    );
    let after = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    assert_eq!(after.live_buffer, original_inputs);
    assert_eq!(after.source_buffer, original_inputs);
    assert_eq!(after.function_epoch, function_epoch);
    assert_eq!(before.body_redisplay, after.body_redisplay);
    assert!(!before.remains_valid_across(&after, WindowLayoutLispBoundary::BufferBody));
    // Counterfactual only: the real old clone token would have matched every
    // field, so a generic revision change cannot explain the refusal above.
    let mut old_cloned_token = before.clone();
    old_cloned_token.source_buffer = old_cloned_token.live_buffer.clone();
    assert_eq!(old_cloned_token, after);

    assert_eq!(
        ensure_fontified_for_redisplay(&mut eval, live, 0, 4).unwrap(),
        RedisplayFontificationOutcome::Fontified,
        "refused attempt must retry the formerly skipped real hook"
    );
    assert_eq!(
        eval.eval_str("ttykey-namespace-calls").unwrap(),
        Value::fixnum(1)
    );
    assert_eq!(
        eval.eval_str("(get-text-property 1 'display)").unwrap(),
        Value::string("X\nY")
    );
    assert!(
        eval.eval_str("(get-text-property 1 'fontified)")
            .unwrap()
            .is_truthy()
    );
    assert!(
        eval.buffer_layout_inputs(live).unwrap().props_modified_tick
            > original_inputs.props_modified_tick
    );
}
