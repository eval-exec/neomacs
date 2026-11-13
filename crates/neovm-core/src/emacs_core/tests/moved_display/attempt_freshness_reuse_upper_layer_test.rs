//! Request-local buffer capture reuse preserves owned freshness inputs.
//!
//! Re-homed from neovm-display (display upper-layer re-home): these tests
//! need the core upper layers that the display unit runtime refuses.
use crate::emacs_core::display_when::DisplayWhenContextExt;
use crate::window::DisplayFrameManagerExt;
use crate::window::display::WindowDisplayContextExt;
use crate::window::{FrameId, WindowId};
use neovm_display::test_support::authority::attempt_freshness_projections_for_test as attempt_freshness_projections;
use neovm_edit::buffer::BufferId;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;

#[test]
fn same_buffer_attempt_owns_mutable_prefix_table_and_fontifier_inputs() {
    let (mut eval, live, frame, window) =
        attempt_freshness_projections(crate::test_utils::new_context());
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
    assert_eq!(before.live_buffer(), before.source_buffer());
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
    assert_eq!(after.live_buffer(), after.source_buffer());
    // Both clone members are immutable owned snapshots; mutating the original
    // Lisp objects must not retroactively change either captured member.
    assert_eq!(format!("{:?}", before), old_debug);
    assert_ne!(before.live_buffer().prefixes, after.live_buffer().prefixes);
    assert_ne!(
        before.live_buffer().display_table,
        after.live_buffer().display_table
    );
    assert_ne!(
        before.live_buffer().fontification_functions,
        after.live_buffer().fontification_functions
    );
    assert_eq!(
        before.live_buffer().modified_tick,
        after.live_buffer().modified_tick
    );
    assert_eq!(
        before.live_buffer().chars_modified_tick,
        after.live_buffer().chars_modified_tick
    );
    assert_eq!(
        before.live_buffer().props_modified_tick,
        after.live_buffer().props_modified_tick
    );
    assert_eq!(
        before.live_buffer().overlay_modified_tick,
        after.live_buffer().overlay_modified_tick
    );
    // Metadata capture must not evaluate the embedded display condition.
    assert_eq!(
        eval.eval_str("ttykey-scroll-capture-called").unwrap(),
        Value::fixnum(0)
    );
}
#[test]
fn distinct_buffer_attempt_keeps_two_independent_projections() {
    let (mut eval, live, frame, window) =
        attempt_freshness_projections(crate::test_utils::new_context());
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
    neovm_value::intern::reset_intern_calls();
    let captured = eval
        .window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    let names = neovm_value::intern::intern_call_names();
    // Each buffer's effective hook is projected by its pre-resolved identity;
    // the two captures below still retain distinct values.
    assert_eq!(
        names
            .iter()
            .filter(|name| name.as_str() == "fontification-functions")
            .count(),
        0,
        "per-buffer captures resolved the hook by name"
    );
    assert_eq!(captured.live_buffer().id, live);
    assert_eq!(captured.source_buffer().id, source);
    assert_ne!(
        captured.live_buffer().prefixes,
        captured.source_buffer().prefixes
    );
    assert_ne!(
        captured.live_buffer().fontification_functions,
        captured.source_buffer().fontification_functions
    );
    assert_eq!(
        captured.live_buffer(),
        &eval.buffer_layout_inputs(live).unwrap()
    );
    assert_eq!(
        captured.source_buffer(),
        &eval.buffer_layout_inputs(source).unwrap()
    );
}
#[test]
fn same_buffer_attempt_keeps_gnu_static_across_fontifier_name_unintern() {
    // Removing this real canonical name must not disturb another test's
    // process-global registry. Follow the marker-spawned child pattern.
    let output = std::process::Command::new(std::env::current_exe().expect("test binary"))
        .args([
            "--exact",
            "emacs_core::moved_display_tests::attempt_freshness_reuse_upper_layer_test::same_buffer_fontifier_name_unintern_child",
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
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "the spawned child did not run (stale --exact path?)\nstdout:\n{}",
        String::from_utf8_lossy(&output.stdout),
    );
}
/// Marker for the fresh process `same_buffer_attempt_rechecks_after_fontifier_namespace_change`
/// spawns; without it the child body returns at once.
const NAMESPACE_CHANGE_CHILD_ENV: &str = "NEOVM_TEST_NAMESPACE_CHANGE_CHILD";

#[test]
fn same_buffer_fontifier_name_unintern_child() {
    if std::env::var_os(NAMESPACE_CHANGE_CHILD_ENV).is_none() {
        return;
    }
    use crate::emacs_core::display_when::DisplayWhenSite;
    use crate::emacs_core::xdisp::{RedisplayFontificationOutcome, ensure_fontified_for_redisplay};
    use crate::window::WindowLayoutLispBoundary;
    use neovm_value::intern::{intern, intern_call_names, reset_intern_calls};

    let (mut eval, live, frame, window) =
        attempt_freshness_projections(crate::test_utils::new_context());
    // A second owner has only its own Lisp graph. It removes the shared name,
    // not any A-owned hook/list/string, after the real first projection.
    let mut other = crate::test_utils::new_context();
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
    eval.evaluate_display_when_sites(live, None, &sites, |spec, _| spec.is_string())
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
        0,
        "both captures read the hook by its pre-resolved identity"
    );
    // GNU holds `Qfontification_functions` as a static: unintern removes only
    // the Lisp name mapping, so the second capture still reads the ORIGINAL
    // symbol's cell. (The retired epoch cache re-resolved by name and would
    // have seen the fresh unbound symbol here.)
    assert_eq!(before.live_buffer(), &original_inputs);
    assert_eq!(before.source_buffer(), &original_inputs);
    let fresh_key = intern("fontification-functions");
    assert_ne!(fresh_key, old_key);
    assert_eq!(eval.obarray().function_epoch(), function_epoch);
    assert_eq!(
        eval.eval_str("(boundp (intern \"fontification-functions\"))")
            .unwrap(),
        Value::NIL,
        "Lisp sees a fresh unbound symbol"
    );

    // A Lisp name unintern does not change any input the C-side attempt
    // reads, so the attempt stays valid; no recheck can be justified by it.
    let after = eval
        .window_layout_attempt_freshness(frame, window, live)
        .unwrap();
    assert_eq!(after.live_buffer(), &original_inputs);
    assert_eq!(after.source_buffer(), &original_inputs);
    assert_eq!(after.function_epoch(), function_epoch);
    assert_eq!(before.body_redisplay(), after.body_redisplay());
    assert!(
        before.remains_valid_across(&after, WindowLayoutLispBoundary::BufferBody),
        "a Lisp-name unintern is invisible to GNU's static fontification variable"
    );
    let mut old_cloned_token = before.clone();
    neovm_display::test_support::authority::set_attempt_source_buffer_from_live_for_test(
        &mut old_cloned_token,
    );
    assert_eq!(old_cloned_token, after);

    // Decisive C-side probe: the fresh Lisp symbol is unbound, yet redisplay
    // still runs the ORIGINAL static variable's hook. A by-name re-resolution
    // would have seen no hook and stayed Unchanged.
    assert_eq!(
        ensure_fontified_for_redisplay(&mut eval, live, 0, 4).unwrap(),
        RedisplayFontificationOutcome::Fontified,
        "GNU's static fontification variable keeps the original hook after name unintern"
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

    // The Lisp name is still usable through the fresh symbol, and installing
    // the saved hook there does not retroactively change what C read.
    eval.evaluate_display_when_sites(live, None, &sites, |spec, _| spec.is_string())
        .expect("install exactly the saved F under fresh T through normal WHEN");
    assert!(
        eval.eval_str(
            "(eq (symbol-value (intern \"fontification-functions\")) ttykey-namespace-saved-hook)"
        )
        .unwrap()
        .is_truthy()
    );
    assert!(
        eval.buffer_layout_inputs(live).unwrap().props_modified_tick
            > original_inputs.props_modified_tick
    );
}
