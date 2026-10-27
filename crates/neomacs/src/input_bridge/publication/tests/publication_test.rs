use std::sync::{Arc, Mutex};

use super::super::super::{PrimaryWindowSize, read_primary_window_size};
use super::*;
#[cfg(unix)]
use neomacs_display_runtime::tty_input::TerminalCellSize;
use neomacs_display_runtime::tty_input::{TerminalColumns, TerminalRows};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    QuitRequested,
    BeforeEnqueue,
    Wake,
}

#[derive(Clone, Copy, Debug)]
enum WakeResult {
    Success,
    Failure,
}

#[derive(Debug)]
enum QueueProbe {
    AwaitingFactory,
    Bound(Receiver<KbInputEvent>),
    ReceiverClosed,
}

#[derive(Debug)]
struct WakeObservation {
    queued: Vec<KbInputEvent>,
    primary_size: PrimaryWindowSize,
}

#[derive(Debug)]
struct TestWake {
    steps: Arc<Mutex<Vec<Step>>>,
    queue: Arc<Mutex<QueueProbe>>,
    observations: Arc<Mutex<Vec<WakeObservation>>>,
    primary_size: SharedPrimaryWindowSize,
    result: WakeResult,
}

impl PublicationWake for TestWake {
    fn notify(&self) -> io::Result<()> {
        self.steps.lock().unwrap().push(Step::Wake);
        let queued = match &*self.queue.lock().unwrap() {
            QueueProbe::Bound(receiver) => receiver.try_iter().collect(),
            QueueProbe::AwaitingFactory => panic!("test queue was not bound by factory"),
            QueueProbe::ReceiverClosed => panic!("closed receiver must not be notified"),
        };
        self.observations.lock().unwrap().push(WakeObservation {
            queued,
            primary_size: read_primary_window_size(&self.primary_size),
        });
        match self.result {
            WakeResult::Success => Ok(()),
            WakeResult::Failure => Err(io::Error::other("deterministic native wake failure")),
        }
    }
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

#[derive(Debug)]
struct Fixture<P = InputPublisher<TestWake, TestQuit>> {
    publisher: P,
    receiver: Receiver<KbInputEvent>,
    queue: Arc<Mutex<QueueProbe>>,
    steps: Arc<Mutex<Vec<Step>>>,
    observations: Arc<Mutex<Vec<WakeObservation>>>,
    primary_size: SharedPrimaryWindowSize,
}

impl Fixture {
    fn new(route: InputRoute, result: WakeResult) -> Self {
        Self::with_factory(result, move |size, quit, wake| {
            InputPublisher::with_resources(route, size, quit, wake)
        })
    }
}

#[cfg(unix)]
impl Fixture<PrimaryInputPublisher<TestWake, TestQuit>> {
    fn primary(result: WakeResult) -> Self {
        Self::with_factory(result, PrimaryInputPublisher::with_resources)
    }
}

impl<P> Fixture<P> {
    fn with_factory(
        result: WakeResult,
        factory: impl FnOnce(SharedPrimaryWindowSize, TestQuit, TestWake) -> (P, Receiver<KbInputEvent>),
    ) -> Self {
        let steps = Arc::new(Mutex::new(Vec::new()));
        let queue = Arc::new(Mutex::new(QueueProbe::AwaitingFactory));
        let observations = Arc::new(Mutex::new(Vec::new()));
        let primary_size = Arc::new(Mutex::new(PrimaryWindowSize {
            width: 80,
            height: 24,
        }));
        let (publisher, receiver) = factory(
            Arc::clone(&primary_size),
            TestQuit {
                steps: Arc::clone(&steps),
            },
            TestWake {
                steps: Arc::clone(&steps),
                queue: Arc::clone(&queue),
                observations: Arc::clone(&observations),
                primary_size: Arc::clone(&primary_size),
                result,
            },
        );
        *queue.lock().unwrap() = QueueProbe::Bound(receiver.clone());
        Self {
            publisher,
            receiver,
            queue,
            steps,
            observations,
            primary_size,
        }
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

#[test]
fn production_factory_owns_an_unbounded_queue() {
    let size = Arc::new(Mutex::new(PrimaryWindowSize {
        width: 80,
        height: 24,
    }));
    let (mut publisher, receiver) =
        InputPublisher::unbounded(InputRoute::Tty, size, QuitRequest::new(), None);
    assert_eq!(receiver.capacity(), None);
    delivered(publisher.publish_display(&raw(b"native")));
    assert!(matches!(
        receiver.try_recv().unwrap(),
        KbInputEvent::RawTtyBytes { bytes, .. } if bytes == b"native"
    ));
}

#[test]
fn primary_size_and_resize_are_visible_when_post_enqueue_wake_runs() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        101_u32.into(),
        37_u32.into(),
    )));
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert_eq!(
        observed[0].primary_size,
        PrimaryWindowSize {
            width: 101,
            height: 37,
        }
    );
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::Resize {
            width: 101,
            height: 37,
            emacs_frame_id: 0,
            ..
        }]
    ));
    assert!(fixture.receiver.is_empty());
}

