//! Real callback boundaries must keep the producer's complete input checks.
//!
//! Each test owns its evaluator and both producers. The reference is the same
//! full producer run from the final live inputs, rather than a second set of
//! layout expectations. No process knobs or shared Lisp state are changed.
use super::*;

fn boundary_fixture() -> (
    Context,
    neovm_core::window::FrameId,
    BufferId,
    neovm_core::window::WindowId,
) {
    let text = format!("{}\nsecond line\n", "abcdefghij".repeat(12));
    let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 160, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(progn
          (setq span-events nil span-mutated nil)
          (set (make-local-variable 'truncate-lines) nil)
          (setq mode-line-format
            '((:eval (progn
                       (setq span-events (cons 'chrome span-events))
                       "BOUNDARY")))))"#,
    )
    .expect("install constant chrome observer and wrapped source");
    (eval, frame, buffer, window)
}

fn boundary_events(eval: &mut Context) -> Vec<Value> {
    neovm_core::emacs_core::value::list_to_vec(
        &eval
            .eval_str("(reverse span-events)")
            .expect("callback order"),
    )
    .expect("proper callback event list")
}

fn assert_before(events: &[Value], first: &str, second: &str) {
    let first_index = events
        .iter()
        .position(|event| *event == Value::symbol(first))
        .unwrap_or_else(|| panic!("missing {first} in {events:?}"));
    let second_index = events
        .iter()
        .position(|event| *event == Value::symbol(second))
        .unwrap_or_else(|| panic!("missing {second} in {events:?}"));
    assert!(
        first_index < second_index,
        "{first} must precede {second}: {events:?}"
    );
}

fn assert_final_source_matches_full_producer(
    eval: &mut Context,
    engine: &LayoutEngine,
    frame: neovm_core::window::FrameId,
) {
    assert!(
        eval.eval_str("span-mutated")
            .expect("one-shot mutation")
            .is_truthy(),
        "the test must cross an actual input-changing Lisp boundary"
    );
    assert!(
        eval.eval_str("truncate-lines")
            .expect("changed input")
            .is_truthy()
    );
    activate_last_engine_presentation(eval, engine, frame);
    let actual = selected_window_layout_trace(eval, engine, frame);
    // The observers have finished their one-shot mutation. The display form,
    // if any, remains installed so its false/error fallback is identical in
    // the reference. Only observers which would add callbacks are disabled.
    eval.eval_str(
        "(setq fontification-functions nil window-scroll-functions nil \
         mode-line-format '(\"BOUNDARY\"))",
    )
    .expect("quiesce observers without changing their final body inputs");
    let mut reference = crate::font::metrics::test_catalog::layout_engine();
    reference.layout_frame_rust(eval, frame);
    activate_last_engine_presentation(eval, &reference, frame);
    assert_eq!(
        actual,
        selected_window_layout_trace(eval, &reference, frame),
        "callbacks must not certify rows produced from pre-callback window parameters"
    );
}

#[test]
fn freshness_spans_keep_unmarking_and_signaling_fontifier_refusals() {
    for callback_result in ["nil", "(error \"fontifier declined after mutation\")"] {
        let (mut eval, frame, _, _) = boundary_fixture();
        eval.eval_str(&format!(
            r#"(setq fontification-functions
              (list (lambda (_position)
                (setq span-events (cons 'font span-events))
                (if span-mutated nil
                  (setq span-mutated t truncate-lines t)
                  {callback_result}))))"#,
        ))
        .expect("install unmarking fontifier");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame);
        let events = boundary_events(&mut eval);
        assert_before(&events, "font", "chrome");
        assert!(
            eval.eval_str("(get-text-property 1 'fontified)")
                .expect("fontifier did not mark source")
                .is_nil(),
            "a callback that never marks fontified must still invalidate changed inputs"
        );
        assert_final_source_matches_full_producer(&mut eval, &engine, frame);
    }
}

