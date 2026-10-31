use super::*;
use crate::emacs_core::process::ProcessWaitBackendInterest;
use crate::keyboard::KeyEvent;
use crossbeam_channel::TryRecvError;
use std::cell::RefCell;
use std::io::ErrorKind;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[derive(Clone, Copy)]
enum ProbeWake {
    Notified,
    Failed(ErrorKind),
}

/// Owns no sending capability: this observes only the wake side of publication.
#[derive(Clone)]
pub(super) struct WakeProbe {
    outcome: ProbeWake,
    notifications: Arc<AtomicUsize>,
    observation: Arc<OnceLock<Arc<dyn Fn() + Send + Sync>>>,
}

impl WakeProbe {
    fn new(outcome: ProbeWake) -> Self {
        Self {
            outcome,
            notifications: Arc::new(AtomicUsize::new(0)),
            observation: Arc::new(OnceLock::new()),
        }
    }

    fn observe(&self, observe: impl Fn() + Send + Sync + 'static) {
        assert!(
            self.observation.set(Arc::new(observe)).is_ok(),
            "wake observation is installed once"
        );
    }

    fn count(&self) -> usize {
        self.notifications.load(Ordering::SeqCst)
    }

    pub(super) fn notify(&self) -> HostInputWakeOutcome {
        self.notifications.fetch_add(1, Ordering::SeqCst);
        if let Some(observe) = self.observation.get() {
            observe();
        }
        match self.outcome {
            ProbeWake::Notified => HostInputWakeOutcome::Notified,
            ProbeWake::Failed(kind) => HostInputWakeOutcome::Failed(std::io::Error::new(
                kind,
                "injected native input wake failure",
            )),
        }
    }
}

static_assertions::assert_impl_all!(WakeProbe: Send, Sync, Clone);

fn key(character: char) -> InputEvent {
    InputEvent::key_press(KeyEvent::char(character))
}

fn assert_key(event: InputEvent, character: char) {
    assert!(
        matches!(event, InputEvent::KeyPress { key, .. } if key == KeyEvent::char(character)),
        "expected key {character:?}"
    );
}

#[test]
fn empty_batch_never_wakes() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));

    publisher
        .send_batch(std::iter::empty())
        .expect("empty batch");

    assert_eq!(probe.count(), 0);
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn single_event_is_queued_before_exactly_one_wake() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));
    let observed = Arc::new(Mutex::new(Vec::new()));
    let observation = Arc::clone(&observed);
    let observing_receiver = receiver.clone();
    probe.observe(move || {
        observation
            .lock()
            .expect("publication observation lock")
            .extend(observing_receiver.try_iter());
    });

    publisher.send(key('a')).expect("single publication");

    assert_eq!(probe.count(), 1);
    let mut observed = observed.lock().expect("publication observation lock");
    assert_eq!(
        observed.len(),
        1,
        "event must already be visible during wake"
    );
    assert_key(observed.pop().expect("event visible at wake"), 'a');
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    drop(observed);
}

#[test]
fn batch_preserves_fifo_with_exactly_one_wake() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));

    publisher
        .send_batch(['a', 'b', 'c'].map(key))
        .expect("batch publication");

    assert_eq!(probe.count(), 1);
    for character in ['a', 'b', 'c'] {
        assert_key(
            receiver.try_recv().expect("committed batch event"),
            character,
        );
    }
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
}

enum ReceiverClosingStep {
    First(Receiver<InputEvent>),
    CloseAfterPrefix(Receiver<InputEvent>),
    Finished,
}

/// Disconnect precisely between the first and second enqueue, without a timer
/// or an independently accessible sender. Retain the prefix as an observation.
fn close_receiver_before_second_event(
    receiver: Receiver<InputEvent>,
    prefix: Rc<RefCell<Vec<InputEvent>>>,
) -> impl Iterator<Item = InputEvent> {
    let mut step = ReceiverClosingStep::First(receiver);
    std::iter::from_fn(
        move || match std::mem::replace(&mut step, ReceiverClosingStep::Finished) {
            ReceiverClosingStep::First(receiver) => {
                step = ReceiverClosingStep::CloseAfterPrefix(receiver);
                Some(key('a'))
            }
            ReceiverClosingStep::CloseAfterPrefix(receiver) => {
                prefix
                    .borrow_mut()
                    .push(receiver.try_recv().expect("first event already committed"));
                drop(receiver);
                Some(key('b'))
            }
            ReceiverClosingStep::Finished => None,
        },
    )
}

