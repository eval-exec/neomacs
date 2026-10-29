//! Application-policy controls using the real core connection and owned receiver.
//! Exact wake count/failure/unwind controls live in neovm-core::host_input::tests.
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::super::super::{PrimaryWindowSize, read_primary_window_size};
use super::*;
#[cfg(unix)]
use neomacs_display_runtime::tty_input::TerminalCellSize;
use neomacs_display_runtime::tty_input::{TerminalColumns, TerminalRows};
use neovm_core::emacs_core::Context;
use neovm_core::host_input::{HostInputPublishError, HostInputWakeOutcome};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    QuitRequested,
    BeforeEnqueue,
}

#[derive(Debug)]
struct TestQuit {
    steps: Arc<Mutex<Vec<Step>>>,
}

impl PublicationQuit for TestQuit {
    fn request(&self) {
        self.steps.lock().unwrap().push(Step::QuitRequested);
    }
}

struct Fixture<P = InputPublisher<TestQuit>> {
    publisher: P,
    context: Context,
    steps: Arc<Mutex<Vec<Step>>>,
    primary_size: SharedPrimaryWindowSize,
}

impl Fixture {
    fn new(route: InputRoute) -> Self {
        Self::with_factory(move |input, size, quit| InputPublisher::new(input, route, size, quit))
    }
}

#[cfg(unix)]
impl Fixture<PrimaryInputPublisher<TestQuit>> {
    fn primary() -> Self {
        Self::with_factory(PrimaryInputPublisher::new)
    }
}

impl<P> Fixture<P> {
    fn with_factory(
        factory: impl FnOnce(HostInputPublisher, SharedPrimaryWindowSize, TestQuit) -> P,
    ) -> Self {
        let mut context = Context::new();
        let input = HostInputPublisher::install(&mut context);
        let steps = Arc::new(Mutex::new(Vec::new()));
        let primary_size = Arc::new(Mutex::new(PrimaryWindowSize {
            width: 80,
            height: 24,
        }));
        let publisher = factory(
            input,
            Arc::clone(&primary_size),
            TestQuit {
                steps: Arc::clone(&steps),
            },
        );
        Self {
            publisher,
            context,
            steps,
            primary_size,
        }
    }

    fn take(&self) -> Vec<KbInputEvent> {
        let mut queued = Vec::new();
        loop {
            match self.context.receive_host_input_for_test(Duration::ZERO) {
                Ok(event) => queued.push(event),
                Err(crossbeam_channel::RecvTimeoutError::Timeout) => return queued,
                Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                    panic!("fixture publisher must retain its live connection")
                }
            }
        }
    }

    fn assert_empty(&self) {
        assert!(matches!(
            self.context.receive_host_input_for_test(Duration::ZERO),
            Err(crossbeam_channel::RecvTimeoutError::Timeout)
        ));
    }
}

fn delivered(result: Publication) {
    match result {
        Publication::Delivered => {}
        Publication::ReceiverClosed => panic!("expected delivered publication"),
    }
}

fn closed(result: Publication) {
    match result {
        Publication::Delivered => panic!("expected closed receiver"),
        Publication::ReceiverClosed => {}
    }
}

fn raw(bytes: &[u8]) -> DisplayEvent {
    DisplayEvent::RawTtyBytes {
        bytes: bytes.to_vec(),
        emacs_frame_id: 0,
    }
}

fn resize(frame: FrameId, columns: TerminalColumns, rows: TerminalRows) -> DisplayEvent {
    DisplayEvent::WindowResize {
        width: u32::from(columns),
        height: u32::from(rows),
        scale_factor: 1.0,
        emacs_frame_id: frame.0,
    }
}

#[derive(Debug)]
enum BoundaryObservation {
    BeforeEnqueue,
}
#[derive(Debug)]
enum BoundaryRelease {
    Observed,
}

#[derive(Debug)]
enum BoundaryAdmission {
    Armed,
    Consumed,
}

