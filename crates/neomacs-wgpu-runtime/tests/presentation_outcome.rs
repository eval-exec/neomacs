use neomacs_wgpu_runtime::{PresentationOutcome, PresentationSkipReason};

#[test]
fn transient_surface_changes_request_another_redraw() {
    assert!(PresentationOutcome::Skipped(PresentationSkipReason::Timeout).should_request_redraw());
    assert!(
        PresentationOutcome::Skipped(PresentationSkipReason::SurfaceChanged)
            .should_request_redraw()
    );
}

#[test]
fn host_invisibility_waits_for_a_host_event() {
    assert!(
        !PresentationOutcome::Skipped(PresentationSkipReason::Suspended).should_request_redraw()
    );
    assert!(
        !PresentationOutcome::Skipped(PresentationSkipReason::Occluded).should_request_redraw()
    );
    assert!(!PresentationOutcome::Presented.should_request_redraw());
}