#[test]
fn sparse_unmarking_fontifier_refreshes_earlier_faces_and_display_after_error() {
    for (mutation, result, force_rewind) in [
        (
            r#"(put-text-property 6 12 'face '(:foreground "red"))"#,
            "nil",
            true,
        ),
        (
            r#"(put-text-property 6 12 'display "CHANGED\nDISPLAY")"#,
            r#"(error "sparse fontifier changed an earlier display property")"#,
            false,
        ),
    ] {
        const PREFIX: &str = "seed\nBEFORE\n";
        let hidden = "hidden\n".repeat(10_000);
        let tail = PREFIX.chars().count() + hidden.chars().count() + 1;
        let text = format!("{PREFIX}{hidden}TAIL\n");
        let (mut eval, frame, _, window) = incr_editing_frame(&text, 360, 200);
        realize_test_gui_frame(&mut eval, frame);
        eval.eval_str(&format!(
            r#"(progn
              (setq buffer-invisibility-spec t
                    fontadm-events nil fontadm-calls nil fontadm-mutated nil
                    header-line-format nil tab-line-format nil)
              (put-text-property (point-min) (point-max) 'fontified t)
              (put-text-property 13 {tail} 'invisible t)
              (put-text-property {tail} (1+ {tail}) 'fontified nil)
              (put-text-property 6 9 'face 'bold)
              (setq mode-line-format
                '((:eval (progn
                    (setq fontadm-events (cons 'chrome fontadm-events))
                    "FONTADM"))))
              (setq fontification-functions
                (list (lambda (position)
                  (setq fontadm-events (cons 'font fontadm-events)
                        fontadm-calls (cons position fontadm-calls))
                  (if fontadm-mutated nil
                    (setq fontadm-mutated t)
                    {mutation}
                    {result})))))"#,
        ))
        .expect("install a one-shot sparse callback without marking its stop");
        let neovm_core::window::Window::Leaf { force_start, .. } = eval
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .find_window_mut(window)
            .unwrap()
        else {
            panic!("leaf window");
        };
        *force_start = true;

        let rewinds_before = crate::buffer_source::row_route::ROUTE_REWIND_COUNT
            .load(std::sync::atomic::Ordering::Relaxed);
        let refusal = force_rewind
            .then(|| crate::buffer_source::row_route::RoutedRowRefusalInjection::arm(1));
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame);
        drop(refusal);
        if force_rewind {
            assert_eq!(
                crate::buffer_source::row_route::ROUTE_REWIND_COUNT
                    .load(std::sync::atomic::Ordering::Relaxed)
                    - rewinds_before,
                1,
                "the provisional segmented row must exercise its real rewind"
            );
        }
        let events = neovm_core::emacs_core::value::list_to_vec(
            &eval.eval_str("(reverse fontadm-events)").unwrap(),
        )
        .unwrap();
        assert_before(&events, "chrome", "font");
        let calls =
            neovm_core::emacs_core::value::list_to_vec(&eval.eval_str("fontadm-calls").unwrap())
                .unwrap();
        assert!(
            calls.len() >= 2 && calls.iter().all(|p| *p == Value::fixnum(tail as i64)),
            "a late sparse callback must reject the provisional leaf and revisit its still-nil stop: {calls:?}"
        );
        assert!(
            eval.eval_str(&format!("(get-text-property {tail} 'fontified)"))
                .unwrap()
                .is_nil(),
            "retry must follow changed inputs even though fontification never succeeds"
        );
        activate_last_engine_presentation(&mut eval, &engine, frame);
        let actual = selected_window_layout_trace(&eval, &engine, frame);
        let rows = trace_text_rows(&actual);
        assert!(rows.iter().any(|row| row.contains("TAIL")), "{rows:?}");
        assert!(rows.iter().all(|row| !row.contains("hidden")), "{rows:?}");
        if !force_rewind {
            assert!(rows.iter().any(|row| row.contains("CHANGED")), "{rows:?}");
            assert!(rows.iter().any(|row| row.contains("DISPLAY")), "{rows:?}");
        }
        // Use the same live source objects and final properties. Quiesce only
        // observers which would repeat callbacks in the independent producer.
        eval.eval_str("(setq fontification-functions nil mode-line-format '(\"FONTADM\"))")
            .unwrap();
        let mut reference = crate::font::metrics::test_catalog::layout_engine();
        reference.layout_frame_rust(&mut eval, frame);
        activate_last_engine_presentation(&mut eval, &reference, frame);
        assert_eq!(
            actual,
            selected_window_layout_trace(&eval, &reference, frame),
            "an unmarked/error sparse callback can change an already-rendered row; compare resolved faces, glyphs, points, geometry and window end"
        );
    }
}

#[test]
fn freshness_spans_keep_false_and_throwing_display_form_refusals() {
    for form_result in ["nil", "(throw 'span-display-abort t)"] {
        let (mut eval, frame, _, _) = boundary_fixture();
        eval.eval_str(&format!(
            r#"(put-text-property 1 2 'display
              '(when (progn
                       (setq span-events (cons 'display span-events))
                       (if span-mutated nil
                         (setq span-mutated t truncate-lines t))
                       {form_result}) . "REPLACEMENT"))"#,
        ))
        .expect("install a false or throwing conditional display source");
        let mut engine = crate::font::metrics::test_catalog::layout_engine();
        engine.layout_frame_rust(&mut eval, frame);
        assert_before(&boundary_events(&mut eval), "display", "chrome");
        assert_final_source_matches_full_producer(&mut eval, &engine, frame);
    }
}

