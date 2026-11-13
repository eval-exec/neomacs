//! Exact certified Rows cache tests through the real canonical TTY fixture.
//! No hand-made positive certificate, producer seal or read-set is admitted.
use super::*;
use neovm_core::emacs_core::value::Value;
use std::cell::RefCell;
use std::rc::Rc;

fn on_policy() -> RowsQueryPolicy {
    // Reuse the actual fixture's audited authority. No constructor, producer
    // seal, collection read set or geometry is synthesized by these tests.
    let crate::engine::rows_query_policy::RowsQueryEnvironment::Terminal(host) =
        terminal_policy().environment()
    else {
        unreachable!("the real fixture supplies its typed terminal authority")
    };
    RowsQueryPolicy::terminal(RowsQueryAdmission::ExactProven, host)
}

fn request(
    context: &mut Context,
    engine: &mut LayoutEngine,
    frame: FrameId,
    window: WindowId,
    scope: WindowLayoutQueryScope,
    policy: &RowsQueryPolicy,
) -> (WindowLayoutQuery, Vec<Report>) {
    let mut reports = Vec::new();
    let mut sink = |_context: &mut Context, report| reports.push(report);
    let query = engine
        .query_window_layout_observed_with_policy(
            context,
            frame,
            window,
            scope,
            policy,
            Some(&mut sink),
        )
        .expect("the actual TTY query converges");
    drop(sink);
    (query, reports)
}

fn certified(reports: &[Report]) {
    assert!(
        matches!(reports, [Report::Certified { rows, points }] if *rows > 0 && *points > 0),
        "a real complete canonical producer must mint this candidate: {reports:?}"
    );
}

fn same_complete_query(left: &WindowLayoutQuery, right: &WindowLayoutQuery) {
    assert_eq!(left.end(), right.end());
    // WindowDisplaySnapshot derives Debug over ALL fields but has no PartialEq.
    // Compare the complete numeric snapshot, including regenerated freshness,
    // points/point_rows, every row/region/cursor/end/posn field; no row masking.
    assert_eq!(
        format!("{:?}", left.geometry()),
        format!("{:?}", right.geometry())
    );
}

fn seed(context: &mut Context, engine: &mut LayoutEngine, frame: FrameId, window: WindowId) {
    let (_, reports) = request(context, engine, frame, window, rows(), &on_policy());
    certified(&reports);
}

fn miss(
    context: &mut Context,
    engine: &mut LayoutEngine,
    frame: FrameId,
    window: WindowId,
    scope: WindowLayoutQueryScope,
) -> WindowLayoutQuery {
    let (query, reports) = request(context, engine, frame, window, scope, &on_policy());
    assert!(
        !reports.is_empty(),
        "a refused certified candidate must enter canonical production, never legacy/prepared fallback"
    );
    query
}

fn hit(
    context: &mut Context,
    engine: &mut LayoutEngine,
    frame: FrameId,
    window: WindowId,
) -> WindowLayoutQuery {
    let (query, reports) = request(context, engine, frame, window, rows(), &on_policy());
    assert!(
        reports.is_empty(),
        "an exact certified hit must not enter the canonical producer: {reports:?}"
    );
    query
}