#[test]
fn receiver_close_after_prefix_reports_commit_and_wakes_once() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));
    let prefix = Rc::new(RefCell::new(Vec::new()));

    let result = publisher.send_batch(close_receiver_before_second_event(
        receiver,
        Rc::clone(&prefix),
    ));

    assert!(matches!(
        result,
        Err(HostInputPublishError::ReceiverClosedAfterPrefix {
            wake: HostInputWakeOutcome::Notified
        })
    ));
    assert_eq!(probe.count(), 1);
    let mut prefix = prefix.borrow_mut();
    assert_eq!(prefix.len(), 1);
    assert_key(prefix.pop().expect("observed committed prefix"), 'a');
}

#[test]
fn receiver_close_after_prefix_retains_simultaneous_wake_failure() {
    let probe = WakeProbe::new(ProbeWake::Failed(ErrorKind::BrokenPipe));
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));
    let prefix = Rc::new(RefCell::new(Vec::new()));

    let result = publisher.send_batch(close_receiver_before_second_event(
        receiver,
        Rc::clone(&prefix),
    ));

    assert!(matches!(
        result,
        Err(HostInputPublishError::ReceiverClosedAfterPrefix {
            wake: HostInputWakeOutcome::Failed(error)
        }) if error.kind() == ErrorKind::BrokenPipe
    ));
    assert_eq!(probe.count(), 1, "failed notification must not be retried");
    let mut prefix = prefix.borrow_mut();
    assert_eq!(prefix.len(), 1);
    assert_key(prefix.pop().expect("observed committed prefix"), 'a');
}

#[test]
fn postcommit_wake_failure_does_not_replay_or_retry() {
    let probe = WakeProbe::new(ProbeWake::Failed(ErrorKind::BrokenPipe));
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));

    let result = publisher.send_batch(['a', 'b'].map(key));

    assert!(matches!(
        result,
        Err(HostInputPublishError::WakeAfterCommit(error))
            if error.kind() == ErrorKind::BrokenPipe
    ));
    assert_eq!(
        probe.count(),
        1,
        "postcommit failure disarms the prefix guard"
    );
    for character in ['a', 'b'] {
        assert_key(
            receiver.try_recv().expect("committed despite failed wake"),
            character,
        );
    }
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    publisher
        .send_batch(std::iter::empty())
        .expect("empty batch");
    assert_eq!(probe.count(), 1);
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
}

#[derive(Clone, Copy)]
enum UnwindingIteratorStep {
    First,
    PanicAfterPrefix,
}

#[test]
fn iterator_unwind_wakes_committed_prefix_exactly_once() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));
    let mut step = UnwindingIteratorStep::First;
    let events = std::iter::from_fn(move || match step {
        UnwindingIteratorStep::First => {
            step = UnwindingIteratorStep::PanicAfterPrefix;
            Some(key('a'))
        }
        UnwindingIteratorStep::PanicAfterPrefix => panic!("injected input iterator unwind"),
    });

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        publisher.send_batch(events)
    }));

    assert!(result.is_err());
    assert_eq!(probe.count(), 1);
    assert_key(receiver.try_recv().expect("prefix survives unwind"), 'a');
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
}

#[derive(Debug, PartialEq, Eq)]
enum PrefixUnwindObservation {
    PrefixNotified,
    IteratorDropped,
}

struct DropObservingInputIterator {
    step: UnwindingIteratorStep,
    observations: Arc<Mutex<Vec<PrefixUnwindObservation>>>,
}

impl Iterator for DropObservingInputIterator {
    type Item = InputEvent;

    fn next(&mut self) -> Option<Self::Item> {
        match self.step {
            UnwindingIteratorStep::First => {
                self.step = UnwindingIteratorStep::PanicAfterPrefix;
                Some(key('a'))
            }
            UnwindingIteratorStep::PanicAfterPrefix => {
                panic!("injected input iterator unwind for drop ordering")
            }
        }
    }
}

impl Drop for DropObservingInputIterator {
    fn drop(&mut self) {
        self.observations
            .lock()
            .expect("iterator drop observation lock")
            .push(PrefixUnwindObservation::IteratorDropped);
    }
}

