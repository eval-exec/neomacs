//! Production input completion, independent of latency diagnostics.
//!
//! A frame captures a completion frontier by value. Later command completion
//! cannot retroactively acknowledge input in that older frame. Nested commands
//! may finish out of order; the frontier never passes an unfinished command.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

const MAX_PENDING: usize = 1024;
static NEXT_STREAM: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct InputCheckpoint {
    pub stream: u64,
    pub through: u64,
}

#[derive(Debug, Default)]
struct Frontier {
    through: u64,
    pending: VecDeque<bool>,
}

#[derive(Debug)]
struct StreamState {
    id: u64,
    frontier: Mutex<Frontier>,
}

/// One ordered frontend input stream. Clones share its sequence namespace.
#[derive(Clone, Debug)]
pub struct InputStream(Arc<StreamState>);

impl Default for InputStream {
    fn default() -> Self {
        Self(Arc::new(StreamState {
            id: NEXT_STREAM.fetch_add(1, Ordering::Relaxed),
            frontier: Mutex::new(Frontier::default()),
        }))
    }
}

impl InputStream {
    /// Refuse tracking when the bounded ledger is full. The caller must still
    /// deliver the input normally, but cannot predict untracked input.
    pub fn issue(&self) -> Option<InputDelivery> {
        let mut frontier = self.0.frontier.lock().unwrap();
        if frontier.pending.len() == MAX_PENDING {
            return None;
        }
        let serial = frontier
            .through
            .checked_add(frontier.pending.len() as u64 + 1)?;
        frontier.pending.push_back(false);
        Some(InputDelivery(Arc::new(Delivery {
            receipt: InputReceipt {
                stream: Some(self.clone()),
                serial,
                outcome: Arc::new(AtomicU8::new(0)),
                consumed: Arc::new(AtomicBool::new(false)),
            },
        })))
    }

    fn checkpoint(&self) -> InputCheckpoint {
        InputCheckpoint {
            stream: self.0.id,
            through: self.0.frontier.lock().unwrap().through,
        }
    }
}

/// Transport evidence only; this does not authorize any editor operation.
#[derive(Clone)]
pub struct InputReceipt {
    // Read-only receipts do not enter the command-completion ledger.
    stream: Option<InputStream>,
    serial: u64,
    outcome: Arc<AtomicU8>,
    consumed: Arc<AtomicBool>,
}

/// The queued event owns delivery; observational receipts do not keep it alive.
/// Discarding the last undelivered copy cancels its prediction explicitly.
#[derive(Clone, Debug)]
pub struct InputDelivery(Arc<Delivery>);

#[derive(Debug)]
struct Delivery {
    receipt: InputReceipt,
}

impl Drop for Delivery {
    fn drop(&mut self) {
        self.receipt.resolve(2);
    }
}

impl InputDelivery {
    /// Track evaluator reading without occupying a completion frontier. A
    /// command may read arbitrary input before returning; native repeats must
    /// neither exhaust that ledger nor become scroll-preview inputs.
    pub fn for_read() -> Self {
        Self(Arc::new(Delivery {
            receipt: InputReceipt {
                stream: None,
                serial: 0,
                outcome: Arc::new(AtomicU8::new(0)),
                consumed: Arc::new(AtomicBool::new(false)),
            },
        }))
    }

    pub fn receipt(&self) -> InputReceipt {
        self.0.receipt.clone()
    }
    pub fn acknowledged_by(&self, checkpoints: &[InputCheckpoint]) -> bool {
        self.0.receipt.acknowledged_by(checkpoints)
    }
    fn complete(&self) {
        self.0.receipt.resolve(1);
    }
}

impl std::fmt::Debug for InputReceipt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InputReceipt")
            .field("stream", &self.stream.as_ref().map(|stream| stream.0.id))
            .field("serial", &self.serial)
            .finish()
    }
}

impl InputReceipt {
    /// The evaluator read this input, or discarded its delivery. Unlike a
    /// completion checkpoint this can advance inside a still-running command
    /// (e.g. a prefix or `read-char`), so it is suitable for repeat backpressure.
    pub fn consumed_or_cancelled(&self) -> bool {
        self.consumed.load(Ordering::Acquire) || self.cancelled()
    }

