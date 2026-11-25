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
            reason: LayoutConvergenceFailureReason::SharedBudgetExhausted,
            last_request: second,
        })
    );
}

#[test]
fn coordinator_exhausts_repeating_callback_causes_without_burning_full_budget() {
    let mut coordinator = FrameLayoutCoordinator::new(12);
    let w1 = DisplayWindowId::new(1);
    let w2 = DisplayWindowId::new(2);

    // Repeating WindowChromeInputsChanged on the same window exhausts after 1 retry.
    let chrome_inputs_w1 = FrameRelayoutRequest::WindowChromeInputsChanged { window_id: w1 };
    assert_eq!(coordinator.request_retry(chrome_inputs_w1), Ok(()));
    assert_eq!(
        coordinator.request_retry(chrome_inputs_w1),
        Err(LayoutConvergenceError {
            retry_count: 1,
            max_retries: 12,
            reason: LayoutConvergenceFailureReason::CauseRetryBudgetExhausted { cause_limit: 1 },
            last_request: chrome_inputs_w1,
        })
    );

    // Distinct window or distinct cause still gets its own converging retry.
    let chrome_inputs_w2 = FrameRelayoutRequest::WindowChromeInputsChanged { window_id: w2 };
    assert_eq!(coordinator.request_retry(chrome_inputs_w2), Ok(()));

    // Non-chrome LogicalInputsChanged (e.g. strict query prepass buffer switch)
    // retains the shared retry budget for multi-step convergence.
    let logical_w1 = FrameRelayoutRequest::LogicalInputsChanged { window_id: w1 };
    assert_eq!(coordinator.request_retry(logical_w1), Ok(()));
    assert_eq!(coordinator.request_retry(logical_w1), Ok(()));

    let face = FrameRelayoutRequest::FaceRealizationInputsChanged {
        before: 1,
        after: 2,
    };
    assert_eq!(coordinator.request_retry(face), Ok(()));
    assert_eq!(
        coordinator.request_retry(face),
        Err(LayoutConvergenceError {
            retry_count: 5,
            max_retries: 12,
            reason: LayoutConvergenceFailureReason::CauseRetryBudgetExhausted { cause_limit: 1 },
            last_request: face,
        })
    );

    // Progress-aware SourceOnlyRetry: distinct visible stop fingerprints
    // continue converging, while repeating the exact same stop set exhausts
    // after 1 retry.
    let sparse_w1_round1 = FrameRelayoutRequest::SourceOnlyRetry {
        window_id: w1,
        stops_fingerprint: 0x1111,
    };
    let sparse_w1_round2 = FrameRelayoutRequest::SourceOnlyRetry {
        window_id: w1,
        stops_fingerprint: 0x2222,
    };
    assert_eq!(coordinator.request_retry(sparse_w1_round1), Ok(()));
    assert_eq!(coordinator.request_retry(sparse_w1_round2), Ok(()));
    assert_eq!(
        coordinator.request_retry(sparse_w1_round2),
        Err(LayoutConvergenceError {
            retry_count: 7,
            max_retries: 12,
            reason: LayoutConvergenceFailureReason::CauseRetryBudgetExhausted { cause_limit: 1 },
            last_request: sparse_w1_round2,
        })
    );

    let display_when_w1 = FrameRelayoutRequest::PostWalkDisplayWhenInputsChanged { window_id: w1 };
    assert_eq!(coordinator.request_retry(display_when_w1), Ok(()));
    assert_eq!(
        coordinator.request_retry(display_when_w1),
        Err(LayoutConvergenceError {
            retry_count: 8,
            max_retries: 12,
            reason: LayoutConvergenceFailureReason::CauseRetryBudgetExhausted { cause_limit: 1 },
            last_request: display_when_w1,
        })
    );
}

#[test]
fn coordinator_allows_monotonic_chrome_heights_and_detects_oscillation_cycles() {
    let mut coordinator = FrameLayoutCoordinator::new(12);
    let w1 = DisplayWindowId::new(1);
    let h = |mode_h: f32| WindowChromeMetrics {
        tab_line_height: 0.0,
        header_line_height: 0.0,
        mode_line_height: mode_h,
    };

    // Three distinct converging height steps all succeed (no artificial cap of 2).
    assert_eq!(
        coordinator.request_retry(FrameRelayoutRequest::WindowChrome {
            window_id: w1,
            assumed: h(16.0),
            measured: h(24.0),
        }),
        Ok(())
    );
    assert_eq!(
        coordinator.request_retry(FrameRelayoutRequest::WindowChrome {
            window_id: w1,
            assumed: h(24.0),
            measured: h(32.0),
        }),
        Ok(())
    );
    assert_eq!(
        coordinator.request_retry(FrameRelayoutRequest::WindowChrome {
            window_id: w1,
            assumed: h(32.0),
            measured: h(40.0),
        }),
        Ok(())
    );

    // A single excursion back to an earlier assumed height (40 -> 16) is allowed
    // because the transition pair (40, 16) has not been taken before.
    assert_eq!(
        coordinator.request_retry(FrameRelayoutRequest::WindowChrome {
            window_id: w1,
            assumed: h(40.0),
            measured: h(16.0),
        }),
        Ok(())
    );

    // Repeating an already-taken (assumed, measured) transition pair (16 -> 24)
    // is a proven cycle and exhausts immediately.
    let cycle = FrameRelayoutRequest::WindowChrome {
        window_id: w1,
        assumed: h(16.0),
        measured: h(24.0),
    };
    assert_eq!(
        coordinator.request_retry(cycle),
        Err(LayoutConvergenceError {
            retry_count: 4,
            max_retries: 12,
            reason: LayoutConvergenceFailureReason::ChromeHeightCycle,
            last_request: cycle,
        })
    );
}