/// Context remains on this thread; the native-only router pauses at its actual
/// pre-enqueue boundary. Typed control channels carry no host input capability.
fn observe_enqueue_boundary(
    publisher: &mut InputPublisher<TestQuit>,
    context: &Context,
    event: DisplayEvent,
    observe_input: impl Fn(&KbInputEvent) + Send + Sync + 'static,
    observe_context: impl FnOnce(&Context),
) {
    let (observed_tx, observed_rx) = crossbeam_channel::bounded(0);
    let (release_tx, release_rx) = crossbeam_channel::bounded(0);
    let admission = Mutex::new(BoundaryAdmission::Armed);
    publisher.before_enqueue = BeforeEnqueue::Observe(Arc::new(move |input| {
        {
            let mut admission = admission.lock().unwrap();
            match *admission {
                BoundaryAdmission::Armed => *admission = BoundaryAdmission::Consumed,
                BoundaryAdmission::Consumed => panic!("singleton conversion enqueued twice"),
            }
        }
        observe_input(input);
        observed_tx
            .send(BoundaryObservation::BeforeEnqueue)
            .unwrap();
        match release_rx.recv().expect("observer retains release owner") {
            BoundaryRelease::Observed => {}
        }
    }));
    std::thread::scope(|scope| {
        // Owning the release sender in this closure guarantees cancellation
        // unblocks the worker before scope joins it if an assertion panics.
        let release = release_tx;
        let observations = observed_rx;
        let worker = scope.spawn(|| publisher.publish_display(&event));
        match observations
            .recv_timeout(Duration::from_secs(10))
            .expect("router reaches enqueue boundary")
        {
            BoundaryObservation::BeforeEnqueue => {}
        }
        observe_context(context);
        release.send(BoundaryRelease::Observed).unwrap();
        delivered(worker.join().unwrap());
    });
    publisher.before_enqueue = BeforeEnqueue::Inactive;
}

#[test]
fn router_uses_the_installed_context_connection() {
    let mut fixture = Fixture::new(InputRoute::Tty);
    delivered(fixture.publisher.publish_display(&raw(b"native")));
    assert!(matches!(fixture.take().as_slice(),
        [KbInputEvent::RawTtyBytes { bytes, .. }] if bytes == b"native"));
    fixture.assert_empty();
}

#[test]
fn primary_size_and_resize_are_visible_at_publication_completion() {
    let mut fixture = Fixture::new(InputRoute::Tty);
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        101_u32.into(),
        37_u32.into(),
    )));
    assert_eq!(
        read_primary_window_size(&fixture.primary_size),
        PrimaryWindowSize {
            width: 101,
            height: 37
        }
    );
    assert!(matches!(
        fixture.take().as_slice(),
        [KbInputEvent::Resize {
            width: 101,
            height: 37,
            emacs_frame_id: 0,
            ..
        }]
    ));
    fixture.assert_empty();
}

#[test]
fn shared_size_changes_before_the_first_core_enqueue() {
    let mut fixture = Fixture::new(InputRoute::Tty);
    let size = Arc::clone(&fixture.primary_size);
    let steps = Arc::clone(&fixture.steps);
    observe_enqueue_boundary(
        &mut fixture.publisher,
        &fixture.context,
        resize(FrameId(0), 101_u32.into(), 37_u32.into()),
        move |input| {
            assert_eq!(
                read_primary_window_size(&size),
                PrimaryWindowSize {
                    width: 101,
                    height: 37
                }
            );
            assert!(matches!(
                input,
                KbInputEvent::Resize {
                    width: 101,
                    height: 37,
                    ..
                }
            ));
            steps.lock().unwrap().push(Step::BeforeEnqueue);
        },
        |context| {
            assert!(
                matches!(
                    context.receive_host_input_for_test(Duration::ZERO),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout)
                ),
                "observation must precede first enqueue"
            )
        },
    );
    assert_eq!(*fixture.steps.lock().unwrap(), [Step::BeforeEnqueue]);
    assert_eq!(fixture.take().len(), 1);
}