#[test]
fn tty_rows_hit_actual_same_request_preserves_complete_geometry_and_current_freshness() {
    let (mut context, frame, window, source, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let (first, reports) = request(
        &mut context,
        &mut engine,
        frame,
        window,
        rows(),
        &on_policy(),
    );
    certified(&reports);
    let second = hit(&mut context, &mut engine, frame, window);
    same_complete_query(&first, &second);
    assert_eq!(
        second.geometry().unwrap().layout_freshness,
        context.window_display_snapshot_freshness(frame, window, source)
    );
    // Successful cache return must release every source Ref before returning.
    context
        .buffer_manager_mut()
        .get_mut(source)
        .unwrap()
        .insert("after actual hit");
}

#[test]
fn tty_rows_hit_actual_presentation_only_change_regenerates_full_token_like_canonical() {
    let (mut context, frame, window, source, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    let attempt = context
        .window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    let before = context
        .window_display_snapshot_freshness(frame, window, source)
        .unwrap();
    context.eval_str("(force-mode-line-update t)").unwrap();
    let current = context
        .window_display_snapshot_freshness(frame, window, source)
        .unwrap();
    assert_ne!(
        before, current,
        "this must exercise existing absolute_rows placement, not raw token equality"
    );
    assert_eq!(
        attempt,
        context
            .window_layout_attempt_freshness(frame, window, source)
            .unwrap(),
        "the fixture must preserve the COMPLETE attempt facts, never relax them"
    );
    let reused = hit(&mut context, &mut engine, frame, window);
    assert_eq!(reused.geometry().unwrap().layout_freshness, Some(current));
    let mut canonical = LayoutEngine::new_without_font_metrics();
    let (fresh, reports) = request(
        &mut context,
        &mut canonical,
        frame,
        window,
        rows(),
        &terminal_policy(),
    );
    certified(&reports);
    same_complete_query(&reused, &fresh);
}

#[test]
fn tty_rows_hit_exact_scope_never_substitutes_other_start_or_count() {
    let (mut context, frame, window, _, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    for scope in [
        WindowLayoutQueryScope::Rows {
            start: LispCharPos1::ONE,
            count: NonZeroUsize::new(3).unwrap(),
        },
        WindowLayoutQueryScope::Rows {
            start: LispCharPos1::ONE,
            count: NonZeroUsize::new(5).unwrap(),
        },
        WindowLayoutQueryScope::Rows {
            start: LispCharPos1::from_one_based_usize(2),
            count: NonZeroUsize::new(4).unwrap(),
        },
    ] {
        let _ = miss(&mut context, &mut engine, frame, window, scope);
    }
}

#[test]
fn tty_rows_hit_source_point_change_requires_canonical() {
    let (mut context, frame, window, _, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    context.eval_str("(goto-char 2)").unwrap();
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_saved_window_point_change_requires_canonical() {
    let (mut context, frame, window, source, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    let source_point = context
        .buffer_manager()
        .get(source)
        .unwrap()
        .point_lisp_char_pos();
    context
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
        .set_point(LispCharPos1::from_one_based_usize(2));
    assert_eq!(
        context
            .buffer_manager()
            .get(source)
            .unwrap()
            .point_lisp_char_pos(),
        source_point
    );
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_current_buffer_change_requires_canonical() {
    let (mut context, frame, window, _, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    let other = context
        .buffer_manager_mut()
        .create_buffer("stage4-other-current");
    context.buffer_manager_mut().set_current(other);
    context.setup_thread_locals();
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_completed_gc_rejects_original_receipt_before_old_keys() {
    let (mut context, frame, window, _, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    context.gc_collect_exact();
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_new_context_owner_cannot_use_old_context_receipt() {
    let (mut old_context, old_frame, old_window, _, _old_restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut old_context, &mut engine, old_frame, old_window);
    let old_owner = old_context.context_instance_id();
    let (mut context, frame, window, _, _restore) = fixture();
    assert_ne!(old_owner, context.context_instance_id());
    // Do not force numeric-id collisions or move a foreign manager/heap. The
    // existing Core owner tests cover the earlier owner-before-key refusal;
    // this actual engine request must still take canonical rather than hit.
    let _ = miss(&mut context, &mut engine, frame, window, rows());
    // Reset each terminal registry while its own heap is still installed.
    // Context teardown clears TLS; restore the first owner before its guard.
    drop(_restore);
    drop(context);
    old_context.setup_thread_locals();
    drop(_old_restore);
}

fn dependency_mutation(install: &str, mutate: &str) {
    let (mut context, frame, window, source, _restore) = fixture();
    context.eval_str(install).unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    let _ = hit(&mut context, &mut engine, frame, window);
    let attempt = context
        .window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    context.eval_str(mutate).unwrap();
    assert_eq!(
        attempt,
        context
            .window_layout_attempt_freshness(frame, window, source)
            .unwrap(),
        "this fixture must exercise the recorded collection seam independently of typed scalar freshness"
    );
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_actual_read_invisibility_cons_mutation_requires_canonical() {
    dependency_mutation(
        // Both cdr values request ellipsis. The complete typed input therefore
        // stays equal, while the actually read cons must retain its revision.
        "(setq stage4-hit-cons (cons 'stage4-hide t)) (make-local-variable 'buffer-invisibility-spec) (setq buffer-invisibility-spec (list stage4-hit-cons)) (put-text-property 13 17 'invisible 'stage4-hide)",
        "(setcdr stage4-hit-cons 'stage4-other-truthy)",
    );
}

#[test]
fn tty_rows_hit_actual_read_prefix_string_mutation_requires_canonical() {
    dependency_mutation(
        "(setq stage4-hit-string (copy-sequence \">>\")) (put-text-property (point-min) (point-max) 'line-prefix stage4-hit-string)",
        "(aset stage4-hit-string 0 ?!)",
    );
}

#[test]
fn tty_rows_hit_actual_projected_face_vector_mutation_requires_canonical() {
    // A non-named vector contributes no face attributes, but resolving the
    // value projects its real VecLike header through Value::kind. The vector
    // dependency must not be dropped just because these contents are ignored.
    dependency_mutation(
        "(setq stage4-hit-vector (vector 'not-a-named-face)) (put-text-property 1 3 'face stage4-hit-vector)",
        "(aset stage4-hit-vector 0 'still-not-a-named-face)",
    );
}

#[test]
fn tty_rows_hit_disjoint_writes_then_original_journal_overflow_never_rebases() {
    let (mut context, frame, window, _, _restore) = fixture();
    // Held locally only across inert non-GC native writes/queries. The
    // fixture disables allocation-triggered GC; no callback runs in positive
    // queries. Do not add a collection until this local Value is dropped.
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    // Pinned to collection_reads::JOURNAL_SIZE=8192; no production API/state.
    const ORIGINAL_JOURNAL_CAPACITY: usize = 8192;
    for index in 0..ORIGINAL_JOURNAL_CAPACITY / 2 {
        unrelated.set_car(Value::fixnum(index as i64));
    }
    let _ = hit(&mut context, &mut engine, frame, window);
    for index in ORIGINAL_JOURNAL_CAPACITY / 2..=ORIGINAL_JOURNAL_CAPACITY {
        unrelated.set_car(Value::fixnum(index as i64));
    }
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_killed_recreated_same_name_text_and_points_requires_canonical() {
    let (mut context, frame, window, source, _restore) = fixture();
    let name = context
        .buffer_manager()
        .get(source)
        .unwrap()
        .name_runtime_string_owned();
    let text = context
        .buffer_manager()
        .get(source)
        .unwrap()
        .buffer_string();
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    assert!(context.buffer_manager_mut().kill_buffer(source));
    let replacement = context.buffer_manager_mut().create_buffer(&name);
    assert_ne!(source, replacement);
    context
        .buffer_manager_mut()
        .get_mut(replacement)
        .unwrap()
        .insert(&text);
    context.buffer_manager_mut().set_current(replacement);
    context
        .frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .find_window_mut(window)
        .unwrap()
        .set_buffer(replacement);
    context.eval_str("(make-local-variable 'fontification-functions) (setq fontification-functions (list 'ignore)) (put-text-property (point-min) (point-max) 'fontified t) (goto-char 1)").unwrap();
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_report_sink_gc_prevents_publication_without_new_baseline() {
    let (mut context, frame, window, _, _restore) = fixture();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let mut seen = false;
    let mut sink = |context: &mut Context, report| {
        certified(&[report]);
        seen = true;
        context.gc_collect_exact();
    };
    engine
        .query_window_layout_observed_with_policy(
            &mut context,
            frame,
            window,
            rows(),
            &on_policy(),
            Some(&mut sink),
        )
        .unwrap();
    drop(sink);
    assert!(
        seen,
        "the actual first canonical proof reaches the post-proof sink"
    );
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_report_sink_read_string_mutation_prevents_publication() {
    let (mut context, frame, window, _, _restore) = fixture();
    context.eval_str("(setq stage4-publish-string (copy-sequence \">>\")) (put-text-property (point-min) (point-max) 'line-prefix stage4-publish-string)").unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let mut seen = false;
    let mut sink = |context: &mut Context, report| {
        certified(&[report]);
        seen = true;
        context
            .eval_str("(aset stage4-publish-string 0 ?!)")
            .unwrap();
    };
    engine
        .query_window_layout_observed_with_policy(
            &mut context,
            frame,
            window,
            rows(),
            &on_policy(),
            Some(&mut sink),
        )
        .unwrap();
    drop(sink);
    assert!(seen);
    let _ = miss(&mut context, &mut engine, frame, window, rows());
}

#[test]
fn tty_rows_hit_dynamic_predicate_remains_actual_repeated_canonical_callback() {
    let (mut context, frame, window, _, _restore) = fixture();
    context.eval_str("(setq stage4-when-calls 0) (fset 'stage4-when (lambda () (setq stage4-when-calls (1+ stage4-when-calls)) t)) (put-text-property 1 4 'display '(when (stage4-when) . \"when\"))").unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let mut previous = 0;
    for _ in 0..2 {
        let (_, reports) = request(
            &mut context,
            &mut engine,
            frame,
            window,
            rows(),
            &on_policy(),
        );
        assert_eq!(reports, vec![Report::Refused(Refusal::DynamicOrRetry)]);
        let calls = context
            .eval_str("stage4-when-calls")
            .unwrap()
            .as_int()
            .unwrap();
        assert!(
            calls > previous,
            "each actual canonical query must call the predicate again"
        );
        previous = calls;
    }
}

#[test]
fn tty_rows_hit_fontifier_leaving_nil_remains_actual_repeated_canonical_callback() {
    let (mut context, frame, window, _, _restore) = fixture();
    context.eval_str("(setq stage4-fontify-positions nil fontification-functions (list (lambda (position) (setq stage4-fontify-positions (cons position stage4-fontify-positions))))) (put-text-property 1 2 'fontified nil)").unwrap();
    let mut engine = LayoutEngine::new_without_font_metrics();
    let mut previous = 0;
    for _ in 0..2 {
        let (_, reports) = request(
            &mut context,
            &mut engine,
            frame,
            window,
            rows(),
            &on_policy(),
        );
        assert_eq!(reports, vec![Report::Refused(Refusal::Fontification)]);
        let calls = context
            .eval_str("(length stage4-fontify-positions)")
            .unwrap()
            .as_int()
            .unwrap();
        assert!(
            calls > previous,
            "an unfontified callback cannot be bypassed by a cache entry"
        );
        previous = calls;
    }
}

#[test]
fn tty_rows_hit_normal_gc_hook_query_observes_actual_driver_extent() {
    let (mut context, _frame, _window, _, _restore) = fixture();
    let engine = Rc::new(RefCell::new(LayoutEngine::new_without_font_metrics()));
    let reports = Rc::new(RefCell::new(Vec::new()));
    let callback_engine = Rc::clone(&engine);
    let callback_reports = Rc::clone(&reports);
    context
        .install_window_layout_query(move |context, frame, window, scope| {
            let mut sink =
                |_context: &mut Context, report| callback_reports.borrow_mut().push(report);
            match callback_engine
                .borrow_mut()
                .query_window_layout_observed_with_policy(
                    context,
                    frame,
                    window,
                    scope,
                    &on_policy(),
                    Some(&mut sink),
                ) {
                Ok(query) => neovm_core::window::WindowLayoutQueryOutcome::Ready(query),
                Err(failure) => neovm_core::window::WindowLayoutQueryOutcome::Failed(failure),
            }
        })
        .expect("install the layout-query handler");
    // Define the hook before capture so its function/variable identities are
    // not installed after a purported cached prefix. No raw Context alias or
    // inaccessible gc_driver_active field is touched from Layout.
    context.eval_str("(setq noninteractive nil stage4-gc-hook-calls 0 post-gc-hook (list (lambda () (setq stage4-gc-hook-calls (1+ stage4-gc-hook-calls)) (save-excursion (vertical-motion 1)))))").unwrap();
    context
        .eval_str("(save-excursion (vertical-motion 1))")
        .unwrap();
    assert!(
        reports
            .borrow()
            .iter()
            .any(|report| matches!(report, Report::Certified { .. })),
        "the normal motion adapter must first create an actual canonical candidate: {:?}",
        reports.borrow()
    );
    reports.borrow_mut().clear();
    // Direct Rust exact GC is outside a memoized evaluator run. Its normal
    // completed-collection callback runs inside the native driver extent.
    context.gc_collect_exact();
    assert!(
        context
            .eval_str("stage4-gc-hook-calls")
            .unwrap()
            .as_int()
            .unwrap()
            > 0
    );
    assert!(
        reports.borrow().contains(&Report::Refused(Refusal::Core(
            RowsObservationRefusal::DriverActive
        ))),
        "a real post-GC callback query must refuse while the original driver remains active"
    );
}

// CONT13: real producer seeds only; these tests never construct a receipt.
fn original_refused_matches_complete_off(
    context: &mut Context,
    engine: &mut LayoutEngine,
    frame: FrameId,
    window: WindowId,
    source: BufferId,
) {
    assert!(
        engine
            .query_cache
            .get_certified(context, frame, window, rows())
            .is_none(),
        "the stale original attempt must refuse before returning saved geometry",
    );
    let actual = miss(context, engine, frame, window, rows());
    let mut canonical = LayoutEngine::new_without_font_metrics();
    let (expected, _) = request(
        context,
        &mut canonical,
        frame,
        window,
        rows(),
        &terminal_policy(),
    );
    same_complete_query(&actual, &expected);
    assert_eq!(
        actual.geometry().unwrap().layout_freshness,
        context.window_display_snapshot_freshness(frame, window, source),
        "canonical fallback must return complete CURRENT freshness",
    );
}

#[test]
fn tty_rows_hit_actual_detached_overlay_move_refuses_original_attempt() {
    let (mut context, frame, window, source, _restore) = fixture();
    // Like paren.el's persistent overlays, create and DELETE before the
    // canonical seed. A detached OverlayObj must not be projected by it.
    let detached = context
        .eval_str(
            "(progn (setq cont13-moving-overlay (make-overlay (point-max) (point-max))) (delete-overlay cont13-moving-overlay) (overlay-buffer cont13-moving-overlay))",
        )
        .unwrap();
    assert!(
        detached.is_nil(),
        "the retained overlay must actually be detached"
    );
    let mut engine = LayoutEngine::new_without_font_metrics();
    seed(&mut context, &mut engine, frame, window);
    let original = context
        .window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    let _ = hit(&mut context, &mut engine, frame, window);
    let before_move = context
        .window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    assert_eq!(
        original, before_move,
        "the positive seed/hit must precede the ONLY intervening overlay move",
    );
    let (overlay_tick, text_ticks, point) = {
        let buffer = context.buffer_manager().get(source).unwrap();
        (
            buffer.overlay_modified_tick(),
            (
                buffer.modified_tick(),
                buffer.chars_modified_tick(),
                buffer.props_modified_tick(),
            ),
            buffer.point_lisp_char_pos(),
        )
    };
    context
        .eval_str("(move-overlay cont13-moving-overlay 2 6)")
        .unwrap();
    let after_move = context
        .window_layout_attempt_freshness(frame, window, source)
        .unwrap();
    let buffer = context.buffer_manager().get(source).unwrap();
    assert_eq!(buffer.overlay_modified_tick(), overlay_tick.wrapping_add(1));
    assert_eq!(
        (
            buffer.modified_tick(),
            buffer.chars_modified_tick(),
            buffer.props_modified_tick(),
        ),
        text_ticks,
        "moving an overlay must exercise overlay revision, not text/property edits",
    );
    assert_eq!(buffer.point_lisp_char_pos(), point);
    assert_ne!(
        before_move, after_move,
        "the move ALONE changes the full attempt"
    );
    assert_ne!(
        original, after_move,
        "original full-attempt equality must refuse"
    );
    // No creation, deletion or property edit intervened after the real
    // positive seed/hit. Reattaching this detached overlay is the mutation.
    // Active native OverlayObj Boxes still receive no broadened authority.
    original_refused_matches_complete_off(&mut context, &mut engine, frame, window, source);
}

#[test]
fn tty_rows_hit_actual_frame_resize_refuses_original_geometry_in_both_directions() {
    for ((from_width, from_height), (to_width, to_height)) in
        [((120, 40), (100, 32)), ((100, 32), (120, 40))]
    {
        let (mut context, frame, window, source, _restore) = fixture();
        context
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .resize_pixelwise(from_width, from_height);
        let mut engine = LayoutEngine::new_without_font_metrics();
        seed(&mut context, &mut engine, frame, window);
        let _ = hit(&mut context, &mut engine, frame, window);
        let original = context
            .window_layout_attempt_freshness(frame, window, source)
            .unwrap();
        let snapshot = context
            .window_display_snapshot_freshness(frame, window, source)
            .unwrap();
        let frame_inputs = context.frame_manager().get(frame).unwrap().layout_inputs();
        context
            .frame_manager_mut()
            .get_mut(frame)
            .unwrap()
            .resize_pixelwise(to_width, to_height);
        assert_ne!(
            frame_inputs,
            context.frame_manager().get(frame).unwrap().layout_inputs(),
            "real resize changes the existing frame geometry projection",
        );
        assert_ne!(
            original,
            context
                .window_layout_attempt_freshness(frame, window, source)
                .unwrap(),
            "original full-attempt equality must refuse changed frame/window geometry",
        );
        let current = context
            .window_display_snapshot_freshness(frame, window, source)
            .unwrap();
        assert!(
            !snapshot.same_query_row_content(&current),
            "placement compatibility must not align away width/height or window bounds",
        );
        original_refused_matches_complete_off(&mut context, &mut engine, frame, window, source);
    }
}
