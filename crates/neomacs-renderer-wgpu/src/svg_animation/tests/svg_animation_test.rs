//! Engine tests: compile → evaluate → patch → sample.

#[path = "xml_test.rs"]
mod xml;

use super::eval;
use super::patch;
use super::plan;
use super::sampler;
use neomacs_display_protocol::animated_visual::AnimatedVisual;
use neomacs_display_protocol::{ImageAnimationPolicy, ImageColorContext};
use std::time::Duration;

/// A spinner-class document: one parent-targeted rule, numeric values,
/// indefinite repeat.
const PULSING_CIRCLE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100">
  <circle id="dot" cx="50" cy="50" r="10" fill="tomato">
    <animate attributeName="r" values="10;40;10" dur="2s" repeatCount="indefinite"/>
  </circle>
</svg>"##;

/// One rule targeting another element by `href`, animating an absent
/// attribute so insertion is exercised.
const HREF_OPACITY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <rect id="box" x="0" y="0" width="10" height="10" fill="blue"/>
  <animate href="#box" attributeName="opacity" from="1" to="0" dur="1s" repeatCount="indefinite"/>
</svg>"##;

fn compile(source: &str) -> plan::AnimationPlan {
    let compiled = plan::compile(source.as_bytes()).expect("plan compiles");
    assert!(!compiled.is_empty(), "plan has rules");
    compiled
}

/// The single override active at `doc_time`, as text.
fn one_override(animation: &plan::AnimationPlan, doc_time: Duration) -> (String, String) {
    let overrides = eval::evaluate(animation, doc_time);
    assert_eq!(overrides.len(), 1, "exactly one override");
    (
        animation.rules[overrides[0].rule].site.attribute.clone(),
        overrides[0].value.clone(),
    )
}

#[test]
fn plan_reads_timing_values_and_site() {
    let animation = compile(PULSING_CIRCLE);
    assert_eq!(animation.rules.len(), 1);
    let rule = &animation.rules[0];
    assert_eq!(rule.site.attribute, "r");
    // The site targets the existing value: `r="10"`.
    assert!(rule.site.value_range.is_some());
    let timeline = &rule.timeline;
    assert_eq!(timeline.dur, Duration::from_secs(2));
    assert_eq!(timeline.begin, Duration::ZERO);
    assert!(matches!(timeline.repeat, plan::Repeat::Indefinite));
    assert_eq!(
        timeline.values,
        vec![
            plan::AnimatedValue::Numbers(vec![10.0]),
            plan::AnimatedValue::Numbers(vec![40.0]),
            plan::AnimatedValue::Numbers(vec![10.0]),
        ]
    );
}

#[test]
fn plan_answers_the_scheduler_questions() {
    let animation = compile(PULSING_CIRCLE);
    assert_eq!(
        AnimatedVisual::period(&animation),
        Some(Duration::from_secs(2))
    );
    assert!(AnimatedVisual::is_continuous(&animation));
    // Uniform thirds put a keyframe boundary at the half-second marks.
    assert_eq!(
        AnimatedVisual::next_event(&animation, Duration::ZERO),
        Some(Duration::from_millis(1000))
    );
    assert_eq!(
        AnimatedVisual::next_event(&animation, Duration::from_millis(1500)),
        Some(Duration::from_secs(2))
    );
}

#[test]
fn href_target_and_absent_attribute_compile() {
    let animation = compile(HREF_OPACITY);
    let rule = &animation.rules[0];
    assert_eq!(rule.site.attribute, "opacity");
    // `opacity` is absent on the rect: the site is an insertion, not a range.
    assert!(rule.site.value_range.is_none());
    assert!(rule.site.insert_pos > 0);
}

#[test]
fn unsupported_constructs_drop_their_rules() {
    let cases = [
        // Motion paths are outside the subset.
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle r="1"><animateMotion dur="2s" repeatCount="indefinite"/></circle></svg>"##,
        // Event-based begin has no document-only meaning.
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle r="1"><animate attributeName="r" from="1" to="2" dur="2s" begin="click"/></circle></svg>"##,
        // Additive composition needs base values at evaluation time.
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle r="1"><animate attributeName="r" from="1" to="2" dur="2s" additive="sum"/></circle></svg>"##,
        // Unknown transform kinds cannot round-trip.
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle r="1"><animateTransform attributeName="transform" type="warp" from="0" to="1" dur="2s"/></circle></svg>"##,
    ];
    for case in cases {
        let compiled = plan::compile(case.as_bytes()).expect("document parses");
        assert!(compiled.is_empty(), "rule dropped for: {case}");
    }
}

#[test]
fn animate_motion_prefix_still_scans_but_never_compiles() {
    assert!(super::may_contain_animation(b"<svg><animateMotion/></svg>"));
    assert!(super::may_contain_animation(b"<svg><set/></svg>"));
    assert!(!super::may_contain_animation(b"<svg><circle/></svg>"));
}

