use super::*;

#[test]
fn coordinator_rejects_request_beyond_shared_retry_budget() {
    let mut coordinator = FrameLayoutCoordinator::new(1);
    let first = FrameRelayoutRequest::FrameTabBar {
        assumed_height: 16.0,
        measured_height: 24.0,
    };
    let second = FrameRelayoutRequest::Minibuffer {
        window_id: DisplayWindowId::new(7),
        allocated_height_px: 16.0,
        required_height_px: 48.0,
    };

    assert_eq!(coordinator.request_retry(first), Ok(()));
    assert_eq!(
        coordinator.request_retry(second),
        Err(LayoutConvergenceError {
            retry_count: 1,
            max_retries: 1,
            last_request: second,
        })
    );
}
