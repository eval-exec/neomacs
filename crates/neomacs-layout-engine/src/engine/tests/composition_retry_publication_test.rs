//! A folded region can carry automatic compositions beyond the first resolve range.
use super::*;

#[test]
#[tracing_test::traced_test]
fn invisible_composition_retry_publishes_each_window_once() {
    let (mut eval, frame_id, _, selected) = incr_editing_frame("", 800, 480);
    realize_test_gui_frame(&mut eval, frame_id);
    crate::test_composition::install_rules(&mut eval);
    eval.eval_str(
        "(progn
           ;; The minimal evaluator has not loaded the GUI Lisp defaults.
           (setq buffer-invisibility-spec t)
           (let* ((cols (max 1 (ceiling (window-body-width nil t) (frame-char-width))))
                  (rows (max 1 (ceiling (window-body-height nil t) (frame-char-height))))
                  (screenful (* rows cols))
                  (span-at (+ screenful (/ screenful 2)))
                  (hidden-start (point)))
             (insert \"visible lead\\n\")
             (insert (make-string span-at ?a) \"☁️\"
                     (make-string (- (* 2 screenful) span-at 2) ?a))
             (put-text-property hidden-start (point) 'invisible t)
             (insert \"visible tail\\n\")
             (goto-char (point-min))))",
    )
    .expect("issue #830 folded buffer");
    let mut engine = crate::font::metrics::test_catalog::layout_engine();
    crate::font::metrics::test_catalog::enable_metrics(&mut engine);
    for _ in 0..2 {
        let FrameLayoutAttempt::Prepared(output) =
            engine.redisplay_frame_attempt(&mut eval, frame_id)
        else {
            panic!("folded automatic composition must prepare a presentation");
        };
        let ids: Vec<_> = output
            .window_matrices
            .iter()
            .map(|entry| neovm_core::window::WindowId(entry.window_id.get() as u64))
            .collect();
        let unique: std::collections::HashSet<_> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "one matrix per published window");
        assert!(unique.contains(&selected));
        // Preparing and activating exercise PresentationGeometry's duplicate-id
        // check and publish the snapshots consumed by position queries.
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("frame")
            .activate_display_presentation(neovm_core::window::geometry::PresentationId::new(
                output.presentation_id.get(),
            ))
            .expect("activate prepared folded-buffer presentation");
        let frame = eval.frame_manager().get(frame_id).expect("frame");
        for id in ids {
            assert_eq!(
                frame
                    .redisplay_snapshot(id)
                    .expect("published window")
                    .window_id,
                id
            );
        }
    }
    assert!(
        logs_contain("automatic composition coverage retry"),
        "the folded span must exercise coverage retry before publishing"
    );
}
