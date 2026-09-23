use super::*;

fn row(index: i64, start: i64, end: i64) -> DisplayRowSnapshot {
    DisplayRowSnapshot {
        row: index,
        start_buffer_pos: Some(LispCharPos1::new(start)),
        end_buffer_pos: Some(LispCharPos1::new(end)),
        ..DisplayRowSnapshot::default()
    }
}

#[test]
fn measurement_probe_is_not_itself_a_viewport_commit() {
    let initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
    let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
        &initial,
        ResolvedWindowStart::from_layout_charpos(1),
        ScrollPolicy::Unlimited,
        0,
    ) else {
        panic!("display uncertainty should request measurement")
    };
    assert_eq!(measurement.probe_window_start(), LayoutCharPos0::new(30));

    let probe = vec![row(0, 30, 39), row(1, 40, 49), row(2, 50, 59)];
    assert_eq!(
        measurement.observe(&probe, LispCharPos1::new(45), false),
        ViewportDecision::Commit {
            window_start: ResolvedWindowStart::from_layout_charpos(20)
        },
        "point is two rows below the old bottom, so policy advances two old rows"
    );
}

#[test]
fn default_policy_places_point_relative_without_a_forward_probe() {
    let initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
    assert_eq!(
        ForwardViewportMeasurement::begin(
            &initial,
            ResolvedWindowStart::from_layout_charpos(1),
            ScrollPolicy::Recenter,
            0,
        ),
        ViewportDecision::PlaceRelativeToPoint {
            lines_above_point: 1,
            fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
        },
        "GNU's default policy skips try_scrolling and measures backward from point"
    );
}

#[test]
fn trailing_non_buffer_row_does_not_hide_the_last_probe_boundary() {
    let mut initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
    initial.push(DisplayRowSnapshot::default());

    let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
        &initial,
        ResolvedWindowStart::from_layout_charpos(1),
        ScrollPolicy::Unlimited,
        0,
    ) else {
        panic!("an EOB/filler row must not discard the preceding measured boundary")
    };

    assert_eq!(measurement.probe_window_start(), LayoutCharPos0::new(30));
    assert_eq!(measurement.rows_before_probe, 3);
    assert_eq!(measurement.viewport_rows, 4);
}

#[test]
fn bounded_forward_probe_switches_to_point_relative_placement() {
    let initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
    let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
        &initial,
        ResolvedWindowStart::from_layout_charpos(1),
        ScrollPolicy::Conservative { max_lines: 2 },
        0,
    ) else {
        panic!("bounded conservative scrolling should measure nearby rows")
    };
    let probe = vec![row(0, 30, 39), row(1, 40, 49), row(2, 50, 59)];
    assert_eq!(
        measurement.observe(&probe, LispCharPos1::new(90), true),
        ViewportDecision::PlaceRelativeToPoint {
            lines_above_point: 1,
            fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
        },
        "once the nearest possible point row exceeds scroll_max, GNU recenters"
    );
}

#[test]
fn measured_point_without_enough_progress_never_accepts_the_probe() {
    let initial = vec![row(0, 1, 10)];
    let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
        &initial,
        ResolvedWindowStart::from_layout_charpos(1),
        ScrollPolicy::Unlimited,
        0,
    ) else {
        panic!("display uncertainty should request measurement")
    };
    let probe = vec![
        row(0, 10, 10),
        row(1, 10, 10),
        row(2, 10, 10),
        row(3, 10, 10),
        row(4, 10, 99),
    ];

    assert_eq!(
        measurement.observe(&probe, LispCharPos1::new(99), false),
        ViewportDecision::PlaceRelativeToPoint {
            lines_above_point: 0,
            fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
        },
        "a transient probe must not become the presentation when its rows cannot supply a policy-approved start"
    );
}

fn probe_measurement() -> ForwardViewportMeasurement {
    let initial = vec![row(0, 1, 10), row(1, 11, 20), row(2, 21, 30)];
    let ViewportDecision::NeedMoreMeasurement(measurement) = ForwardViewportMeasurement::begin(
        &initial,
        ResolvedWindowStart::from_layout_charpos(1),
        ScrollPolicy::Conservative { max_lines: 30 },
        0,
    ) else {
        panic!("a bounded conservative policy measures forward first")
    };
    measurement
}

#[test]
fn budget_left_keeps_the_producer_decision() {
    let measurement = probe_measurement();
    assert_eq!(
        ViewportDecision::NeedMoreMeasurement(measurement.clone())
            .resolve_with_budget(1, ViewportAttemptStart::Semantic(1)),
        Some(ViewportDecision::NeedMoreMeasurement(measurement))
    );
    let placement = ViewportDecision::PlaceRelativeToPoint {
        lines_above_point: 0,
        fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
    };
    assert_eq!(
        placement
            .clone()
            .resolve_with_budget(1, ViewportAttemptStart::MeasurementProbe { origin: 1 }),
        Some(placement)
    );
}

#[test]
fn spent_budget_ends_the_leaf_at_a_semantic_start() {
    // The hang behind "SPC SPC after a resize": with no retries left the
    // producer kept asking for a placement that resolved to the start it
    // already had. GNU reaches `done:` instead (xdisp.c:21384-21398).
    let measurement = probe_measurement();
    assert_eq!(
        ViewportDecision::NeedMoreMeasurement(measurement)
            .resolve_with_budget(0, ViewportAttemptStart::Semantic(1)),
        None
    );
    assert_eq!(
        ViewportDecision::PlaceRelativeToPoint {
            lines_above_point: 0,
            fallback_window_start: ResolvedWindowStart::from_layout_charpos(1),
        }
        .resolve_with_budget(0, ViewportAttemptStart::Semantic(1)),
        None
    );
}

#[test]
fn spent_budget_never_publishes_a_measurement_probe() {
    let measurement = probe_measurement();
    let fallback = measurement.fallback_placement();
    assert!(matches!(
        fallback,
        ViewportDecision::PlaceRelativeToPoint { .. }
    ));
    assert_eq!(
        ViewportDecision::NeedMoreMeasurement(measurement)
            .resolve_with_budget(0, ViewportAttemptStart::MeasurementProbe { origin: 1 }),
        Some(fallback.clone())
    );
    assert_eq!(
        fallback
            .clone()
            .resolve_with_budget(0, ViewportAttemptStart::MeasurementProbe { origin: 1 }),
        Some(fallback)
    );
}

#[test]
fn settled_decisions_need_no_retry_at_any_budget() {
    for budget in [0, 1] {
        for start in [
            ViewportAttemptStart::Semantic(1),
            ViewportAttemptStart::MeasurementProbe { origin: 1 },
        ] {
            assert_eq!(
                ViewportDecision::Keep.resolve_with_budget(budget, start),
                None
            );
            assert_eq!(
                ViewportDecision::Commit {
                    window_start: ResolvedWindowStart::from_layout_charpos(1)
                }
                .resolve_with_budget(budget, start),
                None
            );
        }
    }
}
