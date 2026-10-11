use super::*;
fn viewport(start: usize) -> Vec<ScrollViewport> {
    vec![ScrollViewport {
        window: 1,
        buffer: 1,
        start,
        hscroll: 0,
        vscroll: 0,
    }]
}
fn time(nanoseconds: u64) -> PlatformTimestamp {
    PlatformTimestamp {
        clock_id: 1,
        nanoseconds,
    }
}
#[test]
fn output_confirmation_preserves_its_measurement_kind_and_uncertainty() {
    let mut measurements = Measurements::default();
    let token = measurements.receive(7, "precise", time(100));
    let kind = PresentationObservation::FirstPixelOutput { uncertainty_ns: 70 };
    measurements.start(&[token], |_| viewport(1));
    measurements.projected_requested(&[token], 7, 41);
    measurements.projected_observed(7, 41, time(150), kind);
    measurements.seal(7, PresentationId::new(2), &viewport(2));
    let samples = measurements.observed(PresentationId::new(2), time(300), kind);
    assert_eq!(samples[0]["observation"], "native-first-pixel-output");
    assert_eq!(
        samples[0]["projected_observation"],
        "native-first-pixel-output"
    );
    assert_eq!(samples[0]["timestamp_uncertainty_ns"], 70);
    assert_eq!(samples[0]["projected_timestamp_uncertainty_ns"], 70);
    assert_eq!(samples[0]["input_to_present_ns"], 200);
}

#[test]
fn projected_latency_requires_exact_native_submission_and_keeps_authoritative_completion() {
    let mut measurements = Measurements::default();
    let token = measurements.receive(7, "precise", time(100));
    measurements.projected_requested(&[token], 7, 41);
    measurements.projected_confirmed(8, 41, time(110));
    measurements.projected_confirmed(7, 40, time(120));
    measurements.projected_confirmed(
        7,
        41,
        PlatformTimestamp {
            clock_id: 2,
            nanoseconds: 130,
        },
    );
    assert!(measurements.pending[0].projected.is_none());
    measurements.projected_confirmed(7, 41, time(150));
    measurements.projected_confirmed(7, 41, time(160));
    assert!(
        measurements
            .confirmed(PresentationId::new(1), time(170))
            .is_empty()
    );
    measurements.start(&[token], |_| viewport(1));
    measurements.seal(7, PresentationId::new(2), &viewport(2));
    let samples = measurements.confirmed(PresentationId::new(2), time(300));
    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0]["input_to_projected_present_ns"], 50);
    assert_eq!(samples[0]["projected_submission"], 41);
    assert_eq!(samples[0]["input_to_present_ns"], 200);
    assert!(measurements.pending.is_empty());
}

#[test]
fn queued_input_and_other_frames_cannot_complete_a_latency_sample() {
    let mut measurements = Measurements::default();
    let token = measurements.receive(7, "wheel", time(100));
    measurements.seal(7, PresentationId::new(1), &viewport(2));
    assert!(
        measurements
            .confirmed(PresentationId::new(1), time(200))
            .is_empty()
    );
    measurements.start(&[token], |_| viewport(1));
    measurements.seal(8, PresentationId::new(2), &viewport(2));
    assert!(
        measurements
            .confirmed(PresentationId::new(2), time(300))
            .is_empty()
    );
    measurements.seal(7, PresentationId::new(3), &viewport(2));
    let samples = measurements.confirmed(PresentationId::new(3), time(400));
    assert_eq!(samples[0]["input_to_present_ns"], 300);
    assert!(
        measurements
            .confirmed(PresentationId::new(3), time(500))
            .is_empty()
    );
}
#[test]
fn superseded_layout_keeps_original_input_time_and_bounds_memory() {
    let mut measurements = Measurements::default();
    for _ in 0..MAX_PENDING + 1 {
        measurements.receive(7, "precise", time(100));
    }
    assert_eq!(measurements.pending.len(), MAX_PENDING);
    assert_eq!(measurements.dropped, 1);
    let token = measurements.pending.back().unwrap().token;
    measurements.start(&[token], |_| viewport(1));
    for id in 1..=20 {
        measurements.seal(7, PresentationId::new(id), &viewport(2));
    }
    assert_eq!(
        measurements.pending.back().unwrap().layouts.len(),
        MAX_PRESENTATIONS
    );
    assert!(
        measurements
            .confirmed(PresentationId::new(1), time(200))
            .is_empty()
    );
    let samples = measurements.confirmed(PresentationId::new(20), time(500));
    assert_eq!(samples[0]["input_to_present_ns"], 400);
    assert_eq!(samples[0]["evicted_inputs"], 1);
}
#[test]
fn command_preflight_cannot_ack_before_the_viewport_changes() {
    let mut measurements = Measurements::default();
    let token = measurements.receive(7, "precise", time(100));
    measurements.start(&[token], |_| viewport(1));
    measurements.seal(7, PresentationId::new(1), &viewport(1));
    assert!(
        measurements
            .confirmed(PresentationId::new(1), time(200))
            .is_empty()
    );
    measurements.seal(7, PresentationId::new(2), &viewport(2));
    assert_eq!(
        measurements
            .confirmed(PresentationId::new(2), time(300))
            .len(),
        1
    );
    let token = measurements.receive(7, "page", time(400));
    measurements.start(&[token], |_| viewport(2));
    measurements.cancel(&[token]);
    measurements.seal(7, PresentationId::new(3), &viewport(3));
    assert!(
        measurements
            .confirmed(PresentationId::new(3), time(500))
            .is_empty()
    );
}

#[test]
fn completed_no_op_cannot_be_credited_to_a_later_command() {
    let mut measurements = Measurements::default();
    let token = measurements.receive(7, "page", time(100));
    measurements.start(&[token], |_| viewport(1));
    measurements.finish(&[token], |_| viewport(1));
    measurements.seal(7, PresentationId::new(2), &viewport(2));
    assert!(
        measurements
            .confirmed(PresentationId::new(2), time(300))
            .is_empty()
    );
}

#[test]
fn completed_scrolls_can_be_coalesced_back_to_the_original_viewport() {
    let mut measurements = Measurements::default();
    let token = measurements.receive(7, "precise", time(100));
    measurements.start(&[token], |_| viewport(1));
    measurements.finish(&[token], |_| viewport(2));
    measurements.seal(7, PresentationId::new(2), &viewport(1));
    assert_eq!(
        measurements
            .confirmed(PresentationId::new(2), time(300))
            .len(),
        1
    );
}

#[test]
fn unrelated_clock_cannot_fabricate_latency() {
    let mut measurements = Measurements::default();
    let token = measurements.receive(7, "page", time(100));
    measurements.start(&[token], |_| viewport(1));
    measurements.seal(7, PresentationId::new(1), &viewport(2));
    let samples = measurements.confirmed(
        PresentationId::new(1),
        PlatformTimestamp {
            clock_id: 2,
            nanoseconds: 200,
        },
    );
    assert!(samples[0]["input_to_present_ns"].is_null());
}