    pub fn acknowledged_by(&self, checkpoints: &[InputCheckpoint]) -> bool {
        self.stream.as_ref().is_some_and(|stream| {
            checkpoints.iter().any(|checkpoint| {
                checkpoint.stream == stream.0.id && checkpoint.through >= self.serial
            })
        })
    }

    pub fn same_input(&self, other: &Self) -> bool {
        match (&self.stream, &other.stream) {
            (Some(stream), Some(other_stream)) => {
                self.serial == other.serial && stream.0.id == other_stream.0.id
            }
            (None, None) => Arc::ptr_eq(&self.outcome, &other.outcome),
            _ => false,
        }
    }

    pub fn cancelled(&self) -> bool {
        self.outcome.load(Ordering::Acquire) == 2
    }

    fn resolve(&self, outcome: u8) {
        if self
            .outcome
            .compare_exchange(0, outcome, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let Some(stream) = &self.stream else {
            return;
        };
        let mut frontier = stream.0.frontier.lock().unwrap();
        let Some(offset) = self.serial.checked_sub(frontier.through + 1) else {
            return;
        };
        if let Some(done) = frontier.pending.get_mut(offset as usize) {
            *done = true;
        }
        while frontier.pending.front() == Some(&true) {
            frontier.pending.pop_front();
            frontier.through += 1;
        }
    }
}

/// Evaluator-owned input staging. No Lisp values or live buffer data enter
/// the shared transport ledger.
#[derive(Debug, Default)]
pub struct InputProgress {
    streams: BTreeMap<u64, Weak<StreamState>>,
    staging: Rc<RefCell<InputStaging>>,
}

#[derive(Debug, Default)]
struct InputStaging {
    pending: Vec<InputDelivery>,
    scopes: Vec<(u64, Vec<InputDelivery>)>,
    next_scope: u64,
}

impl InputProgress {
    pub fn consumed(&mut self, receipt: InputDelivery) {
        receipt.0.receipt.consumed.store(true, Ordering::Release);
        let Some(stream) = &receipt.0.receipt.stream else {
            receipt.complete();
            return;
        };
        self.streams.retain(|_, stream| stream.strong_count() > 0);
        self.streams
            .entry(stream.0.id)
            .or_insert_with(|| Arc::downgrade(&stream.0));
        let mut staging = self.staging.borrow_mut();
        if let Some((_, inputs)) = staging.scopes.last_mut() {
            inputs.push(receipt);
        } else {
            staging.pending.push(receipt);
        }
    }

    /// Begin before reading keys, so canceled or unbound sequences complete
    /// too. Nested command readers own their own receipts; synchronous reads
    /// inside an ordinary command belong to that command's scope.
    pub fn begin_command(&mut self) -> CommandInputs {
        let mut staging = self.staging.borrow_mut();
        staging.next_scope += 1;
        let scope = staging.next_scope;
        let inputs = std::mem::take(&mut staging.pending);
        staging.scopes.push((scope, inputs));
        CommandInputs {
            staging: self.staging.clone(),
            scope,
        }
    }

    /// Observe the innermost executing command without retaining delivery
    /// ownership. Timers and callers outside a command have no preview inputs.
    pub fn current_command_receipts(&self) -> Vec<InputReceipt> {
        self.staging
            .borrow()
            .scopes
            .last()
            .map_or_else(Vec::new, |(_, inputs)| {
                if inputs.len() > 128 {
                    return Vec::new();
                }
                inputs.iter().map(InputDelivery::receipt).collect()
            })
    }