#[test]
fn gui_resolves_only_original_window_alias_after_recording_primary_size() {
    let mut fixture = Fixture::new(InputRoute::gui(Arc::new(Mutex::new(Some(FrameId(42))))));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        100_u32.into(),
        32_u32.into(),
    )));
    assert!(matches!(
        fixture.take().as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 42,
            ..
        }]
    ));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(77),
        50_u32.into(),
        20_u32.into(),
    )));
    assert!(matches!(
        fixture.take().as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 77,
            ..
        }]
    ));
    assert_eq!(
        read_primary_window_size(&fixture.primary_size),
        PrimaryWindowSize {
            width: 100,
            height: 32
        }
    );
}

#[test]
fn gui_unassigned_primary_skips_only_zero_resize_after_recording_size() {
    let mut fixture = Fixture::new(InputRoute::gui(Arc::new(Mutex::new(None))));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        101_u32.into(),
        37_u32.into(),
    )));
    assert_eq!(
        read_primary_window_size(&fixture.primary_size),
        PrimaryWindowSize {
            width: 101,
            height: 37
        }
    );
    fixture.assert_empty();
    assert!(fixture.steps.lock().unwrap().is_empty());
}

#[test]
fn gui_unassigned_primary_still_delivers_zero_alias_keys() {
    let mut fixture = Fixture::new(InputRoute::gui(Arc::new(Mutex::new(None))));
    delivered(fixture.publisher.publish_display(&DisplayEvent::Key {
        key: neovm_core::keyboard::FrontendKey::Keysym(neovm_core::keyboard::XK_RETURN),
        modifiers: 0,
        pressed: true,
        emacs_frame_id: 0,
    }));
    assert!(matches!(
        fixture.take().as_slice(),
        [KbInputEvent::KeyPress {
            emacs_frame_id: 0,
            ..
        }]
    ));
}

#[test]
fn gui_primary_alias_observes_assignment_replacement_and_clear() {
    let owner = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::new(InputRoute::gui(Arc::clone(&owner)));
    *owner.lock().unwrap() = Some(FrameId(42));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        100_u32.into(),
        32_u32.into(),
    )));
    let first = fixture.take();
    *owner.lock().unwrap() = Some(FrameId(99));
    delivered(
        fixture
            .publisher
            .publish_display(&resize(FrameId(0), 90_u32.into(), 27_u32.into())),
    );
    let second = fixture.take();
    *owner.lock().unwrap() = None;
    delivered(
        fixture
            .publisher
            .publish_display(&resize(FrameId(0), 70_u32.into(), 22_u32.into())),
    );
    fixture.assert_empty();
    assert!(matches!(
        first.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 42,
            ..
        }]
    ));
    assert!(matches!(
        second.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 99,
            ..
        }]
    ));
    assert_eq!(
        read_primary_window_size(&fixture.primary_size),
        PrimaryWindowSize {
            width: 70,
            height: 22
        }
    );
}

#[test]
fn gui_explicit_resize_delivers_while_primary_is_unassigned() {
    let mut fixture = Fixture::new(InputRoute::gui(Arc::new(Mutex::new(None))));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(77),
        50_u32.into(),
        20_u32.into(),
    )));
    assert!(matches!(
        fixture.take().as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 77,
            ..
        }]
    ));
    assert_eq!(
        read_primary_window_size(&fixture.primary_size),
        PrimaryWindowSize {
            width: 80,
            height: 24
        }
    );
}

#[test]
fn gui_primary_alias_recovers_poisoned_owner() {
    let owner = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::new(InputRoute::gui(Arc::clone(&owner)));
    assert!(
        std::panic::catch_unwind(|| {
            *owner.lock().unwrap() = Some(FrameId(42));
            // Hold the owner lock across the intentional panic.
            let _guard = owner.lock().unwrap();
            panic!("deterministic GUI owner poison");
        })
        .is_err()
    );
    assert!(owner.is_poisoned());
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        101_u32.into(),
        37_u32.into(),
    )));
    assert!(matches!(
        fixture.take().as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 42,
            ..
        }]
    ));
}

