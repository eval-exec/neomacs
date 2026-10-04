//! Zero-query coverage certificate controls against the actual old inspector.
//! Every Context/heap is private to one mutator. Shared row data is initialized
//! immutable numeric geometry; observations contain no Lisp handles or caches.
use super::*;
use crate::incremental_layout::edit_sync::fontify_coverage_test_support::{
    self as coverage, Guard as CoverageGuard,
};
use crate::neovm_bridge::LayoutBufferView;
use crate::redisplay_fontification::VisibleFontificationCoverage;
use neovm_core::buffer::CharPos0;
use neovm_core::window::{
    DisplayPointRole, DisplayPointRow, DisplayPointRows, DisplayPointSnapshot,
    WindowDisplaySnapshot,
};

fn observed_coverage(
    on: bool,
) -> (
    FrameObservation,
    String,
    LayoutStats,
    sync_probes::Counts,
    coverage::Counts,
) {
    let _coverage = CoverageGuard::set(on);
    let observed = observed_sync(true);
    (
        observed.0,
        observed.1,
        observed.2,
        observed.4,
        coverage::counts(),
    )
}

#[test]
fn accepted_sync_elides_fontify_points_after_complete_outputs_and_callbacks() {
    let off = observed_coverage(false);
    let on = observed_coverage(true);
    assert_eq!(
        on.0, off.0,
        "complete frame/faces/source/pointers/points/ends/geometry"
    );
    assert_eq!(
        on.1, off.1,
        "all mode-line/fontifier and positional observations"
    );
    assert_eq!(off.2.edit_windows, 1);
    assert_eq!(on.2.edit_windows, 1);
    assert!(off.2.reused_rows > 0 && on.2.reused_rows > 0);
    assert!(off.3.sync_admissions > 0 && on.3.sync_admissions > 0);
    assert!(
        off.4.completed_sync_installs > 0 && on.4.completed_sync_installs > 0,
        "both real walks must reach finish_edit_sync and install_edit"
    );
    assert!(
        off.4.sync_iterators > 0 && off.4.sync_points > 0,
        "actual original iterator construction/visits: {:?}",
        off.4
    );
    assert_eq!(
        off.4.sync_queries, 0,
        "this actual installed Sync has no uncovered query"
    );
    assert_eq!(on.4.sync_queries, off.4.sync_queries);
    // Only this final site-count assertion is the intended pre-production RED.
    // A compile/setup/reference/admission failure is not a regression receipt.
    assert_eq!(
        on.4.sync_iterators, 0,
        "accepted Sync constructed fontification point iterators"
    );
    assert_eq!(on.4.sync_points, 0);
    assert!(on.4.shortcuts > 0);
}

fn points(row: i64, positions: &[i64]) -> Vec<DisplayPointSnapshot> {
    positions
        .iter()
        .enumerate()
        .map(|(col, &position)| DisplayPointSnapshot {
            buffer_pos: LispCharPos1::new(position),
            role: DisplayPointRole::Glyph,
            x: col as i64,
            y: row,
            width: 1,
            height: 1,
            row,
            col: col as i64,
        })
        .collect()
}
fn snapshot(row_positions: &[&[i64]], flat: &[i64]) -> WindowDisplaySnapshot {
    WindowDisplaySnapshot {
        point_rows: Some(DisplayPointRows {
            rows: row_positions
                .iter()
                .enumerate()
                .map(|(row, positions)| DisplayPointRow::from_points(points(row as i64, positions)))
                .collect(),
        }),
        points: points(0, flat),
        ..Default::default()
    }
}
fn inspect_pair(
    snapshot: &WindowDisplaySnapshot,
    prepass: usize,
    admitted: bool,
) -> (
    VisibleFontificationCoverage,
    coverage::Counts,
    coverage::Counts,
) {
    let text = "a".repeat(100);
    let (mut eval, _frame, buffer_id, _window) = incr_editing_frame(&text, 800, 600);
    eval.eval_str("(put-text-property 6 7 'fontified \"owned-fontified-value\")")
        .unwrap();
    let buffer = eval.buffer_manager().get(buffer_id).unwrap();
    assert_eq!(buffer.layout_point_max_char_pos().get(), 100);
    let original: Vec<_> = snapshot.iter_points().collect();
    let (off_result, off_counts) = {
        let _guard = CoverageGuard::set(false);
        let result = if admitted {
            VisibleFontificationCoverage::inspect_for_edit_sync(
                buffer,
                snapshot,
                CharPos0::new(prepass),
                true,
            )
        } else {
            VisibleFontificationCoverage::inspect(buffer, snapshot, CharPos0::new(prepass))
        };
        (result, coverage::counts())
    };
    let (on_result, on_counts) = {
        let _guard = CoverageGuard::set(true);
        let result = if admitted {
            VisibleFontificationCoverage::inspect_for_edit_sync(
                buffer,
                snapshot,
                CharPos0::new(prepass),
                true,
            )
        } else {
            VisibleFontificationCoverage::inspect(buffer, snapshot, CharPos0::new(prepass))
        };
        (result, coverage::counts())
    };
    assert_eq!(
        on_result, off_result,
        "complete old sparse plan, including merge order"
    );
    assert_eq!(
        snapshot.iter_points().collect::<Vec<_>>(),
        original,
        "certificate must not alter any immutable point/placement"
    );
    assert_eq!(
        on_counts.queries, off_counts.queries,
        "actual property-query count"
    );
    (on_result, off_counts, on_counts)
}