#[test]
fn shared_size_changes_before_the_first_core_enqueue() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    let receiver = fixture.receiver.clone();
    let primary_size = Arc::clone(&fixture.primary_size);
    let steps = Arc::clone(&fixture.steps);
    fixture.publisher.before_enqueue = BeforeEnqueue::Observe(Arc::new(move |input| {
        assert!(
            receiver.is_empty(),
            "observation must precede first enqueue"
        );
        assert_eq!(
            read_primary_window_size(&primary_size),
            PrimaryWindowSize {
                width: 101,
                height: 37,
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
    }));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        101_u32.into(),
        37_u32.into(),
    )));
    assert_eq!(
        *fixture.steps.lock().unwrap(),
        [Step::BeforeEnqueue, Step::Wake]
    );
}

#[test]
fn gui_resolves_only_original_window_alias_after_recording_primary_size() {
    let mut fixture = Fixture::new(
        InputRoute::gui(Arc::new(Mutex::new(Some(FrameId(42))))),
        WakeResult::Success,
    );
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        100_u32.into(),
        32_u32.into(),
    )));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(77),
        50_u32.into(),
        20_u32.into(),
    )));
    let observed = fixture.observations.lock().unwrap();
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 42,
            ..
        }]
    ));
    assert!(matches!(
        observed[1].queued.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 77,
            ..
        }]
    ));
    assert_eq!(
        observed[1].primary_size,
        PrimaryWindowSize {
            width: 100,
            height: 32,
        }
    );
}

#[test]
fn gui_unassigned_primary_skips_only_zero_resize_after_recording_size() {
    let primary_frame = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::new(InputRoute::gui(primary_frame), WakeResult::Success);
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        101_u32.into(),
        37_u32.into(),
    )));
    assert_eq!(
        read_primary_window_size(&fixture.primary_size),
        PrimaryWindowSize {
            width: 101,
            height: 37,
        }
    );
    assert!(fixture.receiver.is_empty());
    assert!(fixture.observations.lock().unwrap().is_empty());
    assert!(fixture.steps.lock().unwrap().is_empty());
}

#[test]
fn gui_unassigned_primary_still_delivers_zero_alias_keys() {
    let primary_frame = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::new(InputRoute::gui(primary_frame), WakeResult::Success);
    delivered(fixture.publisher.publish_display(&DisplayEvent::Key {
        key: neovm_core::keyboard::FrontendKey::Keysym(neovm_core::keyboard::XK_RETURN),
        modifiers: 0,
        pressed: true,
        emacs_frame_id: 0,
    }));
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::KeyPress {
            emacs_frame_id: 0,
            ..
        }]
    ));
    assert_eq!(*fixture.steps.lock().unwrap(), [Step::Wake]);
}

#[test]
fn gui_primary_alias_observes_assignment_replacement_and_clear() {
    let primary_frame = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::new(
        InputRoute::gui(Arc::clone(&primary_frame)),
        WakeResult::Success,
    );
    *primary_frame.lock().unwrap() = Some(FrameId(42));
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        100_u32.into(),
        32_u32.into(),
    )));
    *primary_frame.lock().unwrap() = Some(FrameId(99));
    delivered(
        fixture
            .publisher
            .publish_display(&resize(FrameId(0), 90_u32.into(), 27_u32.into())),
    );
    *primary_frame.lock().unwrap() = None;
    delivered(
        fixture
            .publisher
            .publish_display(&resize(FrameId(0), 70_u32.into(), 22_u32.into())),
    );
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(
        observed.len(),
        2,
        "cleared owner suppresses only its alias resize"
    );
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 42,
            ..
        }]
    ));
    assert!(matches!(
        observed[1].queued.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 99,
            ..
        }]
    ));
    assert_eq!(
        read_primary_window_size(&fixture.primary_size),
        PrimaryWindowSize {
            width: 70,
            height: 22,
        }
    );
    assert_eq!(*fixture.steps.lock().unwrap(), [Step::Wake, Step::Wake]);
}

