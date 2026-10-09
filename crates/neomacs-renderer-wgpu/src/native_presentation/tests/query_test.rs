use super::*;
fn session() -> SwapchainTiming {
    SwapchainTiming {
        swapchain: vk::SwapchainKHR::null(),
        generation: 1,
        domain: vk::TimeDomainKHR::from_raw(abi::SWAPCHAIN_LOCAL),
        domain_id: 9,
        counter: 3,
        outstanding: [1, 2].into(),
        last_id: 2,
    }
}
fn record(id: u64) -> abi::PastTiming {
    abi::PastTiming {
        present_id: id,
        complete: 1,
        stage_count: 1,
        domain: vk::TimeDomainKHR::from_raw(abi::SWAPCHAIN_LOCAL),
        domain_id: 9,
        ..Default::default()
    }
}
const CALIBRATION: Calibration = Calibration {
    monotonic: 100,
    local: 110,
    uncertainty_ns: 50,
};
const STAGE: abi::StageTime = abi::StageTime {
    stage: abi::FIRST_PIXEL_OUT,
    time: 200,
};
#[test]
fn native_records_require_exact_id_complete_stage_and_clock_generation() {
    let mut session = session();
    let observations = session
        .decode(3, 2, &[record(2), record(999)], &[STAGE; 2], CALIBRATION)
        .unwrap();
    assert_eq!(observations.len(), 1);
    assert_eq!(
        (observations[0].present_id, observations[0].monotonic_ns),
        (2, 190)
    );
    assert_eq!(observations[0].uncertainty_ns, 50);
    assert!(session.outstanding.contains(&1));
    assert!(
        session
            .decode(3, 1, &[record(2)], &[STAGE], CALIBRATION)
            .unwrap()
            .is_empty()
    );
    let mut partial = record(1);
    partial.complete = 0;
    assert!(
        session
            .decode(3, 1, &[partial], &[STAGE], CALIBRATION)
            .unwrap()
            .is_empty()
    );
    assert!(session.outstanding.contains(&1));
    assert!(
        session
            .decode(4, 1, &[record(1)], &[STAGE], CALIBRATION)
            .is_none()
    );
    assert!(
        session
            .decode(3, 2, &[record(1)], &[STAGE], CALIBRATION)
            .is_none()
    );
    let mut bad_domain = record(1);
    bad_domain.domain_id = 10;
    assert!(
        session
            .decode(3, 1, &[bad_domain], &[STAGE], CALIBRATION)
            .unwrap()
            .is_empty()
    );
}
#[test]
fn queue_completion_and_uncertain_clocks_are_not_pixel_output_evidence() {
    for (stage, calibration) in [
        (abi::StageTime { stage: 1, ..STAGE }, CALIBRATION),
        (abi::StageTime { time: 0, ..STAGE }, CALIBRATION),
        (
            STAGE,
            Calibration {
                uncertainty_ns: 1_000_001,
                ..CALIBRATION
            },
        ),
    ] {
        assert!(
            session()
                .decode(3, 1, &[record(1)], &[stage], calibration)
                .unwrap()
                .is_empty()
        );
    }
}
