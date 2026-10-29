//! Completed producer controls use the same immutable numeric proof as Sync.
//! Each fixture owns its evaluator and rows; guards restore numeric witnesses
//! without changing process policy or sharing Lisp state across mutators.
use super::*;

#[derive(Clone, Copy, Debug)]
enum CompletedPath {
    Full,
    Cursor,
    Scroll,
}

fn completed_observation(
    path: CompletedPath,
    enabled: bool,
) -> (FrameObservation, String, LayoutStats, coverage::Counts) {
    const LINE: &str = "(defun f (a b) (+ a b))\n";
    let text = LINE.repeat(200);
    let (mut eval, frame, buffer, window) = incr_editing_frame(&text, 800, 600);
    prepare_faces(&mut eval, &text);
    eval.eval_str(
        "(progn (setq completed-fontify-calls nil) \
         (setq fontification-functions (list (lambda (start) \
           (setq completed-fontify-calls (cons start completed-fontify-calls)) \
           (put-text-property start (point-max) 'fontified t)))))",
    )
    .unwrap();
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    match path {
        CompletedPath::Full => {}
        CompletedPath::Cursor | CompletedPath::Scroll => {
            let _reference = CoverageGuard::set(false);
            engine.layout_frame_rust(&mut eval, frame);
            activate_last_engine_presentation(&mut eval, &engine, frame);
            match path {
                CompletedPath::Cursor => {
                    eval.eval_str("(goto-char (+ (point) 1))").unwrap();
                }
                CompletedPath::Scroll => scroll_window_to(
                    &mut eval,
                    frame,
                    window,
                    buffer,
                    (6 * LINE.len()) as i64 + 1,
                    14 * LINE.len(),
                ),
                CompletedPath::Full => unreachable!("matched replay path"),
            }
        }
    }
    let _coverage = CoverageGuard::set(enabled);
    engine.layout_frame_rust(&mut eval, frame);
    let counts = coverage::counts();
    let stats = engine.last_layout_stats().clone();
    activate_last_engine_presentation(&mut eval, &engine, frame);
    let observation = frame_observation(&eval, &engine, frame);
    let callbacks = printed_eval_result(
        &mut eval,
        "(prin1-to-string (list completed-fontify-calls prove-first-mode-count \
         (get-text-property 1 'fontified) (point) (window-start) (window-end)))",
    );
    (observation, callbacks, stats, counts)
}

#[test]
fn completed_full_cursor_and_scroll_elide_only_covered_iterator_work() {
    for path in [
        CompletedPath::Full,
        CompletedPath::Cursor,
        CompletedPath::Scroll,
    ] {
        let off = completed_observation(path, false);
        let on = completed_observation(path, true);
        assert_eq!(on.0, off.0, "{path:?}: all glyphs/faces/positions/ends");
        assert_eq!(on.1, off.1, "{path:?}: fontifier and mode-line callbacks");
        match path {
            CompletedPath::Full => assert!(on.2.full_windows > 0, "{:?}", on.2),
            CompletedPath::Cursor => assert_eq!(on.2.cursor_only_windows, 1, "{:?}", on.2),
            CompletedPath::Scroll => assert_eq!(on.2.scroll_windows, 1, "{:?}", on.2),
        }
        assert_eq!(
            off.3.sync_iterators, 0,
            "{path:?}: not an edit-Sync attempt"
        );
        assert!(
            off.3.iterators > 0 && off.3.points > 0,
            "{path:?}: {:?}",
            off.3
        );
        assert_eq!(
            on.3.queries, off.3.queries,
            "{path:?}: every real query remains"
        );
        assert!(on.3.shortcuts > 0, "{path:?}: {:?}", on.3);
        assert!(
            on.3.points < off.3.points,
            "{path:?}: {:?} -> {:?}",
            off.3,
            on.3
        );
    }
}

#[test]
fn completed_coverage_preserves_sparse_boundaries_and_conservative_fallbacks() {
    let text = "a".repeat(100);
    let (mut eval, _, buffer, _) = incr_editing_frame(&text, 800, 600);
    eval.eval_str("(put-text-property 6 7 'fontified t)")
        .unwrap();
    let buffer = eval.buffer_manager().get(buffer).unwrap();
    let mut unknown = snapshot(&[], &[]);
    unknown.set_points(points(0, &[5, 6, 101]));
    let cases = [
        (snapshot(&[&[1, 4], &[101, 102]], &[]), 4, true),
        (snapshot(&[&[1, 5, 6], &[90, 101]], &[]), 4, false),
        (unknown, 4, false),
        (snapshot(&[&[-1, 0, 1]], &[]), 0, false),
        (snapshot(&[&[1, 3], &[2, 4]], &[]), 4, false),
        (snapshot(&[&[3, 4], &[1, 2]], &[]), 4, false),
    ];
    for (snapshot, prepass, shortcut) in cases {
        let inspect = |enabled| {
            let _guard = CoverageGuard::set(enabled);
            let result = VisibleFontificationCoverage::inspect_completed(
                buffer,
                &snapshot,
                CharPos0::new(prepass),
            );
            (result, coverage::counts())
        };
        let off = inspect(false);
        let on = inspect(true);
        assert_eq!(
            on.0, off.0,
            "complete sparse plan, preserving order and merging"
        );
        assert_eq!(on.1.queries, off.1.queries);
        if shortcut {
            assert_eq!(on.1.iterators, 0);
            assert_eq!(on.1.shortcuts, 1);
        } else {
            assert_eq!(on.1.iterators, off.1.iterators);
            assert_eq!(on.1.points, off.1.points);
            assert_eq!(on.1.shortcuts, 0);
        }
    }
}
