//! Test-only publication after an observed positive backend wait entry.
//!
//! A pre-poll observation proves the wait loop has checked its input queue;
//! it does not claim that the kernel has already blocked the reader.

use crate::emacs_core::Context;
use crate::emacs_core::process::BackendWaitEntry;
use crate::keyboard::InputEvent;
use std::thread::JoinHandle;
use std::time::Duration;

const DEADLOCK_CAP: Duration = Duration::from_secs(30);

pub(crate) use crate::test_utils::{InputPublishError, InputPublisher};

/// Give timer fixtures a displayed buffer whose real edits admit redisplay.
pub(crate) fn select_visible_fixture(context: &mut Context, name: &str) {
    let buffer = context.buffers.create_buffer(name);
    context.buffers.set_current(buffer);
    let frame = context.frames.create_frame("wait-fixture", 80, 24, buffer);
    assert!(
        context.frames.select_frame(frame),
        "selected wait fixture frame"
    );
}

/// Only native facts cross to the producer thread, never Lisp Values.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WaitWitness {
    InputWaitEntered,
    EchoDecoration(String),
    TimerCompleted,
    IdleTimerCompleted,
    IdleEpoch(IdleEpoch),
    CommandKeysSnapshotCaptured,
    RetirementDrained,
    AutoSaveCompleted,
    ProcessOutputAndIdleCompleted,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum IdleEpoch {
    First,
    Second,
}

#[derive(Debug)]
pub(crate) enum Readiness {
    Pending,
    Ready(WaitWitness),
}

#[derive(Debug)]
pub(crate) struct ObservedAtWait {
    pub(crate) witness: WaitWitness,
    pub(crate) entry: BackendWaitEntry,
}

#[derive(Debug)]
enum InputCommit {
    Delivered,
    Failed,
}

#[derive(Debug)]
struct CancelPublication;

#[derive(Debug)]
enum GateOutcome {
    Observed(ObservedAtWait),
    Missing(crossbeam_channel::RecvTimeoutError),
    Cancelled,
}

/// A consumed, evaluator-local capability to arm exactly one wait observer.
#[must_use]
#[derive(Debug)]
pub(crate) struct InputGate {
    observed: crossbeam_channel::Sender<ObservedAtWait>,
    committed: crossbeam_channel::Receiver<InputCommit>,
}

/// Owns the producer, its cancellation capability and the queue keepalive.
/// Explicit finish joins and qualifies the observation; Drop only cancels.
#[must_use]
#[derive(Debug)]
pub(crate) struct InputPublication {
    worker: Option<JoinHandle<Result<GateOutcome, InputPublishError>>>,
    cancel: crossbeam_channel::Sender<CancelPublication>,
    _keepalive: InputPublisher,
}

impl InputGate {
    pub(crate) fn new(
        publisher: InputPublisher,
        events: Vec<InputEvent>,
    ) -> (Self, InputPublication) {
        let (observed, observations) = crossbeam_channel::bounded(1);
        let (commit, committed) = crossbeam_channel::bounded(1);
        let (cancel, cancelled) = crossbeam_channel::bounded(1);
        let keepalive = publisher.clone();
        let worker = std::thread::spawn(move || {
            let outcome = crossbeam_channel::select! {
                recv(observations) -> observation => match observation {
                    Ok(observation) => GateOutcome::Observed(observation),
                    Err(_) => GateOutcome::Missing(crossbeam_channel::RecvTimeoutError::Disconnected),
                },
                recv(cancelled) -> _ => return Ok(GateOutcome::Cancelled),
                default(DEADLOCK_CAP) => GateOutcome::Missing(crossbeam_channel::RecvTimeoutError::Timeout),
            };
            match outcome {
                GateOutcome::Observed(observation) => {
                    // Only an owned observation authorizes consuming the
                    // normal input batch. Publication is never retried.
                    let published = publisher.send_batch(events);
                    let acknowledgement = match &published {
                        Ok(()) => InputCommit::Delivered,
                        Err(_) => InputCommit::Failed,
                    };
                    let _ = commit.send(acknowledgement);
                    published?;
                    Ok(GateOutcome::Observed(observation))
                }
                GateOutcome::Missing(reason) => {
                    publisher.cancel_wait()?;
                    Ok(GateOutcome::Missing(reason))
                }
                GateOutcome::Cancelled => Ok(GateOutcome::Cancelled),
            }
        });
        (
            Self {
                observed,
                committed,
            },
            InputPublication {
                worker: Some(worker),
                cancel,
                _keepalive: keepalive,
            },
        )
    }