#[test]
fn gui_explicit_resize_delivers_while_primary_is_unassigned() {
    let primary_frame = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::new(InputRoute::gui(primary_frame), WakeResult::Success);
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(77),
        50_u32.into(),
        20_u32.into(),
    )));
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 77,
            ..
        }]
    ));
    assert_eq!(
        observed[0].primary_size,
        PrimaryWindowSize {
            width: 80,
            height: 24,
        }
    );
}

#[test]
fn gui_primary_alias_recovers_poisoned_owner() {
    let primary_frame = Arc::new(Mutex::new(None));
    let mut fixture = Fixture::new(
        InputRoute::gui(Arc::clone(&primary_frame)),
        WakeResult::Success,
    );
    let poisoned = std::panic::catch_unwind(|| {
        let mut owner = primary_frame.lock().unwrap();
        *owner = Some(FrameId(42));
        panic!("deterministic GUI owner poison");
    });
    assert!(poisoned.is_err());
    assert!(primary_frame.is_poisoned());
    delivered(fixture.publisher.publish_display(&resize(
        FrameId(0),
        101_u32.into(),
        37_u32.into(),
    )));
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(observed.len(), 1);
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::Resize {
            emacs_frame_id: 42,
            ..
        }]
    ));
    assert_eq!(*fixture.steps.lock().unwrap(), [Step::Wake]);
}

#[test]
fn zero_sized_resize_preserves_shared_size_and_existing_core_fact() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    delivered(
        fixture
            .publisher
            .publish_display(&resize(FrameId(0), 0_u32.into(), 37_u32.into())),
    );
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(observed[0].primary_size.width, 80);
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::Resize {
            width: 0,
            height: 37,
            ..
        }]
    ));
}

#[test]
fn raw_quit_request_precedes_the_wake_that_observes_committed_input() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    let receiver = fixture.receiver.clone();
    let steps = Arc::clone(&fixture.steps);
    fixture.publisher.before_enqueue = BeforeEnqueue::Observe(Arc::new(move |_| {
        assert!(
            receiver.is_empty(),
            "quit observation precedes first enqueue"
        );
        assert_eq!(*steps.lock().unwrap(), [Step::QuitRequested]);
        steps.lock().unwrap().push(Step::BeforeEnqueue);
    }));
    delivered(fixture.publisher.publish_display(&raw(b"\x1b[Ax\x07\xc3")));
    assert_eq!(
        *fixture.steps.lock().unwrap(),
        [Step::QuitRequested, Step::BeforeEnqueue, Step::Wake]
    );
    let observed = fixture.observations.lock().unwrap();
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::RawTtyBytes {
            bytes,
            target: neovm_core::keyboard::TtyInputTarget::Terminal(0),
        }] if bytes == b"\x1b[Ax\x07\xc3"
    ));
}

#[test]
fn failed_wake_after_commit_delivers_once_and_never_replays() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Failure);
    delivered(fixture.publisher.publish_display(&raw(b"a")));
    delivered(fixture.publisher.publish_display(&raw(b"b")));
    assert_eq!(*fixture.steps.lock().unwrap(), [Step::Wake, Step::Wake]);
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(observed.len(), 2);
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::RawTtyBytes { bytes, .. }] if bytes == b"a"
    ));
    assert!(matches!(
        observed[1].queued.as_slice(),
        [KbInputEvent::RawTtyBytes { bytes, .. }] if bytes == b"b"
    ));
    assert!(fixture.receiver.is_empty());
}

#[test]
fn receiver_close_preserves_early_quit_and_never_notifies() {
    let Fixture {
        mut publisher,
        receiver,
        queue,
        steps,
        observations,
        ..
    } = Fixture::new(InputRoute::Tty, WakeResult::Success);
    *queue.lock().unwrap() = QueueProbe::ReceiverClosed;
    drop(receiver);
    closed(publisher.publish_display(&raw(b"\x07")));
    assert_eq!(*steps.lock().unwrap(), [Step::QuitRequested]);
    assert!(observations.lock().unwrap().is_empty());
}