#[test]
fn iterator_unwind_notifies_committed_prefix_before_iterator_destructor() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));
    let observations = Arc::new(Mutex::new(Vec::new()));
    let waking_observations = Arc::clone(&observations);
    probe.observe(move || {
        waking_observations
            .lock()
            .expect("prefix wake observation lock")
            .push(PrefixUnwindObservation::PrefixNotified);
    });
    let events = DropObservingInputIterator {
        step: UnwindingIteratorStep::First,
        observations: Arc::clone(&observations),
    };

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        publisher.send_batch(events)
    }));

    assert!(result.is_err());
    assert_eq!(probe.count(), 1, "only the committed prefix is notified");
    assert_eq!(
        *observations.lock().expect("unwind observations lock"),
        vec![
            PrefixUnwindObservation::PrefixNotified,
            PrefixUnwindObservation::IteratorDropped,
        ],
        "the prefix must wake before arbitrary iterator destruction"
    );
    assert_key(receiver.try_recv().expect("prefix survives unwind"), 'a');
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn receiver_closed_before_first_event_does_not_wake() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));
    drop(receiver);

    let result = publisher.send(key('a'));

    assert!(matches!(result, Err(HostInputPublishError::ReceiverClosed)));
    assert_eq!(probe.count(), 0);
}

#[test]
fn closed_publisher_rejects_events_and_accepts_empty_batch() {
    let publisher = HostInputPublisher::closed();

    assert!(matches!(
        publisher.send(key('a')),
        Err(HostInputPublishError::ReceiverClosed)
    ));
    publisher
        .send_batch(std::iter::empty())
        .expect("empty closed batch");
}

#[derive(Debug)]
enum DisconnectObservation {
    Unobserved,
    ReceiverStillConnected,
    DisconnectedAfterTail(Vec<InputEvent>),
}

#[test]
fn only_final_publisher_drop_wakes_after_sender_disconnect_and_buffered_tail() {
    let probe = WakeProbe::new(ProbeWake::Notified);
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::Observed(probe.clone()));
    let clone = publisher.clone();
    drop(clone);
    assert_eq!(probe.count(), 0, "dropping a nonfinal clone cannot wake");
    publisher.send(key('a')).expect("buffer a tail event");
    assert_eq!(probe.count(), 1);

    // Observe only the final connection drop. The receiving half cannot keep
    // the sender alive and must see its buffered tail before disconnection.
    let observed = Arc::new(Mutex::new(DisconnectObservation::Unobserved));
    let observation = Arc::clone(&observed);
    probe.observe(move || {
        let mut tail = Vec::new();
        let result = loop {
            match receiver.try_recv() {
                Ok(event) => tail.push(event),
                Err(TryRecvError::Empty) => break DisconnectObservation::ReceiverStillConnected,
                Err(TryRecvError::Disconnected) => {
                    break DisconnectObservation::DisconnectedAfterTail(tail);
                }
            }
        };
        *observation.lock().expect("disconnect observation lock") = result;
    });

    drop(publisher);

    assert_eq!(probe.count(), 2, "the final owner adds exactly one wake");
    let mut observed = observed.lock().expect("disconnect observation lock");
    let result = std::mem::replace(&mut *observed, DisconnectObservation::Unobserved);
    match result {
        DisconnectObservation::DisconnectedAfterTail(mut tail) => {
            assert_eq!(tail.len(), 1, "only the committed tail precedes closure");
            assert_key(tail.pop().expect("buffered event before disconnect"), 'a');
        }
        DisconnectObservation::Unobserved => panic!("final drop did not notify"),
        DisconnectObservation::ReceiverStillConnected => {
            panic!("disconnect notification ran before the sender was dropped");
        }
    }
}

#[test]
fn installing_host_input_preserves_running_state_until_explicit_activation() {
    let mut context = Context::new();
    let running = context.command_loop.running;
    assert!(context.input_rx.as_ref().is_none());

    let _publisher = HostInputPublisher::install(&mut context);

    assert!(context.input_rx.as_ref().is_some());
    assert_eq!(context.command_loop.running, running);
    context.init_input_system();
    assert!(context.command_loop.running);
}

#[test]
fn context_host_input_connection_is_unbounded() {
    let mut context = Context::new();
    let _publisher = HostInputPublisher::install(&mut context);
    assert_eq!(
        context
            .input_rx
            .as_ref()
            .expect("context-owned host receiver")
            .capacity(),
        None
    );
}