#[test]
fn zero_sized_resize_preserves_shared_size_and_existing_core_fact() {
    let mut fixture = Fixture::new(InputRoute::Tty);
    delivered(
        fixture
            .publisher
            .publish_display(&resize(FrameId(0), 0_u32.into(), 37_u32.into())),
    );
    assert_eq!(read_primary_window_size(&fixture.primary_size).width, 80);
    assert!(matches!(
        fixture.take().as_slice(),
        [KbInputEvent::Resize {
            width: 0,
            height: 37,
            ..
        }]
    ));
}

#[test]
fn raw_quit_request_precedes_the_first_core_enqueue() {
    let mut fixture = Fixture::new(InputRoute::Tty);
    let steps = Arc::clone(&fixture.steps);
    observe_enqueue_boundary(
        &mut fixture.publisher,
        &fixture.context,
        raw(b"\x1b[Ax\x07\xc3"),
        move |_| {
            assert_eq!(*steps.lock().unwrap(), [Step::QuitRequested]);
            steps.lock().unwrap().push(Step::BeforeEnqueue);
        },
        |context| {
            assert!(
                matches!(
                    context.receive_host_input_for_test(Duration::ZERO),
                    Err(crossbeam_channel::RecvTimeoutError::Timeout)
                ),
                "quit observation precedes first enqueue"
            )
        },
    );
    assert_eq!(
        *fixture.steps.lock().unwrap(),
        [Step::QuitRequested, Step::BeforeEnqueue]
    );
    assert!(
        matches!(fixture.take().as_slice(), [KbInputEvent::RawTtyBytes { bytes, target: neovm_core::keyboard::TtyInputTarget::Terminal(0) }] if bytes == b"\x1b[Ax\x07\xc3")
    );
}

#[test]
fn failed_wake_after_commit_maps_to_delivered_without_republication() {
    delivered(publication_result(finish_host_input_publication(Err(
        HostInputPublishError::WakeAfterCommit(std::io::Error::other(
            "deterministic typed wake outcome",
        )),
    ))));
    // Real enqueue count and native-wake no-retry are checked by core's
    // postcommit_wake_failure_does_not_replay_or_retry control, not a shadow publisher.
}

#[test]
fn receiver_close_preserves_early_quit() {
    let Fixture {
        mut publisher,
        context,
        steps,
        ..
    } = Fixture::new(InputRoute::Tty);
    drop(context);
    closed(publisher.publish_display(&raw(b"\x07")));
    assert_eq!(*steps.lock().unwrap(), [Step::QuitRequested]);
}

#[test]
fn primary_size_is_recorded_even_when_first_core_send_fails() {
    let Fixture {
        mut publisher,
        context,
        primary_size,
        ..
    } = Fixture::new(InputRoute::Tty);
    drop(context);
    closed(publisher.publish_display(&resize(FrameId(0), 101_u32.into(), 37_u32.into())));
    assert_eq!(
        read_primary_window_size(&primary_size),
        PrimaryWindowSize {
            width: 101,
            height: 37
        }
    );
}

#[test]
fn converted_batch_is_fifo_and_preserves_early_quit() {
    let fixture = Fixture::new(InputRoute::Tty);
    delivered(fixture.publisher.publish_batch([
        KbInputEvent::raw_tty_bytes_for_terminal(b"a".to_vec(), 0),
        KbInputEvent::raw_tty_bytes_for_terminal(b"\x07".to_vec(), 0),
    ]));
    assert_eq!(*fixture.steps.lock().unwrap(), [Step::QuitRequested]);
    assert!(matches!(fixture.take().as_slice(),
        [KbInputEvent::RawTtyBytes { bytes: a, .. }, KbInputEvent::RawTtyBytes { bytes: quit, .. }]
        if a == b"a" && quit == b"\x07"));
}

#[test]
fn direct_core_input_uses_the_installed_context_connection() {
    let mut fixture = Fixture::new(InputRoute::Tty);
    delivered(
        fixture
            .publisher
            .publish_core(KbInputEvent::raw_tty_bytes_for_terminal(
                b"already-core".to_vec(),
                0,
            )),
    );
    assert!(
        matches!(fixture.take().as_slice(), [KbInputEvent::RawTtyBytes { bytes, .. }] if bytes == b"already-core")
    );
}

