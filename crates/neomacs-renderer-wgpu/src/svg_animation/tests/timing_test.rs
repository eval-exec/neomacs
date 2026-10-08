use super::*;

fn document(rule: &str) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" opacity="0.2" fill="red">{rule}</rect></svg>"#
    )
}

#[test]
fn overlapping_rules_follow_activation_priority_and_reveal_underlying_animation() {
    let source = document(
        r#"<set attributeName="fill" to="red" dur="4s"/><set attributeName="fill" to="blue" begin="1s" dur="1s"/>"#,
    );
    let animation = compile(&source);
    for (millis, expected) in [(500, "red"), (1500, "blue"), (2500, "red")] {
        assert_eq!(
            one_override(&animation, Duration::from_millis(millis)).1,
            expected
        );
    }
}

#[test]
fn later_begin_takes_priority_over_later_document_order() {
    let source = document(
        r#"<set attributeName="fill" to="blue" begin="1s" dur="3s"/><set attributeName="fill" to="red" dur="4s"/>"#,
    );
    assert_eq!(
        one_override(&compile(&source), Duration::from_secs(2)).1,
        "blue"
    );
}

#[test]
fn fractional_repeats_freeze_at_the_partial_cycle_value() {
    let source = document(
        r#"<animate attributeName="opacity" from="0" to="1" dur="4s" repeatCount="2.5" fill="freeze"/>"#,
    );
    let animation = compile(&source);
    assert_eq!(one_override(&animation, Duration::from_secs(10)).1, "0.5");
    assert_eq!(animation.loop_period(), Some(Duration::from_secs(10)));
}

#[test]
fn repeated_key_times_skip_expired_zero_width_segments() {
    let source = document(
        r#"<animate attributeName="opacity" values="0;10;20;30" keyTimes="0;0.5;0.5;1" dur="4s"/>"#,
    );
    assert_eq!(
        one_override(&compile(&source), Duration::from_secs(3)).1,
        "25"
    );
}

#[test]
fn discrete_key_times_need_not_end_at_one() {
    let source = document(
        r#"<animate attributeName="fill" values="red;green;blue" calcMode="discrete" keyTimes="0;0.2;0.8" dur="1s"/>"#,
    );
    assert_eq!(
        one_override(&compile(&source), Duration::from_millis(250)).1,
        "green"
    );
}

#[test]
fn scheduler_reports_discrete_and_terminal_boundaries() {
    let source = document(
        r#"<animate attributeName="fill" values="red;green;blue" calcMode="discrete" dur="3s"/>"#,
    );
    let animation = compile(&source);
    assert_eq!(
        animation.next_event(Duration::ZERO),
        Some(Duration::from_secs(1))
    );
    assert_eq!(
        animation.next_event(Duration::from_secs(2)),
        Some(Duration::from_secs(3))
    );
}

#[test]
fn composite_period_is_exact_and_skips_finished_finite_effects() {
    let source = document(
        r#"<animate attributeName="opacity" from="0" to="1" dur="2s" repeatCount="indefinite"/><animate attributeName="x" from="0" to="1" dur="3s" repeatCount="indefinite"/><set attributeName="fill" to="red" dur="5s"/>"#,
    );
    let animation = compile(&source);
    assert_eq!(animation.period(), Some(Duration::from_secs(6)));
    assert_eq!(animation.intro_end(), Duration::from_secs(5));
}

#[test]
fn finite_sampling_includes_freeze_and_remove_terminal_pixels() {
    for fill in ["freeze", "remove"] {
        let source = document(&format!(
            r#"<animate attributeName="opacity" from="0" to="1" dur="1s" fill="{fill}"/>"#
        ));
        let sampled = sampler::sample(
            source.as_bytes(),
            ImageColorContext::default(),
            &crate::svg::SvgResourceContext::Isolated,
            ImageAnimationPolicy::enabled(Some(2)),
        )
        .unwrap();
        let opacity = if fill == "freeze" { "1" } else { "0.2" };
        let terminal = format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><rect width="10" height="10" fill="red" opacity="{opacity}"/></svg>"#
        );
        let expected = crate::svg::decode(
            terminal.as_bytes(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            crate::svg::SvgResourceContext::Isolated,
        )
        .unwrap();
        assert_eq!(sampled.frames.last().unwrap().rgba, expected.rgba, "{fill}");
    }
}

#[test]
fn discrete_freeze_holds_value_just_before_active_end() {
    let source = document(
        r#"<animate attributeName="fill" values="red;blue" calcMode="discrete" keyTimes="0;1" dur="1s" fill="freeze"/>"#,
    );
    assert_eq!(
        one_override(&compile(&source), Duration::from_secs(1)).1,
        "red"
    );
}

#[test]
fn finite_sample_count_includes_terminal_within_cap() {
    let source =
        document(r#"<animate attributeName="opacity" from="0" to="1" dur="20s" fill="freeze"/>"#);
    let sampled = sampler::sample(
        source.as_bytes(),
        ImageColorContext::default(),
        &crate::svg::SvgResourceContext::Isolated,
        ImageAnimationPolicy::enabled(Some(30)),
    )
    .unwrap();
    assert_eq!(sampled.frames.len(), 256);
    assert!((sampled.frames[0].delay.seconds().unwrap() - 20.0 / 255.0).abs() < 1e-9);
}

#[test]
fn linear_freeze_reaches_final_value_with_equal_terminal_key_times() {
    let source = document(
        r#"<animate attributeName="opacity" values="0;0.5;1" keyTimes="0;1;1" dur="1s" fill="freeze"/>"#,
    );
    assert_eq!(
        one_override(&compile(&source), Duration::from_secs(1)).1,
        "1"
    );
}

#[test]
fn foreign_namespace_elements_do_not_contribute_svg_animations() {
    let source = document(
        r#"<foreign:animate xmlns:foreign="urn:foreign" attributeName="opacity" from="0" to="1" dur="1s" repeatCount="indefinite"/>"#,
    );
    let compiled = plan::compile(source.as_bytes()).unwrap();
    assert!(
        compiled.is_empty(),
        "foreign XML elements are not SVG animations"
    );
}
