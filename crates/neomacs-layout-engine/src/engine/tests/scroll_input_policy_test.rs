use super::*;

#[test]
fn x11_scroll_factor_is_published_with_each_accepted_viewport() {
    let (mut eval, frame, _, _) = incr_editing_frame("alpha\nbeta\n", 800, 240);
    realize_test_gui_frame(&mut eval, frame);
    let mut engine = LayoutEngine::new();
    engine.enable_cosmic_metrics();
    for (value, expected) in [("0.25", 0.25), ("2", 2.0), ("nil", 1.0)] {
        eval.eval_str(&format!("(setq x-scroll-event-delta-factor {value})"))
            .unwrap();
        let FrameLayoutAttempt::Prepared(output) = engine.redisplay_frame_attempt(&mut eval, frame)
        else {
            panic!("viewport must prepare")
        };
        assert_eq!(output.scroll_input_policy.x11_delta_factor, expected);
        assert_eq!(
            output.materialize().scroll_input_policy.x11_delta_factor,
            expected
        );
    }
}
