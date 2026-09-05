//! Bounded cross-thread event delivery with a lossless control plane.
//!
//! Filesystem bursts are external input and must not grow evaluator memory
//! without bound.  Producers therefore publish into a fixed-capacity queue.
//! Once full, an atomic latch records that consumers must conservatively
//! rescan. Terminal lifecycle records and fatal worker failures use a separate
//! unbounded control channel, so correctness never competes with discardable
//! data-plane events for capacity.

use crate::emacs_core::process::WaitNotifier;
use crossbeam_channel::{Receiver, Sender, TrySendError};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

pub(super) const EVENT_CAPACITY: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublishOutcome {
    Published,
    Overflowed,
    Closed,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DeliveryRecord<T, Control> {
    Event(T),
    Control(Control),
}

struct Sequenced<T> {
    sequence: u128,
    value: T,
}

#[derive(Default)]
struct PublicationOrder {
    next: u128,
}

impl PublicationOrder {
    fn issue(&mut self) -> u128 {
        let sequence = self.next;
        self.next = self
            .next
            .checked_add(1)
            .expect("file notification publication sequence exhausted");
        sequence
    }
}

pub(super) struct DeliverySender<T, Control> {
    sender: Sender<Sequenced<T>>,
    controls: Sender<Sequenced<Control>>,
    overflowed: Arc<AtomicBool>,
    publication_order: Arc<Mutex<PublicationOrder>>,
    notifier: Option<WaitNotifier>,
}

impl<T, Control> Clone for DeliverySender<T, Control> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            controls: self.controls.clone(),
            overflowed: Arc::clone(&self.overflowed),
            publication_order: Arc::clone(&self.publication_order),
            notifier: self.notifier.clone(),
        }
    }
}

pub(super) struct DeliveryReceiver<T, Control> {
    receiver: Receiver<Sequenced<T>>,
    controls: Receiver<Sequenced<Control>>,
    overflowed: Arc<AtomicBool>,
    publication_order: Arc<Mutex<PublicationOrder>>,
    event_capacity: usize,
}

pub(super) struct DeliveryBatch<T, Control> {
    pub(super) records: Vec<DeliveryRecord<T, Control>>,
    pub(super) overflowed: bool,
}

pub(super) fn channel<T, Control>(
    notifier: Option<WaitNotifier>,
) -> (DeliverySender<T, Control>, DeliveryReceiver<T, Control>) {
    channel_with_capacity(EVENT_CAPACITY, notifier)
}

fn channel_with_capacity<T, Control>(
    capacity: usize,
    notifier: Option<WaitNotifier>,
) -> (DeliverySender<T, Control>, DeliveryReceiver<T, Control>) {
    let (sender, receiver) = crossbeam_channel::bounded(capacity);
    let (control_sender, control_receiver) = crossbeam_channel::unbounded();
    let overflowed = Arc::new(AtomicBool::new(false));
    let publication_order = Arc::new(Mutex::new(PublicationOrder::default()));
    (
        DeliverySender {
            sender,
            controls: control_sender,
            overflowed: Arc::clone(&overflowed),
            publication_order: Arc::clone(&publication_order),
            notifier,
        },
        DeliveryReceiver {
            receiver,
            controls: control_receiver,
            overflowed,
            publication_order,
            event_capacity: capacity,
        },
    )
}

impl<T, Control> DeliverySender<T, Control> {
    pub(super) fn publish(&self, item: T) -> PublishOutcome {
        let mut order = self.lock_publication_order();
        let item = Sequenced {
            sequence: order.issue(),
            value: item,
        };
        let outcome = match self.sender.try_send(item) {
            Ok(()) => PublishOutcome::Published,
            Err(TrySendError::Full(_)) => {
                self.overflowed.store(true, Ordering::Release);
                PublishOutcome::Overflowed
            }
            Err(TrySendError::Disconnected(_)) => PublishOutcome::Closed,
        };
        drop(order);
        if outcome != PublishOutcome::Closed {
            self.notify_evaluator();
        }
        outcome
    }

