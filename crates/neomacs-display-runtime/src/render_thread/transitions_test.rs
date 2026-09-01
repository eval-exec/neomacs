use super::*;
use neomacs_display_protocol::frame_glyphs::{
    BufferTransitionTarget, ContentTransitionHint, PresentedWindowRegions,
};
use neomacs_display_protocol::{ContentTransitionIntent, Rect, TransitionEasing, TransitionEffect};
use std::time::Duration;

#[test]
fn default_transition_state_has_expected_policy_defaults() {
    let ts = TransitionState::default();
    // BOTH transitions are off by default (stock-Emacs-like instant repaint);
    // opt in via `(neomacs-effect-set 'buffer-transition :enabled t)` or
    // `(neomacs-effect-set 'scroll-transition :enabled t)`.  The
    // effect/duration/easing asserted below are the values used once enabled.
    assert!(!ts.policy.buffer.enabled);
    assert!(!ts.policy.scroll.enabled);
    assert_eq!(ts.policy.buffer.duration, Duration::from_millis(200));
    assert_eq!(ts.policy.scroll.duration, Duration::from_millis(150));
    assert_eq!(ts.policy.buffer.effect, TransitionEffect::Slide);
    assert_eq!(ts.policy.scroll.effect, TransitionEffect::Slide);
}

#[test]
fn visual_config_is_the_transition_policy_source_of_truth() {
    let mut config = neomacs_display_protocol::VisualConfig::default();
    config.buffer_transition.enabled = false;
    config.scroll_transition.duration = Duration::from_millis(425);
    config.scroll_transition.effect = TransitionEffect::PageCurl;
    config.scroll_transition.easing = TransitionEasing::Spring;

    let policy = TransitionPolicy::from(&config);

    assert!(!policy.buffer.enabled);
    assert_eq!(policy.scroll.duration, Duration::from_millis(425));
    assert_eq!(policy.scroll.effect, TransitionEffect::PageCurl);
    assert_eq!(policy.scroll.easing, TransitionEasing::Spring);
}

#[test]
fn default_transition_state_starts_without_active_transitions() {
    let ts = TransitionState::default();
    assert!(ts.offscreen_a.is_none());
    assert!(ts.offscreen_b.is_none());
    assert!(ts.current_is_a);
    assert!(!ts.has_active());
}

#[test]
fn frame_buffer_hint_plans_one_synchronized_group_for_each_viewport() {
    let left = PresentedWindowRegions {
        text_body: Rect::new(0.0, 18.0, 200.0, 118.0),
        ..PresentedWindowRegions::default()
    }
    .buffer_viewport()
    .unwrap();
    let right = PresentedWindowRegions {
        text_body: Rect::new(200.0, 18.0, 200.0, 118.0),
        ..PresentedWindowRegions::default()
    }
    .buffer_viewport()
    .unwrap();
    let hint = ContentTransitionHint::BufferReplaced {
        target: BufferTransitionTarget::Frame {
            regions: vec![left, right],
        },
        intent: ContentTransitionIntent::Replace,
    };

    // Buffer transitions are opt-in; this test exercises the planner.
    let mut policy = TransitionPolicy::default();
    policy.buffer.enabled = true;
    let planned =
        plan_transition_hint(&policy, &hint).expect("buffer transitions were enabled above");

    assert_eq!(planned.key, TransitionKey::Frame);
    assert_eq!(planned.source, TransitionSource::Buffer);
    assert_eq!(planned.plan.region_count(), 2);
    assert_eq!(
        planned
            .plan
            .regions()
            .map(|region| region.bounds)
            .collect::<Vec<_>>(),
        vec![left.bounds(), right.bounds()]
    );
}

#[test]
fn synchronized_transition_plan_rejects_empty_or_inconsistent_region_clocks() {
    assert!(SynchronizedTransitionPlan::try_from_plans([]).is_none());

    // Buffer transitions are opt-in; this test exercises the planner.
    let mut policy = TransitionPolicy::default();
    policy.buffer.enabled = true;
    let first = policy
        .buffer_plan(
            Rect::new(0.0, 0.0, 200.0, 100.0),
            ContentTransitionIntent::Replace,
        )
        .unwrap();
    let mut inconsistent = first;
    inconsistent.bounds.x = 200.0;
    inconsistent.duration += Duration::from_millis(1);

    assert!(
        SynchronizedTransitionPlan::try_from_plans([first, inconsistent]).is_none(),
        "one group cannot encode per-region clocks"
    );
}
