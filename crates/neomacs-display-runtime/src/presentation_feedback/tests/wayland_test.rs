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
        path: path.clone(),
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

#[test]
fn display_scoped_drops_protocol_before_display_owner_on_drop_and_unwind() {
    use std::cell::RefCell;
    use std::rc::Rc;

    struct DropRecorder {
        label: &'static str,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for DropRecorder {
        fn drop(&mut self) {
            self.log.borrow_mut().push(self.label);
        }
    }

    struct FakeProtocol {
        _presentation: DropRecorder,
        _connection: DropRecorder,
        log: Rc<RefCell<Vec<&'static str>>>,
    }

    impl Drop for FakeProtocol {
        fn drop(&mut self) {
            self.log.borrow_mut().push("protocol_drop");
        }
    }

    let log = Rc::new(RefCell::new(Vec::new()));
    {
        let _scoped = DisplayScoped::new(
            FakeProtocol {
                _presentation: DropRecorder {
                    label: "presentation",
                    log: Rc::clone(&log),
                },
                _connection: DropRecorder {
                    label: "connection",
                    log: Rc::clone(&log),
                },
                log: Rc::clone(&log),
            },
            DropRecorder {
                label: "display_owner",
                log: Rc::clone(&log),
            },
        );
    }
    assert_eq!(
        *log.borrow(),
        vec![
            "protocol_drop",
            "presentation",
            "connection",
            "display_owner"
        ]
    );

    log.borrow_mut().clear();
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _scoped = DisplayScoped::new(
            FakeProtocol {
                _presentation: DropRecorder {
                    label: "presentation",
                    log: Rc::clone(&log),
                },
                _connection: DropRecorder {
                    label: "connection",
                    log: Rc::clone(&log),
                },
                log: Rc::clone(&log),
            },
            DropRecorder {
                label: "display_owner",
                log: Rc::clone(&log),
            },
        );
        panic!("simulated unwind");
    }));
    assert!(unwind.is_err());
    assert_eq!(
        *log.borrow(),
        vec![
            "protocol_drop",
            "presentation",
            "connection",
            "display_owner"
        ]
    );
}

#[test]
fn uninitialized_observer_without_attached_display_fails_closed_to_disabled() {
    let mut observer = PresentationObserver {
        state: ObserverState::Uninitialized {
            path: PathBuf::from("/tmp/unused-presentation-receipt"),
            display_owner: None,
        },
    };
    assert!(observer.take_uninitialized_display().is_none());
    assert!(matches!(observer.state, ObserverState::Disabled));
}