#[test]
fn linear_interpolation_hits_keyframes_and_midpoints() {
    let animation = compile(PULSING_CIRCLE);
    // Keyframe values hold at their times: t=0 → 10, t=1s → 40.
    assert_eq!(
        one_override(&animation, Duration::ZERO),
        ("r".into(), "10".into())
    );
    assert_eq!(
        one_override(&animation, Duration::from_secs(1)),
        ("r".into(), "40".into())
    );
    // Quarter into the second segment: 40 → 10 at half progress is 25.
    assert_eq!(
        one_override(&animation, Duration::from_millis(1500)),
        ("r".into(), "25".into())
    );
    // The loop wraps: 2.75s is 0.75s into the cycle (fraction 0.375),
    // three quarters through the first segment: 10 + 30 x 0.75.
    assert_eq!(
        one_override(&animation, Duration::from_millis(2750)),
        ("r".into(), "32.5".into())
    );
}

#[test]
fn discrete_values_step() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <rect width="4" height="4"><animate attributeName="fill" values="red;green;blue" dur="3s" calcMode="discrete" repeatCount="indefinite"/></rect>
</svg>"##;
    let animation = compile(source);
    // Colors are opaque values: they step, never blend.
    assert_eq!(
        one_override(&animation, Duration::ZERO),
        ("fill".into(), "red".into())
    );
    assert_eq!(
        one_override(&animation, Duration::from_secs(1)),
        ("fill".into(), "green".into())
    );
    assert_eq!(
        one_override(&animation, Duration::from_millis(2500)),
        ("fill".into(), "blue".into())
    );
}

#[test]
fn begin_offset_holds_the_base_before_activation() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle r="1"><animate attributeName="r" from="1" to="5" begin="1s" dur="2s" repeatCount="indefinite"/></circle>
</svg>"##;
    let animation = compile(source);
    assert!(eval::evaluate(&animation, Duration::from_millis(999)).is_empty());
    assert_eq!(
        one_override(&animation, Duration::from_secs(2)),
        ("r".into(), "3".into())
    );
}

#[test]
fn finite_rules_freeze_or_remove_after_their_end() {
    let frozen = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle r="1"><animate attributeName="r" from="1" to="5" dur="1s" fill="freeze"/></circle>
</svg>"##;
    let animation = compile(frozen);
    assert_eq!(
        one_override(&animation, Duration::from_secs(10)),
        ("r".into(), "5".into())
    );

    let removed = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle r="1"><animate attributeName="r" from="1" to="5" dur="1s"/></circle>
</svg>"##;
    let animation = compile(removed);
    assert!(eval::evaluate(&animation, Duration::from_secs(10)).is_empty());
}

#[test]
fn colors_interpolate_channel_wise() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <rect fill="#000000"><animate attributeName="fill" values="#000000;#ffffff" dur="1s" repeatCount="indefinite"/></rect>
</svg>"##;
    let animation = compile(source);
    assert_eq!(
        one_override(&animation, Duration::from_millis(500)),
        ("fill".into(), "#808080".into())
    );
}

#[test]
fn transforms_serialize_with_their_function() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100">
  <g><animateTransform attributeName="transform" type="rotate" values="0 50 50;360 50 50" dur="2s" repeatCount="indefinite"/></g>
</svg>"##;
    let animation = compile(source);
    assert_eq!(
        one_override(&animation, Duration::from_secs(1)),
        ("transform".into(), "rotate(180 50 50)".into())
    );
}

#[test]
fn key_times_reshape_segments() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle r="1"><animate attributeName="r" values="0;10;20" keyTimes="0;0.25;1" dur="4s" repeatCount="indefinite"/></circle>
</svg>"##;
    let animation = compile(source);
    // Halfway through the first (quarter-length) segment: 0 → 10 at 0.5.
    assert_eq!(
        one_override(&animation, Duration::from_millis(500)),
        ("r".into(), "5".into())
    );
    // The last segment spans 75% of the cycle: at 2s, 10 -> 20 at 1/3.
    let (_, value) = one_override(&animation, Duration::from_secs(2));
    let radius: f64 = value.parse().expect("numeric");
    assert!(
        (radius - (10.0 + 10.0 / 3.0)).abs() < 1e-9,
        "radius: {radius}"
    );
}

#[test]
fn patch_replaces_present_attributes_in_place() {
    let animation = compile(PULSING_CIRCLE);
    let overrides = eval::evaluate(&animation, Duration::from_millis(1500));
    let patched = patch::apply(&animation, PULSING_CIRCLE.as_bytes(), &overrides).expect("patch");
    let text = String::from_utf8(patched).expect("utf-8");
    assert!(text.contains("r=\"25\""), "patched text: {text}");
    // Everything else about the document is untouched.
    assert!(text.contains("cx=\"50\""), "patched text: {text}");
    assert!(text.contains("values=\"10;40;10\""), "patched text: {text}");
}

#[test]
fn patch_inserts_absent_attributes() {
    let animation = compile(HREF_OPACITY);
    let overrides = eval::evaluate(&animation, Duration::from_millis(500));
    assert_eq!(overrides.len(), 1);
    let patched = patch::apply(&animation, HREF_OPACITY.as_bytes(), &overrides).expect("patch");
    let text = String::from_utf8(patched).expect("utf-8");
    assert!(text.contains("opacity=\"0.5\""), "patched text: {text}");
    // Inserted into the rect's start tag, not the root's.
    let rect = text
        .split('<')
        .find(|tag| tag.starts_with("rect"))
        .expect("rect");
    assert!(rect.contains("opacity="), "rect tag: {rect}");
}