#[test]
fn context_replacement_preserves_staged_events_and_discards_old_raw_tail() {
    let mut context = Context::new();
    let old = HostInputPublisher::install(&mut context);
    context
        .command_loop
        .keyboard
        .pending_input_events
        .push_back(key('s'));
    old.send(key('o')).expect("queue old unread channel tail");

    let current = HostInputPublisher::install(&mut context);

    assert_eq!(context.command_loop.keyboard.pending_input_events.len(), 1);
    assert_key(
        context
            .command_loop
            .keyboard
            .pending_input_events
            .pop_visible_front()
            .expect("staged old event remains owned by Context"),
        's',
    );
    assert!(matches!(
        old.send(key('x')),
        Err(HostInputPublishError::ReceiverClosed)
    ));
    assert!(matches!(
        context.receive_host_input_for_test(Duration::ZERO),
        Err(crossbeam_channel::RecvTimeoutError::Timeout)
    ));
    current.send(key('n')).expect("queue replacement input");
    assert_key(
        context
            .receive_host_input_for_test(Duration::ZERO)
            .expect("new connection event"),
        'n',
    );
    assert!(matches!(
        context.receive_host_input_for_test(Duration::ZERO),
        Err(crossbeam_channel::RecvTimeoutError::Timeout)
    ));
}

#[test]
fn publication_at_native_wait_entry_wakes_without_other_activity() {
    let mut context = Context::new();
    assert!(context.processes.has_wait_notification_backend());
    let publisher = HostInputPublisher::install(&mut context);
    // Keep the connection alive through the wait: final publisher drop must
    // never rescue a mutant with the enqueue notification removed.
    let worker_publisher = publisher.clone();
    let (entered_tx, entered_rx) = crossbeam_channel::bounded(1);
    let (published_tx, published_rx) = crossbeam_channel::bounded(1);
    let timeout = Duration::from_secs(1);
    context
        .processes
        .before_next_backend_wait_for_test(move |entry| {
            assert_eq!(entry.requested_timeout(), timeout);
            entered_tx
                .send(())
                .expect("release producer at real native wait entry");
            published_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("producer commits before entering native wait");
        });
    let worker = std::thread::spawn(move || {
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("native wait entry reached");
        let result = worker_publisher.send(key('a'));
        published_tx.send(()).expect("acknowledge publication");
        result
    });

    let events = context
        .processes
        .wait_for_backend_events(timeout, ProcessWaitBackendInterest::NotificationsOnly);
    let publication = worker.join().expect("input publication worker joins");

    publication.expect("native publication succeeds");
    assert!(
        events
            .expect("fresh native backend remains available")
            .has_notification_wakeup(),
        "published input must wake the native backend; a finite timeout is not notification"
    );
    assert_key(
        context
            .receive_host_input_for_test(Duration::ZERO)
            .expect("exact event survives native wait"),
        'a',
    );
    assert!(matches!(
        context.receive_host_input_for_test(Duration::ZERO),
        Err(crossbeam_channel::RecvTimeoutError::Timeout)
    ));
    drop(publisher);
}

#[test]
fn final_publisher_drop_at_native_wait_entry_wakes_disconnected_receiver() {
    let mut context = Context::new();
    assert!(context.processes.has_wait_notification_backend());
    let publisher = HostInputPublisher::install(&mut context);
    let (entered_tx, entered_rx) = crossbeam_channel::bounded(1);
    let (closed_tx, closed_rx) = crossbeam_channel::bounded(1);
    let timeout = Duration::from_secs(1);
    context
        .processes
        .before_next_backend_wait_for_test(move |entry| {
            assert_eq!(entry.requested_timeout(), timeout);
            entered_tx.send(()).expect("release final publisher owner");
            closed_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("sender retired before native wait");
        });
    let worker = std::thread::spawn(move || {
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("native wait reached");
        drop(publisher);
        closed_tx.send(()).expect("acknowledge closed input");
    });

    let events = context
        .processes
        .wait_for_backend_events(timeout, ProcessWaitBackendInterest::NotificationsOnly);
    worker.join().expect("final input owner joins");

    assert!(
        events.expect("native backend").has_notification_wakeup(),
        "last publisher must wake a poller, not only a channel receiver"
    );
    assert!(matches!(
        context.receive_host_input_for_test(Duration::ZERO),
        Err(crossbeam_channel::RecvTimeoutError::Disconnected)
    ));
}