#[test]
fn freshness_spans_keep_chrome_input_mutation_refusal() {
    let (mut eval, frame, _, _) = boundary_fixture();
    eval.eval_str(
        r#"(setq mode-line-format
          '((:eval (progn
                     (setq span-events (cons 'chrome span-events))
                     (if span-mutated nil
                       (setq span-mutated t truncate-lines t))
                     "BOUNDARY"))))"#,
    )
    .expect("install an input-changing late chrome formatter");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    assert!(boundary_events(&mut eval).contains(&Value::symbol("chrome")));
    assert_final_source_matches_full_producer(&mut eval, &engine, frame);
}

#[test]
fn freshness_spans_keep_scroll_hook_mutation_before_leaf_callbacks() {
    let _policy = neovm_core::emacs_core::eval::RedisplayHookPolicyGuard::gnu();
    let text = format!("{}\n", "abcdefghij".repeat(12)).repeat(12);
    let (mut eval, frame, _, window) = incr_editing_frame(&text, 160, 240);
    realize_test_gui_frame(&mut eval, frame);
    eval.eval_str(
        r#"(progn
          (setq span-events nil span-mutated nil)
          (set (make-local-variable 'truncate-lines) nil)
          (setq mode-line-format '("BOUNDARY")))"#,
    )
    .expect("initial wrapped source");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    engine.layout_frame_rust(&mut eval, frame);
    activate_last_engine_presentation(&mut eval, &engine, frame);
    eval.eval_str(
        r#"(progn
          (setq mode-line-format
            '((:eval (progn
                       (setq span-events (cons 'chrome span-events))
                       "BOUNDARY"))))
          (setq window-scroll-functions
            (list (lambda (_window _start)
              (setq span-events (cons 'scroll span-events))
              (if span-mutated nil
                (setq span-mutated t truncate-lines t))
              (setq fontification-functions
                (list (lambda (position)
                  (setq span-events (cons 'font span-events))
                  (put-text-property position (point-max) 'fontified t)))))))
          (goto-char 364))"#,
    )
    .expect("install scrolling mutation and ordered leaf observers");
    eval.eval_form(Value::list(vec![
        Value::symbol("set-window-start"),
        Value::make_window(window.0),
        Value::fixnum(364),
    ]))
    .expect("commit an explicit start at the fourth long line");
    let FrameLayoutAttempt::Prepared(presentation) =
        engine.redisplay_frame_attempt(&mut eval, frame)
    else {
        panic!("scroll-hook input mutation must converge to a prepared frame");
    };
    // The explicit attempt entry returns ownership to its caller; mirror the
    // ordinary layout_frame_rust adapter before activating/observing its rows.
    engine.last_frame_display_state = Some(presentation);
    let events = boundary_events(&mut eval);
    assert_before(&events, "scroll", "font");
    assert_before(&events, "font", "chrome");
    assert_final_source_matches_full_producer(&mut eval, &engine, frame);
}