    pub fn checkpoint(&self) -> Vec<InputCheckpoint> {
        self.streams
            .values()
            .filter_map(Weak::upgrade)
            .map(|stream| InputStream(stream).checkpoint())
            .collect()
    }
}

#[must_use]
pub struct CommandInputs {
    staging: Rc<RefCell<InputStaging>>,
    scope: u64,
}

impl Drop for CommandInputs {
    fn drop(&mut self) {
        let mut staging = self.staging.borrow_mut();
        if let Some(index) = staging
            .scopes
            .iter()
            .position(|(scope, _)| *scope == self.scope)
        {
            let (_, inputs) = staging.scopes.remove(index);
            for receipt in inputs {
                receipt.complete();
            }
        }
    }
}

#[cfg(test)]
#[path = "input_progress/repeat_tests.rs"]
mod repeat_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_observation_belongs_only_to_innermost_command() {
        let stream = InputStream::default();
        let outer_input = stream.issue().unwrap();
        let receipt = outer_input.receipt();
        let mut progress = InputProgress::default();
        progress.consumed(outer_input);
        assert!(progress.current_command_receipts().is_empty());
        let outer = progress.begin_command();
        assert!(progress.current_command_receipts()[0].same_input(&receipt));
        let inner = progress.begin_command();
        assert!(progress.current_command_receipts().is_empty());
        drop(inner);
        assert_eq!(progress.current_command_receipts().len(), 1);
        drop(outer);
        assert!(progress.current_command_receipts().is_empty());
        assert!(receipt.acknowledged_by(&progress.checkpoint()));
    }

    #[test]
    fn old_frame_cannot_acknowledge_a_later_command_completion() {
        let stream = InputStream::default();
        let receipt = stream.issue().unwrap();
        let mut progress = InputProgress::default();
        progress.consumed(receipt.clone());
        let command = progress.begin_command();
        let old_frame = progress.checkpoint();
        assert!(!receipt.acknowledged_by(&old_frame));
        drop(command);
        assert!(!receipt.acknowledged_by(&old_frame));
        assert!(receipt.acknowledged_by(&progress.checkpoint()));
    }

    #[test]
    fn nested_completion_waits_for_the_outer_command() {
        let stream = InputStream::default();
        let outer = stream.issue().unwrap();
        let inner = stream.issue().unwrap();
        let mut progress = InputProgress::default();
        progress.consumed(outer.clone());
        let outer_command = progress.begin_command();
        let inner_command = progress.begin_command();
        progress.consumed(inner.clone());
        drop(inner_command);
        assert!(!inner.acknowledged_by(&progress.checkpoint()));
        drop(outer_command);
        assert!(inner.acknowledged_by(&progress.checkpoint()));
    }

    #[test]
    fn completion_on_unwind_and_stream_identity_are_independent() {
        let stream = InputStream::default();
        let receipt = stream.issue().unwrap();
        let foreign = InputStream::default().issue().unwrap();
        let mut progress = InputProgress::default();
        progress.consumed(receipt.clone());
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _command = progress.begin_command();
            panic!("command aborted");
        }));
        assert!(receipt.acknowledged_by(&progress.checkpoint()));
        assert!(!foreign.acknowledged_by(&progress.checkpoint()));
    }

    #[test]
    fn discarding_delivery_cancels_even_while_the_compositor_keeps_a_receipt() {
        let stream = InputStream::default();
        let delivery = stream.issue().unwrap();
        let observer = delivery.receipt();
        let queued_copy = delivery.clone();
        drop(delivery);
        assert!(!observer.cancelled());
        drop(queued_copy);
        assert!(observer.cancelled());
        assert!(observer.acknowledged_by(&[stream.checkpoint()]));
    }

    #[test]
    fn completed_delivery_does_not_later_turn_into_cancellation() {
        let stream = InputStream::default();
        let delivery = stream.issue().unwrap();
        let observer = delivery.receipt();
        let mut progress = InputProgress::default();
        let command = progress.begin_command();
        progress.consumed(delivery);
        assert!(!observer.acknowledged_by(&progress.checkpoint()));
        drop(command);
        assert!(observer.acknowledged_by(&progress.checkpoint()));
        assert!(!observer.cancelled());
    }

    #[test]
    fn pending_receipts_are_bounded_and_capacity_returns_after_completion() {
        let stream = InputStream::default();
        let receipts: Vec<_> = (0..MAX_PENDING).map(|_| stream.issue().unwrap()).collect();
        assert!(stream.issue().is_none());
        for receipt in receipts {
            receipt.complete();
        }
        assert!(stream.issue().is_some());
    }
}