#[test]
fn sampling_disabled_policy_stays_static() {
    assert!(
        sampler::sample(
            PULSING_CIRCLE.as_bytes(),
            ImageColorContext::default(),
            &super::super::svg::SvgResourceContext::Isolated,
            ImageAnimationPolicy::disabled(),
        )
        .is_none()
    );
}

#[test]
fn sampling_static_document_has_no_frames() {
    let static_document = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><circle cx="5" cy="5" r="4"/></svg>"##;
    assert!(
        sampler::sample(
            static_document.as_bytes(),
            ImageColorContext::default(),
            &super::super::svg::SvgResourceContext::Isolated,
            ImageAnimationPolicy::enabled(None),
        )
        .is_none()
    );
}

#[test]
fn sampling_quantizes_a_loop_into_distinct_frames() {
    // fps 4 over the 2s loop: eight 250ms slots.
    let animation = sampler::sample(
        PULSING_CIRCLE.as_bytes(),
        ImageColorContext::default(),
        &super::super::svg::SvgResourceContext::Isolated,
        ImageAnimationPolicy::enabled(Some(4)),
    )
    .expect("sampled animation");
    assert_eq!(animation.frames.len(), 8);
    assert_eq!(animation.frames[0].delay.seconds(), Some(0.25));
    for frame in &animation.frames {
        assert_eq!((frame.width, frame.height), (100, 100));
        assert_eq!(frame.rgba.len(), 100 * 100 * 4);
    }
    // Slot zero is the t=0 state; slot one has grown the circle.
    assert_ne!(animation.frames[0].rgba, animation.frames[1].rgba);
    // The loop is symmetric: slot 1 (r=17.5 at 0.25 progress) and slot 7
    // (r=17.5 at 0.75 progress... of the mirrored segment) both differ from
    // slot 0 and from the extreme slot.
    assert_ne!(animation.frames[1].rgba, animation.frames[2].rgba);
}

/// PR review round 3: two absent-attribute animations on different
/// elements must both survive compilation — the dedup key is the whole
/// site, and the insertion position is what identifies the element.
#[test]
fn dedup_keeps_same_attribute_rules_on_distinct_elements() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle cx="3" cy="5" r="1"><animate attributeName="opacity" from="1" to="0" dur="1s" repeatCount="indefinite"/></circle>
  <circle cx="7" cy="5" r="1"><animate attributeName="opacity" from="0" to="1" dur="1s" repeatCount="indefinite"/></circle>
</svg>"##;
    let animation = compile(source);
    assert_eq!(animation.rules.len(), 2, "both dots animate");
    let positions: Vec<usize> = animation
        .rules
        .iter()
        .map(|rule| rule.site.insert_pos)
        .collect();
    assert_ne!(
        positions[0], positions[1],
        "sites target different elements"
    );
}

/// PR review round 3: `begin` is part of the timing, not an invisible
/// offset — a delayed finite rule's animation must fall inside the
/// sampled loop, and a looping plan's grid starts at its steady state.
#[test]
fn loop_period_and_origin_respect_begin() {
    let delayed_finite = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle r="1"><animate attributeName="r" from="1" to="5" begin="1s" dur="1s" repeatCount="indefinite"/></circle>
</svg>"##;
    let animation = compile(delayed_finite);
    // Looping plan: the grid is the steady cycle, offset past the intro.
    assert_eq!(
        AnimatedVisual::period(&animation),
        Some(Duration::from_secs(1))
    );
    assert_eq!(animation.intro_end(), Duration::from_secs(1));
    // At document time 0.5s — inside the intro — the base value shows.
    assert!(eval::evaluate(&animation, Duration::from_millis(500)).is_empty());
    // At 1.5s the rule is mid-cycle.
    assert_eq!(
        one_override(&animation, Duration::from_millis(1500)),
        ("r".into(), "3".into())
    );

    let finite_only = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle r="1"><animate attributeName="r" from="1" to="5" begin="1s" dur="1s"/></circle>
</svg>"##;
    let animation = compile(finite_only);
    // Finite plan: begin + active is the replayed span.
    assert_eq!(animation.loop_period(), Some(Duration::from_secs(2)));
}

/// PR review round 3: a scheduler must not be woken by keyframe
/// boundaries of cycles that precede the rule's activation.
#[test]
fn next_event_never_precedes_begin() {
    let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10">
  <circle r="1"><animate attributeName="r" from="1" to="5" begin="10s" dur="1s" repeatCount="indefinite"/></circle>
</svg>"##;
    let animation = compile(source);
    assert_eq!(
        AnimatedVisual::next_event(&animation, Duration::ZERO),
        Some(Duration::from_secs(10))
    );
}

#[path = "prefix_test.rs"]
mod prefix;
#[path = "timing_test.rs"]
mod timing;