    pub(crate) fn arm(self, context: &mut Context, witness: WaitWitness) {
        context
            .processes
            .before_next_backend_wait_for_test(move |entry| {
                self.observed
                    .send(ObservedAtWait { witness, entry })
                    .expect("producer owns the wait observation receiver");
                match self
                    .committed
                    .recv_timeout(DEADLOCK_CAP)
                    .expect("producer commits one input batch before pre-poll returns")
                {
                    InputCommit::Delivered => {}
                    InputCommit::Failed => panic!("test input publication failed"),
                }
            });
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WaitPublicationError {
    #[error("test producer join resource already consumed")]
    AlreadyFinished,
    #[error("test producer thread panicked")]
    WorkerPanicked,
    #[error(transparent)]
    Publish(#[from] InputPublishError),
    #[error("required wait witness absent: {0}")]
    Missing(crossbeam_channel::RecvTimeoutError),
    #[error("required wait publication was cancelled")]
    Cancelled,
}

impl InputPublication {
    pub(crate) fn finish(mut self) -> Result<ObservedAtWait, WaitPublicationError> {
        let outcome = self
            .worker
            .take()
            .ok_or(WaitPublicationError::AlreadyFinished)?
            .join()
            .map_err(|_| WaitPublicationError::WorkerPanicked)??;
        match outcome {
            GateOutcome::Observed(observation) => Ok(observation),
            GateOutcome::Missing(reason) => Err(WaitPublicationError::Missing(reason)),
            GateOutcome::Cancelled => Err(WaitPublicationError::Cancelled),
        }
    }
}

impl Drop for InputPublication {
    fn drop(&mut self) {
        let _ = self.cancel.try_send(CancelPublication);
    }
}

#[derive(Debug)]
enum SemanticGate {
    Waiting(InputGate),
    Armed,
}

/// Preserve the fixture's existing redisplay callback and consume the gate
/// after its exact semantic fact becomes visible. The next positive wait,
/// rather than this callback or a service-only zero poll, releases input.
pub(crate) fn install_after_redisplay(
    context: &mut Context,
    gate: InputGate,
    mut readiness: impl FnMut(&mut Context) -> Readiness + 'static,
) {
    let mut previous = context.redisplay_fn.take();
    let mut state = SemanticGate::Waiting(gate);
    context.redisplay_fn = Some(Box::new(move |context| {
        if let Some(previous) = &mut previous {
            previous(context);
        }
        match std::mem::replace(&mut state, SemanticGate::Armed) {
            SemanticGate::Waiting(gate) => match readiness(context) {
                Readiness::Pending => state = SemanticGate::Waiting(gate),
                Readiness::Ready(witness) => gate.arm(context, witness),
            },
            SemanticGate::Armed => {}
        }
    }));
}

static_assertions::assert_impl_all!(InputPublisher: Send, Sync, Clone, std::fmt::Debug);
static_assertions::assert_impl_all!(InputGate: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(InputGate: Clone, Copy);
static_assertions::assert_impl_all!(InputPublication: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(InputPublication: Clone, Copy);
static_assertions::assert_impl_all!(ObservedAtWait: Send, std::fmt::Debug);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyboard::KeyEvent;

    #[test]
    fn zero_service_poll_retains_gate_until_positive_input_wait() {
        let mut context = Context::new();
        let publisher = InputPublisher::install(&mut context);
        let (gate, publication) = InputGate::new(
            publisher.clone(),
            vec![InputEvent::key_press(KeyEvent::char('x'))],
        );
        gate.arm(&mut context, WaitWitness::InputWaitEntered);
        context
            .processes
            .wait_for_backend_events(
                Duration::ZERO,
                crate::emacs_core::process::ProcessWaitBackendInterest::NotificationsOnly,
            )
            .expect("native wait backend");
        assert!(
            matches!(
                context.input_rx.as_ref().expect("receiver").try_recv(),
                Err(crossbeam_channel::TryRecvError::Empty)
            ),
            "zero service poll must not authorize input"
        );
        let completion = context
            .wait_for_command_input(Some(std::time::Instant::now() + Duration::from_secs(1)));
        let observed = publication
            .finish()
            .expect("positive wait authorizes input");
        assert_eq!(observed.witness, WaitWitness::InputWaitEntered);
        assert!(observed.entry.requested_timeout() > Duration::ZERO);
        assert_eq!(context.command_loop.keyboard.pending_input_events.len(), 1);
        assert_eq!(
            completion.expect("command-input wait"),
            crate::emacs_core::wait::CommandInputWaitOutcome::InputPending
        );
        assert_eq!(
            context.read_char().expect("single queued key"),
            crate::emacs_core::value::Value::fixnum('x' as i64)
        );
        assert_eq!(context.command_loop.keyboard.pending_input_events.len(), 0);
    }

    #[test]
    fn missing_gate_cancels_without_publishing_the_normal_key() {
        let mut context = Context::new();
        let publisher = InputPublisher::install(&mut context);
        let (gate, publication) = InputGate::new(
            publisher.clone(),
            vec![InputEvent::key_press(KeyEvent::char('x'))],
        );
        // An explicitly dropped, never-armed gate is a deterministic missing
        // witness; it does not wait for the deadlock cap.
        drop(gate);
        assert!(matches!(
            publication.finish(),
            Err(WaitPublicationError::Missing(
                crossbeam_channel::RecvTimeoutError::Disconnected
            ))
        ));
        assert!(
            context.quit_requested.is_requested(),
            "failure-only cancellation requested"
        );
        assert!(
            matches!(
                context.input_rx.as_ref().expect("receiver").try_recv(),
                Err(crossbeam_channel::TryRecvError::Empty)
            ),
            "no normal input without a witness"
        );
        context.quit_requested.clear();
    }

    #[test]
    fn dropping_publication_cancels_its_waiting_producer_without_input() {
        let mut context = Context::new();
        let publisher = InputPublisher::install(&mut context);
        let (gate, mut publication) = InputGate::new(
            publisher.clone(),
            vec![InputEvent::key_press(KeyEvent::char('x'))],
        );
        // Retain the join resource in this control while exercising exactly
        // the nonblocking Drop cancellation path.
        let worker = publication.worker.take().expect("worker owner");
        drop(publication);
        let outcome = worker
            .join()
            .expect("producer thread")
            .expect("cancelled producer");
        assert!(matches!(outcome, GateOutcome::Cancelled));
        assert!(!context.quit_requested.is_requested());
        assert!(matches!(
            context.input_rx.as_ref().expect("receiver").try_recv(),
            Err(crossbeam_channel::TryRecvError::Empty)
        ));
        drop(gate);
    }
}
