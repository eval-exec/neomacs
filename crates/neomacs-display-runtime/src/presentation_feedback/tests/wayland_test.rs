use super::*;
use neomacs_display_protocol::PresentationId;

fn submission(serial: u64, layout: u64) -> Submission {
    Submission {
        serial,
        frame: 7,
        layout: PresentationId::new(layout),
        width: 800,
        height: 600,
        scale: 1.0,
    }
}

#[test]
fn trace_only_feedback_requires_explicit_enablement() {
    assert!(matches!(
        initial_state(None, || false),
        ObserverState::Disabled
    ));
    assert!(matches!(
        initial_state(None, || true),
        ObserverState::Uninitialized(None)
    ));
}

#[test]
fn receipt_feedback_does_not_depend_on_trace_enablement() {
    let path = PathBuf::from("/run/neomacs-receipt");
    let state = initial_state(Some(path.clone()), || {
        panic!("receipt feedback must not depend on tracing")
    });
    assert!(matches!(state, ObserverState::Uninitialized(Some(actual)) if actual == path));
}

#[test]
fn trace_only_feedback_never_advances_file_receipts() {
    let mut receipts = Receipts {
        path: None,
        latest_presented: 0,
        clock_id: Some(1),
        pending: PendingReceipts::default(),
    };
    receipts.pending.requested(1, observe_platform_now());
    receipts.observe(NativeFeedback::Presented(ConfirmedPresentation {
        submission: submission(1, 100),
        timestamp: CompositorTimestamp {
            clock_id: 1,
            seconds: 42,
            nanoseconds: 123,
        },
    }));
    assert!(receipts.pending.0.is_empty());
    assert_eq!(receipts.latest_presented, 0);
}

#[test]
fn pending_receipts_wake_without_new_input_and_stop_after_ack_or_timeout() {
    let now = observe_platform_now();
    let mut pending = PendingReceipts::default();
    assert_eq!(pending.deadline(now), None);
    pending.requested(1, now);
    pending.requested(2, now);
    assert_eq!(
        pending.deadline(now),
        Some(now.plus(Duration::from_millis(4)))
    );
    pending.received(2);
    assert!(pending.deadline(now).is_some());
    pending.received(1);
    assert_eq!(pending.deadline(now), None);
    pending.requested(3, now);
    assert_eq!(pending.deadline(now.plus(Duration::from_secs(1))), None);
}

#[test]
fn receipt_identifies_confirmed_layout_not_latest_requested_layout() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("receipt");
    let mut receipts = Receipts {
        path: Some(path.clone()),
        latest_presented: 0,
        clock_id: Some(1),
        pending: PendingReceipts::default(),
    };
    let confirmed = |submission| {
        NativeFeedback::Presented(ConfirmedPresentation {
            submission,
            timestamp: CompositorTimestamp {
                clock_id: 1,
                seconds: 42,
                nanoseconds: 123,
            },
        })
    };
    receipts.observe(NativeFeedback::Discarded(submission(2, 200)));
    assert!(!path.exists());
    receipts.observe(confirmed(submission(1, 100)));
    let first = fs::read_to_string(&path).unwrap();
    assert!(first.contains(":submission 1 :frame 7 :presentation 100 "));
    receipts.observe(confirmed(submission(3, 300)));
    let latest = fs::read_to_string(&path).unwrap();
    assert!(latest.contains(":submission 3 :frame 7 :presentation 300 "));
    receipts.observe(confirmed(submission(1, 100)));
    receipts.observe(NativeFeedback::Discarded(submission(4, 400)));
    assert_eq!(fs::read_to_string(&path).unwrap(), latest);
}