#[test]
fn coverage_preserves_half_open_boundaries_and_literal_eager_consumer() {
    // C(5)=4 is P itself and must be queried; C(101)=100 is E and excluded.
    let lower = snapshot(&[&[5]], &[]);
    let (plan, off, on) = inspect_pair(&lower, 4, true);
    assert!(matches!(plan, VisibleFontificationCoverage::Requires(_)));
    assert_eq!(off.queries, 1);
    assert_eq!(on.iterators, off.iterators);
    let marked = snapshot(&[&[6]], &[]);
    let (plan, off, on) = inspect_pair(&marked, 4, true);
    assert_eq!(plan, VisibleFontificationCoverage::Complete);
    assert_eq!(
        off.queries, 1,
        "truthy fontified still requires the old query"
    );
    assert_eq!(
        on.iterators, off.iterators,
        "range proof cannot guess the property"
    );
    let outside = snapshot(&[&[1, 4], &[101, 102]], &[]);
    assert_eq!(
        inspect_pair(&outside, 4, true).0,
        VisibleFontificationCoverage::Complete
    );
    let (_, off, on) = inspect_pair(&outside, 4, false);
    assert!(
        off.iterators > 0 && on.iterators > 0,
        "non-admitted eager path is literal"
    );
    let mut flat = outside.clone();
    flat.set_points(points(0, &[5, 101]));
    let (_, off, on) = inspect_pair(&flat, 100, true);
    assert_eq!(off.queries, 0);
    assert_eq!(on.queries, 0, "empty [E,E) contains no possible query");
}

#[test]
fn coverage_keeps_unknown_nonpositive_overlapping_and_backward_fallbacks() {
    let mut flat = snapshot(&[], &[]);
    flat.set_points(points(0, &[5, 6]));
    let cases = [
        flat,
        snapshot(&[&[-1, 0, 1]], &[]),
        snapshot(&[&[1, 3], &[2, 4]], &[]),
        snapshot(&[&[3, 4], &[1, 2]], &[]),
    ];
    for (index, case) in cases.iter().enumerate() {
        let prepass = if index == 1 { 0 } else { 4 };
        let (_, off, on) = inspect_pair(case, prepass, true);
        assert!(off.iterators > 0 && off.points > 0);
        assert_eq!(on.iterators, off.iterators, "whole fallback {index}");
        assert_eq!(on.points, off.points, "global iterator visits {index}");
        assert_eq!(on.shortcuts, 0, "no partial-row substitution {index}");
    }
}