#[test]
fn empty_conversion_neither_requests_quit_nor_enqueues() {
    let fixture = Fixture::new(InputRoute::Tty);
    delivered(fixture.publisher.publish_batch(std::iter::empty()));
    assert!(fixture.steps.lock().unwrap().is_empty());
    fixture.assert_empty();
}

#[cfg(unix)]
#[test]
fn runtime_primary_facts_share_the_existing_bridge_conversion() {
    let mut fixture = Fixture::primary();
    delivered(
        fixture
            .publisher
            .publish(PrimaryTtyInput::Bytes(b"\xc3".to_vec())),
    );
    delivered(
        fixture
            .publisher
            .publish(PrimaryTtyInput::Bytes(b"\xa9".to_vec())),
    );
    delivered(
        fixture
            .publisher
            .publish(PrimaryTtyInput::Resize(TerminalCellSize::new(
                101_u32.into(),
                37_u32.into(),
            ))),
    );
    assert!(matches!(fixture.take().as_slice(), [
        KbInputEvent::RawTtyBytes { bytes: first, target: neovm_core::keyboard::TtyInputTarget::Terminal(0) },
        KbInputEvent::RawTtyBytes { bytes: second, target: neovm_core::keyboard::TtyInputTarget::Terminal(0) },
        KbInputEvent::Resize { width: 101, height: 37, emacs_frame_id: 0, .. }
    ] if first == b"\xc3" && second == b"\xa9"));
}

#[test]
fn actual_key_release_conversion_neither_enqueues_nor_requests_quit() {
    let mut fixture = Fixture::new(InputRoute::Tty);
    delivered(fixture.publisher.publish_display(&DisplayEvent::Key {
        key: neovm_core::keyboard::FrontendKey::Keysym(neovm_core::keyboard::XK_RETURN),
        modifiers: 0,
        pressed: false,
        emacs_frame_id: 0,
    }));
    assert!(fixture.steps.lock().unwrap().is_empty());
    fixture.assert_empty();
}

#[cfg(unix)]
#[test]
fn fixed_primary_factory_uses_the_installed_terminal_zero_connection() {
    let mut fixture = Fixture::primary();
    delivered(
        fixture
            .publisher
            .publish(PrimaryTtyInput::Bytes(b"fixed-primary".to_vec())),
    );
    assert!(
        matches!(fixture.take().as_slice(), [KbInputEvent::RawTtyBytes {
        bytes, target: neovm_core::keyboard::TtyInputTarget::Terminal(0),
    }] if bytes == b"fixed-primary")
    );
}

#[test]
fn partial_receiver_close_maps_to_closed_and_retains_early_quit() {
    let Fixture {
        publisher,
        context,
        steps,
        ..
    } = Fixture::new(InputRoute::Tty);
    let mut context = Some(context);
    let events = [b"a".as_slice(), b"\x07".as_slice()]
        .into_iter()
        .map(|bytes| {
            if bytes == b"\x07" {
                let receiver_owner = context.take().expect("second event owns context");
                assert!(
                    matches!(receiver_owner.receive_host_input_for_test(Duration::ZERO).unwrap(),
                KbInputEvent::RawTtyBytes { bytes, .. } if bytes == b"a")
                );
                drop(receiver_owner);
            }
            KbInputEvent::raw_tty_bytes_for_terminal(bytes.to_vec(), 0)
        });
    closed(publisher.publish_batch(events));
    assert_eq!(*steps.lock().unwrap(), [Step::QuitRequested]);
}

#[test]
fn partial_receiver_close_wake_failure_still_maps_to_closed() {
    closed(publication_result(finish_host_input_publication(Err(
        HostInputPublishError::ReceiverClosedAfterPrefix {
            wake: HostInputWakeOutcome::Failed(std::io::Error::other("typed combined outcome")),
        },
    ))));
}