#[test]
fn primary_size_is_recorded_even_when_first_core_send_fails() {
    let Fixture {
        mut publisher,
        receiver,
        queue,
        primary_size,
        ..
    } = Fixture::new(InputRoute::Tty, WakeResult::Success);
    *queue.lock().unwrap() = QueueProbe::ReceiverClosed;
    drop(receiver);
    closed(publisher.publish_display(&resize(FrameId(0), 101_u32.into(), 37_u32.into())));
    assert_eq!(
        read_primary_window_size(&primary_size),
        PrimaryWindowSize {
            width: 101,
            height: 37,
        }
    );
}

#[test]
fn converted_batch_is_fifo_with_exactly_one_post_enqueue_wake() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    delivered(fixture.publisher.publish_batch([
        KbInputEvent::raw_tty_bytes_for_terminal(b"a".to_vec(), 0),
        KbInputEvent::raw_tty_bytes_for_terminal(b"\x07".to_vec(), 0),
    ]));
    assert_eq!(
        *fixture.steps.lock().unwrap(),
        [Step::QuitRequested, Step::Wake]
    );
    let observed = fixture.observations.lock().unwrap();
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::RawTtyBytes { bytes: a, .. },
         KbInputEvent::RawTtyBytes { bytes: quit, .. }]
            if a == b"a" && quit == b"\x07"
    ));
}

#[test]
fn direct_core_input_uses_the_same_post_enqueue_wake() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    delivered(
        fixture
            .publisher
            .publish_core(KbInputEvent::raw_tty_bytes_for_terminal(
                b"already-core".to_vec(),
                0,
            )),
    );
    assert_eq!(*fixture.steps.lock().unwrap(), [Step::Wake]);
    assert_eq!(fixture.observations.lock().unwrap()[0].queued.len(), 1);
}

#[test]
fn empty_conversion_neither_requests_quit_nor_notifies() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    delivered(fixture.publisher.publish_batch(std::iter::empty()));
    assert!(fixture.steps.lock().unwrap().is_empty());
    assert!(fixture.observations.lock().unwrap().is_empty());
    assert!(fixture.receiver.is_empty());
}

#[cfg(unix)]
#[test]
fn runtime_primary_facts_share_the_existing_bridge_conversion() {
    let mut fixture = Fixture::primary(WakeResult::Success);
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
    let observed = fixture.observations.lock().unwrap();
    assert_eq!(observed.len(), 3);
    assert!(matches!(
        observed[0].queued.as_slice(),
        [KbInputEvent::RawTtyBytes {
            bytes,
            target: neovm_core::keyboard::TtyInputTarget::Terminal(0),
        }] if bytes == b"\xc3"
    ));
    assert!(matches!(
        observed[1].queued.as_slice(),
        [KbInputEvent::RawTtyBytes { bytes, .. }] if bytes == b"\xa9"
    ));
    assert!(matches!(
        observed[2].queued.as_slice(),
        [KbInputEvent::Resize {
            width: 101,
            height: 37,
            emacs_frame_id: 0,
            ..
        }]
    ));
}

#[test]
fn actual_key_release_conversion_neither_enqueues_nor_notifies() {
    let mut fixture = Fixture::new(InputRoute::Tty, WakeResult::Success);
    delivered(fixture.publisher.publish_display(&DisplayEvent::Key {
        key: neovm_core::keyboard::FrontendKey::Keysym(neovm_core::keyboard::XK_RETURN),
        modifiers: 0,
        pressed: false,
        emacs_frame_id: 0,
    }));
    assert!(fixture.steps.lock().unwrap().is_empty());
    assert!(fixture.receiver.is_empty());
}

#[cfg(unix)]
#[test]
fn fixed_primary_factory_owns_native_unbounded_terminal_zero_channel() {
    let size = Arc::new(Mutex::new(PrimaryWindowSize {
        width: 80,
        height: 24,
    }));
    let (mut publisher, receiver) =
        PrimaryInputPublisher::unbounded(size, QuitRequest::new(), None);
    assert_eq!(receiver.capacity(), None);
    delivered(publisher.publish(PrimaryTtyInput::Bytes(b"fixed-primary".to_vec())));
    assert!(matches!(
        receiver.try_recv().unwrap(),
        KbInputEvent::RawTtyBytes {
            bytes,
            target: neovm_core::keyboard::TtyInputTarget::Terminal(0),
        } if bytes == b"fixed-primary"
    ));
}