    /// Publish lifecycle state without event-queue backpressure, expose the
    /// corresponding state transition, and then wake the evaluator. Drains
    /// retire registrations only from these records, never from the atomic
    /// state alone, so racing a poll cannot lose the transition.
    pub(super) fn publish_control(
        &self,
        control: Control,
        commit: impl FnOnce(),
    ) -> PublishOutcome {
        let mut order = self.lock_publication_order();
        let control = Sequenced {
            sequence: order.issue(),
            value: control,
        };
        let connected = self.controls.send(control).is_ok();
        commit();
        drop(order);
        if connected {
            self.notify_evaluator();
            PublishOutcome::Published
        } else {
            PublishOutcome::Closed
        }
    }

    /// Publish a final control record and consume this worker's sender.
    /// Fatal paths use this operation, making "report once, then exit" a
    /// compile-time property for each worker-owned sender.
    pub(super) fn finish_with(self, control: Control, commit: impl FnOnce()) {
        let _ = self.publish_control(control, commit);
    }

    fn notify_evaluator(&self) {
        if let Some(notifier) = self.notifier.as_ref()
            && let Err(error) = notifier.notify()
        {
            tracing::error!(%error, "failed to wake evaluator for file notification");
        }
    }

    fn lock_publication_order(&self) -> MutexGuard<'_, PublicationOrder> {
        self.publication_order
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<T, Control> DeliveryReceiver<T, Control> {
    /// Take one retirement-safe snapshot of both delivery planes.
    ///
    /// The final publication lock gives the evaluator a consistent cut across
    /// both channels. Re-draining data within that cut captures every record
    /// published before a terminal/failure control before the registration can
    /// be retired. The overflow read serves the same purpose for data that
    /// could not enter the bounded queue.
    pub(super) fn drain_consistent(&self) -> DeliveryBatch<T, Control> {
        self.drain_consistent_with(|| {})
    }

    fn drain_consistent_with(
        &self,
        after_initial_events: impl FnOnce(),
    ) -> DeliveryBatch<T, Control> {
        // Each pass is capped even if producers continuously refill the
        // channel. Two passes are sufficient: the queue can hold at most one
        // capacity of data preceding any control record.
        let mut events = self
            .receiver
            .try_iter()
            .take(self.event_capacity)
            .collect::<Vec<_>>();
        after_initial_events();
        // Producers hold this same lock across enqueue (and lifecycle commit
        // for controls), so neither plane can move while we take the final
        // finite snapshot. The control channel is unbounded, but its current
        // contents are finite and cannot be replenished under this guard.
        let _cut = self
            .publication_order
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let controls = self.controls.try_iter().collect::<Vec<_>>();
        events.extend(self.receiver.try_iter().take(self.event_capacity));
        DeliveryBatch {
            records: merge_ordered(events, controls),
            overflowed: self.overflowed.swap(false, Ordering::AcqRel),
        }
    }
}

fn merge_ordered<T, Control>(
    events: Vec<Sequenced<T>>,
    controls: Vec<Sequenced<Control>>,
) -> Vec<DeliveryRecord<T, Control>> {
    let mut events = events.into_iter().peekable();
    let mut controls = controls.into_iter().peekable();
    let mut records = Vec::with_capacity(events.len() + controls.len());
    loop {
        match (events.peek(), controls.peek()) {
            (Some(event), Some(control)) if event.sequence < control.sequence => {
                records.push(DeliveryRecord::Event(
                    events.next().expect("peeked event exists").value,
                ));
            }
            (Some(_), Some(_)) => {
                records.push(DeliveryRecord::Control(
                    controls.next().expect("peeked control exists").value,
                ));
            }
            (Some(_), None) => {
                records.extend(
                    events
                        .by_ref()
                        .map(|event| DeliveryRecord::Event(event.value)),
                );
                break;
            }
            (None, Some(_)) => {
                records.extend(
                    controls
                        .by_ref()
                        .map(|control| DeliveryRecord::Control(control.value)),
                );
                break;
            }
            (None, None) => break,
        }
    }
    records
}

#[cfg(test)]
#[path = "tests/delivery.rs"]
mod tests;