#[test]
fn coverage_uses_authoritative_placed_wide_rows_without_mutating_shared_cells() {
    // Some(row data) remains authoritative over the deliberately contradictory
    // flat vector. Both eager inspectors ignore that flat uncovered position.
    let empty = snapshot(&[&[]], &[5]);
    assert_eq!(
        inspect_pair(&empty, 4, true).0,
        VisibleFontificationCoverage::Complete
    );
    let base = DisplayPointRow::from_points(points(0, &[1, 2]));
    let shifted = base.try_replaced_placement(1, 1, 101).unwrap();
    assert_eq!(base.points().next().unwrap().buffer_pos.as_i64(), 1);
    assert_eq!(shifted.points().next().unwrap().buffer_pos.as_i64(), 102);
    let mut wide_points = points(2, &[i64::MAX]);
    wide_points[0].x = i64::MAX;
    let wide = DisplayPointRow::from_points(wide_points);
    assert!(!wide.is_compact());
    assert!(
        wide.try_replaced_placement(2, 2, 1).is_none(),
        "real checked constructor rejects overflow; never forge private metadata"
    );
    let placed = WindowDisplaySnapshot {
        point_rows: Some(DisplayPointRows {
            rows: vec![base.clone(), shifted, wide],
        }),
        points: points(0, &[5]),
        ..Default::default()
    };
    assert_eq!(
        inspect_pair(&placed, 4, true).0,
        VisibleFontificationCoverage::Complete
    );
    assert_eq!(
        base.points()
            .map(|p| p.buffer_pos.as_i64())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}

fn folded_observation(on: bool) -> (FrameObservation, String, coverage::Counts) {
    let _guard = CoverageGuard::set(on);
    const PREFIX: &str = "visible\n";
    let hidden = "hidden\n".repeat(50_000);
    let text = format!("{PREFIX}{hidden}TAIL\n");
    let tail = PREFIX.chars().count() + hidden.chars().count() + 1;
    let (mut eval, frame_id, _buffer, window_id) = incr_editing_frame(&text, 360, 140);
    eval.eval_str(&format!(
        "(progn (setq buffer-invisibility-spec t) \
         (put-text-property {} {tail} 'invisible t) \
         (setq redisplay-fontify-calls nil) \
         (setq fontification-functions (list (lambda (start) \
           (setq redisplay-fontify-calls (cons start redisplay-fontify-calls)) \
           (let ((end (min (point-max) (+ start 80)))) \
             (put-text-property start end 'fontified t) \
             (put-text-property start end 'font-lock-face 'font-lock-warning-face))))))",
        PREFIX.chars().count() + 1
    ))
    .unwrap();
    let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
    let neovm_core::window::Window::Leaf { force_start, .. } =
        frame.find_window_mut(window_id).unwrap()
    else {
        panic!("leaf");
    };
    *force_start = true;
    let mut engine = LayoutEngine::new();
    engine.layout_frame_rust(&mut eval, frame_id);
    let rows = trace_text_rows(&selected_window_layout_trace(&eval, &engine, frame_id));
    assert!(rows.iter().any(|row| row.contains("TAIL")));
    assert!(rows.iter().all(|row| !row.contains("hidden")));
    let observation = printed_eval_result(
        &mut eval,
        &format!(
            "(prin1-to-string (list (get-text-property {tail} 'fontified) \
          (get-text-property {tail} 'font-lock-face) redisplay-fontify-calls))"
        ),
    );
    assert!(
        observation.starts_with("(t font-lock-warning-face"),
        "actual sparse fontifier must paint the post-fold tail: {observation}"
    );
    (
        frame_observation(&eval, &engine, frame_id),
        observation,
        coverage::counts(),
    )
}

#[test]
fn coverage_keeps_large_folded_tail_fontifiers_and_all_published_faces() {
    let off = folded_observation(false);
    let on = folded_observation(true);
    assert_eq!(
        on.0, off.0,
        "complete folded frame/faces/points/source/ends"
    );
    assert_eq!(
        on.1, off.1,
        "actual ordered sparse fontifier calls/properties"
    );
    assert!(off.2.queries > 0 && on.2.queries > 0);
    assert_eq!(on.2.queries, off.2.queries);
}

#[test]
fn coverage_keeps_seams_replacement_callbacks_and_source_horizon_retries() {
    let text = source();
    let cases = [
        "(insert \"\\n\")",
        "(insert (make-string 100 ?w))",
        "(put-text-property (- (point) 4) (point) 'display \"<REPLACED>\")",
        "(put-text-property (- (point) 4) (point) 'invisible t)",
        "(put-text-property (- (point) 4) (point) 'line-prefix \"P:\")",
        "(progn (setq fontification-functions (list (lambda (_p) (setq prove-first-font-count (1+ prove-first-font-count))))) (delete-region (point) (1+ (point))))",
        r#"(let ((overlay (make-overlay (- (point) 4) (- (point) 4))) (string "O"))
             (put-text-property 0 1 'display
               '(when (progn (setq prove-first-font-count (1+ prove-first-font-count)) t) . "C") string)
             (overlay-put overlay 'before-string string))"#,
    ];
    for form in cases {
        let run = |on| {
            let _coverage = CoverageGuard::set(on);
            let _lazy = SyncGuard::set(true);
            let _budget = SourceBudgetGuard::set(true);
            let _keys = Guard::set(true);
            use crate::buffer_source::window_source::{
                reset_sync_source_budget_horizon_reads_for_test,
                reset_sync_source_budget_retries_for_test,
                sync_source_budget_horizon_reads_for_test, sync_source_budget_retries_for_test,
            };
            reset_sync_source_budget_retries_for_test();
            reset_sync_source_budget_horizon_reads_for_test();
            let observed = run_with_policy(
                false,
                EditSyncMode::Sync,
                true,
                &text,
                line_end(&text, 8),
                &[form],
            );
            (
                observed,
                sync_source_budget_retries_for_test(),
                sync_source_budget_horizon_reads_for_test(),
                coverage::counts(),
            )
        };
        let off = run(false);
        let on = run(true);
        assert_eq!(
            on.0.0, off.0.0,
            "callback/source/query observations: {form}"
        );
        assert_eq!(on.0.1, off.0.1, "actual producer decisions: {form}");
        assert_eq!(on.0.2[0].edit_windows, off.0.2[0].edit_windows);
        assert_eq!(on.0.2[0].reused_rows, off.0.2[0].reused_rows);
        assert_eq!(on.0.2[0].relaid_body_rows, off.0.2[0].relaid_body_rows);
        assert_eq!(on.1, off.1, "literal full-horizon retries: {form}");
        assert_eq!(on.2, off.2, "literal source horizon reads: {form}");
        if form.contains("prove-first-font-count") {
            assert!(
                on.0.0[0]
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .parse::<i64>()
                    .unwrap()
                    > 0,
                "real callback must execute"
            );
        }
        if form.contains("overlay-put") {
            assert_eq!(on.0.2[0].edit_windows, 0, "conditional Lisp refuses reuse");
            assert!(on.0.2[0].full_windows > 0);
            assert_eq!(on.3.completed_sync_installs, 0);
            assert_eq!(
                on.3.shortcuts, 0,
                "conditional refusal stays outside proof scope"
            );
        }
    }
}

#[test]
fn coverage_numeric_scope_restores_unwind_and_independent_mutator_ownership() {
    let initial = (coverage::forced(), coverage::counts(), coverage::still());
    {
        let _outer = CoverageGuard::set(true);
        coverage::note_iterator(true);
        let saved = (coverage::forced(), coverage::counts(), coverage::still());
        let panic = std::panic::catch_unwind(|| {
            let _nested = CoverageGuard::set(false);
            coverage::note_query(true);
            panic!("numeric coverage scope unwind");
        });
        assert!(panic.is_err());
        assert_eq!(
            (coverage::forced(), coverage::counts(), coverage::still()),
            saved
        );
    }
    assert_eq!(
        (coverage::forced(), coverage::counts(), coverage::still()),
        initial
    );
    let shared = std::sync::Arc::new(DisplayPointRows::from_points(points(0, &[1, 2, 3])));
    let results = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for on in [false, true] {
            let rows = shared.clone();
            workers.push(scope.spawn(move || {
                let _guard = CoverageGuard::set(on);
                let text = "a".repeat(100);
                let (eval, _frame, buffer_id, _window) = incr_editing_frame(&text, 800, 600);
                let buffer = eval.buffer_manager().get(buffer_id).unwrap();
                let snapshot = WindowDisplaySnapshot {
                    point_rows: Some((*rows).clone()),
                    ..Default::default()
                };
                let result = VisibleFontificationCoverage::inspect_for_edit_sync(
                    buffer,
                    &snapshot,
                    CharPos0::new(4),
                    true,
                );
                assert_eq!(result, VisibleFontificationCoverage::Complete);
                assert_eq!(coverage::forced(), Some(on));
                (result, snapshot.iter_points().collect::<Vec<_>>())
            }));
        }
        workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        results[0], results[1],
        "only immutable numeric cells cross mutators"
    );
    assert_eq!(
        (coverage::forced(), coverage::counts(), coverage::still()),
        initial
    );
}