#[test]
fn no_poller_channel_fallback_delivers_without_a_separate_wake_backend() {
    let (publisher, receiver) = HostInputPublisher::channel(InputWake::ChannelFallback);
    publisher
        .send(key('f'))
        .expect("channel fallback publication");
    assert_key(receiver.try_recv().expect("fallback event"), 'f');
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    drop(publisher);
    assert!(matches!(
        receiver.try_recv(),
        Err(TryRecvError::Disconnected)
    ));
}

#[test]
fn control_nudge_is_value_free_and_uses_the_receiving_owner_session() {
    use crate::host_effect::wake::{ControlWake, ControlWakeOutcome};
    let (wake, _) = ControlWake::paired();
    let (publisher, receiver) = HostInputPublisher::channel_with_session(
        InputWake::ChannelFallback,
        Some(wake.attach_host_session()),
    );
    let wait = wake.arm(ControlDestination::NativeMain).unwrap();
    assert!(matches!(
        publisher.nudge_control(ControlNudge::MainMailboxReady),
        ControlNudgeOutcome::Notified
    ));
    assert_eq!(
        wait.poll(),
        Some(ControlWakeOutcome::ControlReady(
            ControlNudge::MainMailboxReady
        ))
    );
    assert!(matches!(
        receiver.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
    wake.retire();
    assert!(matches!(
        publisher.nudge_control(ControlNudge::MainMailboxReady),
        ControlNudgeOutcome::RetiredSession
    ));
    assert!(matches!(
        receiver.try_recv(),
        Err(crossbeam_channel::TryRecvError::Empty)
    ));
}

#[test]
fn replacement_rejects_old_control_nudges_and_old_disconnect_without_waking_successor() {
    use crate::host_effect::wake::ControlWakeOutcome;
    let mut context = Context::new();
    let old = HostInputPublisher::install(&mut context);
    let latch = context.processes.host_control_latch();
    let wake = Arc::clone(latch.control().expect("host session owns a control source"));
    let current = HostInputPublisher::install(&mut context);
    let main = wake.arm(ControlDestination::NativeMain).unwrap();
    let idle = wake.arm(ControlDestination::IdleScheduler).unwrap();
    assert!(matches!(
        old.nudge_control(ControlNudge::MainMailboxReady),
        ControlNudgeOutcome::RetiredSession
    ));
    assert!(main.poll().is_none());
    assert!(idle.poll().is_none());
    // The stale disconnect must also avoid the ordinary input bit/notify;
    // unchanged native notify code is reserved for the current connection.
    latch.swap(false, Ordering::AcqRel);
    drop(old);
    assert!(!latch.load(Ordering::Acquire));
    assert!(main.poll().is_none());
    assert!(idle.poll().is_none());
    assert!(matches!(
        current.nudge_control(ControlNudge::MainMailboxReady),
        ControlNudgeOutcome::Notified
    ));
    assert_eq!(
        main.poll(),
        Some(ControlWakeOutcome::ControlReady(
            ControlNudge::MainMailboxReady
        ))
    );
    assert!(matches!(
        context.receive_host_input_for_test(Duration::ZERO),
        Err(crossbeam_channel::RecvTimeoutError::Timeout)
    ));
}

#[test]
fn explicit_host_session_revocation_preserves_backend_control_and_input_installation() {
    use crate::host_effect::wake::ControlWakeOutcome;
    let mut context = Context::new();
    let publisher = HostInputPublisher::install(&mut context);
    let latch = context.processes.host_control_latch();
    let wake = Arc::clone(latch.control().expect("host session owns a control source"));
    context.processes.revoke_host_control_session();
    let registration = wake.arm(ControlDestination::NativeMain).unwrap();
    assert!(matches!(
        publisher.nudge_control(ControlNudge::Cancellation),
        ControlNudgeOutcome::RetiredSession
    ));
    assert!(registration.poll().is_none());
    assert!(matches!(
        wake.handle().nudge(
            ControlDestination::NativeMain,
            ControlNudge::HostServiceReady
        ),
        ControlNudgeOutcome::Notified
    ));
    assert_eq!(
        registration.poll(),
        Some(ControlWakeOutcome::ControlReady(
            ControlNudge::HostServiceReady
        ))
    );
    publisher
        .send(key('r'))
        .expect("revocation does not disable physical input");
    assert_key(
        context.receive_host_input_for_test(Duration::ZERO).unwrap(),
        'r',
    );
}

#[test]
fn final_current_connection_drop_wakes_both_destinations_and_revokes_only_its_session() {
    use crate::host_effect::wake::{ControlWake, ControlWakeOutcome};
    let (wake, _) = ControlWake::paired();
    let session = wake.attach_host_session();
    let retained_session = session.clone();
    let (publisher, receiver) =
        HostInputPublisher::channel_with_session(InputWake::ChannelFallback, Some(session));
    let main = wake.arm(ControlDestination::NativeMain).unwrap();
    let idle = wake.arm(ControlDestination::IdleScheduler).unwrap();
    drop(publisher);
    assert!(matches!(
        receiver.try_recv(),
        Err(TryRecvError::Disconnected)
    ));
    assert_eq!(
        main.poll(),
        Some(ControlWakeOutcome::ControlReady(ControlNudge::HostClosing))
    );
    assert_eq!(
        idle.poll(),
        Some(ControlWakeOutcome::ControlReady(ControlNudge::HostClosing))
    );
    assert!(matches!(
        retained_session.nudge(
            ControlDestination::NativeMain,
            ControlNudge::MainMailboxReady
        ),
        ControlNudgeOutcome::RetiredSession
    ));
    let successor = wake.attach_host_session();
    let successor_wait = wake.arm(ControlDestination::NativeMain).unwrap();
    for outcome in retained_session.disconnect() {
        assert!(matches!(outcome, ControlNudgeOutcome::RetiredSession));
    }
    assert!(successor_wait.poll().is_none());
    assert!(matches!(
        successor.nudge(
            ControlDestination::NativeMain,
            ControlNudge::MainMailboxReady
        ),
        ControlNudgeOutcome::Notified
    ));
}

#[test]
fn public_late_host_install_wakes_the_old_armed_select_before_the_first_key() {
    use crate::host_effect::wake::ControlWakeOutcome;
    use std::time::Instant;

    let mut context = Context::new();
    let old_select = context
        .processes
        .green_select_handle()
        .expect("native Green readiness before host installation");
    let old_registration = old_select.arm().expect("old idle wait arm");
    assert!(old_registration.poll().is_none());
    assert!(context.input_rx.as_ref().is_none());

    let publisher = HostInputPublisher::install(&mut context);
    assert!(context.input_rx.as_ref().is_some());
    assert_eq!(
        old_registration.poll(),
        Some(ControlWakeOutcome::ControlReady(ControlNudge::HostAttached))
    );
    assert!(
        old_select.wait_registered(&old_registration, Instant::now()),
        "installation must wake the already armed old idle source before any key"
    );
    assert!(matches!(
        context.receive_host_input_for_test(Duration::ZERO),
        Err(crossbeam_channel::RecvTimeoutError::Timeout)
    ));
    publisher.send(key('i')).expect("immediate first key");
    assert_key(
        context.receive_host_input_for_test(Duration::ZERO).unwrap(),
        'i',
    );
}

#[test]
fn negative_drop_only_install_nudge_leaves_old_select_unready_until_a_key() {
    use std::time::Instant;

    let mut context = Context::new();
    let old_select = context
        .processes
        .green_select_handle()
        .expect("native Green readiness before host installation");
    let old_registration = old_select.arm().expect("old idle wait arm");
    // Exact source mutation: install's canonical constructor still publishes
    // the same session and receiver; only announce_install is omitted.
    let publisher = HostInputPublisher::connect(&mut context);
    assert!(context.input_rx.as_ref().is_some());
    assert!(old_registration.poll().is_none());
    assert!(
        !old_select.wait_registered(&old_registration, Instant::now()),
        "without the install nudge, the original source must have no readiness"
    );
    assert!(matches!(
        context.receive_host_input_for_test(Duration::ZERO),
        Err(crossbeam_channel::RecvTimeoutError::Timeout)
    ));
    // Recovery uses the unchanged notifying publication path. No real waiter
    // is stranded by this zero-time negative control.
    publisher.send(key('r')).expect("recovery key");
    assert!(old_select.wait_registered(&old_registration, Instant::now()));
    assert_key(
        context.receive_host_input_for_test(Duration::ZERO).unwrap(),
        'r',
    );
}
