//! Production input completion, independent of latency diagnostics.
//!
//! Completion travels as values (DESIGN §6.14). The frontend gives every
//! tracked input a dense [`InputSerial`] on its session's [`InputStream`]; the
//! evaluator records which serials it consumed and which command scopes still
//! own them; every frame carries that record as a [`FrontierSnapshot`]. A frame
//! captures its frontier by value, so later command completion cannot
//! retroactively acknowledge input in an older frame.
//!
//! Command scopes open before keys are read, and a nested command reader (the
//! minibuffer) completes inner commands while the outer one is still running.
//! The snapshot therefore names each serial's [`Disposition`] exactly instead of
//! stopping at the first unfinished command; the completed prefix that older
//! consumers key on stays derivable from it ([`FrontierSnapshot::completed_prefix`]).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// Tracked inputs a frontend may have in flight before it stops tracking.
const MAX_PENDING: u64 = 1024;

/// One frontend input stream, unique within its display session. The session
/// owner allocates it; nothing in this crate mints stream identities.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct InputStreamId(NonZeroU64);

impl InputStreamId {
    /// The stream of a process's only in-process display session.
    pub const FIRST: Self = Self(NonZeroU64::MIN);

    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Dense per-stream serial assigned by the frontend; the first is 1.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct InputSerial(NonZeroU64);

impl InputSerial {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Names one tracked input. A value: it carries no completion state, so it
/// crosses a process boundary unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InputReceipt {
    stream: InputStreamId,
    serial: InputSerial,
}

impl InputReceipt {
    #[must_use]
    pub const fn new(stream: InputStreamId, serial: InputSerial) -> Self {
        Self { stream, serial }
    }

    #[must_use]
    pub const fn stream(self) -> InputStreamId {
        self.stream
    }

    #[must_use]
    pub const fn serial(self) -> InputSerial {
        self.serial
    }
}

/// The explicit fact that a tracked input will never be consumed. Discarding
/// a delivery states this; dropping one has no effect.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct InputCancelled(pub InputReceipt);

/// Strictly increasing serials. Decoding rejects any other order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "Vec<InputSerial>", into = "Vec<InputSerial>")]
pub struct SerialSet(Vec<InputSerial>);

/// A serial list that is not strictly increasing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnorderedSerials;

impl std::fmt::Display for UnorderedSerials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("serial set is not strictly increasing")
    }
}

impl TryFrom<Vec<InputSerial>> for SerialSet {
    type Error = UnorderedSerials;

    fn try_from(serials: Vec<InputSerial>) -> Result<Self, Self::Error> {
        if serials.windows(2).all(|pair| pair[0] < pair[1]) {
            Ok(Self(serials))
        } else {
            Err(UnorderedSerials)
        }
    }
}

impl From<SerialSet> for Vec<InputSerial> {
    fn from(set: SerialSet) -> Self {
        set.0
    }
}

impl FromIterator<InputSerial> for SerialSet {
    fn from_iter<I: IntoIterator<Item = InputSerial>>(serials: I) -> Self {
        Self(
            serials
                .into_iter()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect(),
        )
    }
}

impl SerialSet {
    #[must_use]
    pub fn contains(&self, serial: InputSerial) -> bool {
        self.0.binary_search(&serial).is_ok()
    }

    #[must_use]
    pub fn first(&self) -> Option<InputSerial> {
        self.0.first().copied()
    }

    #[must_use]
    pub fn last(&self) -> Option<InputSerial> {
        self.0.last().copied()
    }

    pub fn iter(&self) -> impl Iterator<Item = InputSerial> + '_ {
        self.0.iter().copied()
    }
}

/// How a frame stands toward one tracked input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Disposition {
    /// Consumed, and its command completed (or it was cancelled): the frame
    /// shows everything the input will ever do.
    Reflected,
    /// Consumed by a command that is still running: the frame may show part
    /// of its effect.
    Partial,
    /// Not consumed when the frame was laid out, or not knowable from it.
    Pending,
}

impl Disposition {
    /// The receipt's disposition in a frame's frontier (one snapshot per
    /// stream). A stream the frame does not describe is `Pending`.
    #[must_use]
    pub fn of(receipt: InputReceipt, frontier: &[FrontierSnapshot]) -> Self {
        frontier
            .iter()
            .find(|snapshot| snapshot.stream() == receipt.stream())
            .map_or(Self::Pending, |snapshot| snapshot.disposition(receipt))
    }
}

/// The highest serial below which every input of `stream` is reflected.
/// Presentations are triggered when it moves (the pre-value frontier).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CompletedPrefix {
    pub stream: InputStreamId,
    pub through: u64,
}

/// One stream's frontier as a frame captured it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "RawFrontierSnapshot", into = "RawFrontierSnapshot")]
pub enum FrontierSnapshot {
    /// `consumed_through` is the highest serial the evaluator consumed (the
    /// stream is consumed in order); `in_progress` lists the consumed serials
    /// whose command had not completed, so none exceeds `consumed_through`.
    Exact {
        stream: InputStreamId,
        consumed_through: u64,
        in_progress: SerialSet,
    },
    /// Input of this stream was consumed while the frame was laid out, so the
    /// frame cannot say what it shows of that input. Every serial through
    /// `reflected_through` had been consumed and its command completed before
    /// layout began, so the frame still shows all of it.
    Indeterminate {
        stream: InputStreamId,
        reflected_through: u64,
    },
}

/// The serde shape of [`FrontierSnapshot`]; decoding checks its invariant.
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename = "FrontierSnapshot")]
enum RawFrontierSnapshot {
    Exact {
        stream: InputStreamId,
        consumed_through: u64,
        in_progress: SerialSet,
    },
    Indeterminate {
        stream: InputStreamId,
        reflected_through: u64,
    },
}

/// An exact frontier naming an in-progress serial it did not consume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnconsumedInProgress;

impl std::fmt::Display for UnconsumedInProgress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("an in-progress serial exceeds the frontier's consumed_through")
    }
}

impl TryFrom<RawFrontierSnapshot> for FrontierSnapshot {
    type Error = UnconsumedInProgress;

    fn try_from(raw: RawFrontierSnapshot) -> Result<Self, Self::Error> {
        match raw {
            RawFrontierSnapshot::Exact {
                stream,
                consumed_through,
                in_progress,
            } => {
                if in_progress
                    .last()
                    .is_some_and(|serial| serial.get() > consumed_through)
                {
                    return Err(UnconsumedInProgress);
                }
                Ok(Self::Exact {
                    stream,
                    consumed_through,
                    in_progress,
                })
            }
            RawFrontierSnapshot::Indeterminate {
                stream,
                reflected_through,
            } => Ok(Self::Indeterminate {
                stream,
                reflected_through,
            }),
        }
    }
}

impl From<FrontierSnapshot> for RawFrontierSnapshot {
    fn from(snapshot: FrontierSnapshot) -> Self {
        match snapshot {
            FrontierSnapshot::Exact {
                stream,
                consumed_through,
                in_progress,
            } => Self::Exact {
                stream,
                consumed_through,
                in_progress,
            },
            FrontierSnapshot::Indeterminate {
                stream,
                reflected_through,
            } => Self::Indeterminate {
                stream,
                reflected_through,
            },
        }
    }
}

impl FrontierSnapshot {
    #[must_use]
    pub const fn stream(&self) -> InputStreamId {
        match self {
            Self::Exact { stream, .. } | Self::Indeterminate { stream, .. } => *stream,
        }
    }

    #[must_use]
    pub fn disposition(&self, receipt: InputReceipt) -> Disposition {
        if receipt.stream() != self.stream() {
            return Disposition::Pending;
        }
        let serial = receipt.serial();
        match self {
            Self::Exact {
                consumed_through,
                in_progress,
                ..
            } if serial.get() <= *consumed_through => {
                if in_progress.contains(serial) {
                    Disposition::Partial
                } else {
                    Disposition::Reflected
                }
            }
            Self::Indeterminate {
                reflected_through, ..
            } if serial.get() <= *reflected_through => Disposition::Reflected,
            Self::Exact { .. } | Self::Indeterminate { .. } => Disposition::Pending,
        }
    }

    /// The completed prefix this snapshot implies: every serial through it is
    /// reflected.
    #[must_use]
    pub fn completed_prefix(&self) -> CompletedPrefix {
        let through = match self {
            Self::Exact {
                consumed_through,
                in_progress,
                ..
            } => in_progress
                .first()
                .map_or(*consumed_through, |serial| serial.get() - 1),
            Self::Indeterminate {
                reflected_through, ..
            } => *reflected_through,
        };
        CompletedPrefix {
            stream: self.stream(),
            through,
        }
    }

    /// A serial the evaluator is known to have passed: every input at or
    /// below it was consumed, or will never be (the stream is consumed in
    /// order). A frontend no longer has any of it in flight.
    #[must_use]
    pub fn passed_through(&self) -> u64 {
        match self {
            Self::Exact {
                consumed_through, ..
            } => *consumed_through,
            Self::Indeterminate {
                reflected_through, ..
            } => *reflected_through,
        }
    }

    /// Settle the frontier captured before a frame's layout against the one
    /// after it. Layout Lisp can enter a nested command reader: a stream whose
    /// consumption moved meanwhile is indeterminate for this frame, except for
    /// what had completed before layout began. Commands that only completed
    /// meanwhile keep the earlier snapshot, which never acknowledges more than
    /// the frame was laid out from.
    #[must_use]
    pub fn settle(before: Vec<Self>, after: &[Self]) -> Vec<Self> {
        let first_consumed_meanwhile = after
            .iter()
            .filter(|later| {
                !before
                    .iter()
                    .any(|earlier| earlier.stream() == later.stream())
            })
            .map(|later| Self::Indeterminate {
                stream: later.stream(),
                reflected_through: 0,
            })
            .collect::<Vec<_>>();
        before
            .into_iter()
            .map(|snapshot| {
                let stream = snapshot.stream();
                let moved = after.iter().any(|later| {
                    later.stream() == stream
                        && consumed_through(later) != consumed_through(&snapshot)
                });
                if moved {
                    Self::Indeterminate {
                        stream,
                        reflected_through: snapshot.completed_prefix().through,
                    }
                } else {
                    snapshot
                }
            })
            .chain(first_consumed_meanwhile)
            .collect()
    }
}

fn consumed_through(snapshot: &FrontierSnapshot) -> Option<u64> {
    match snapshot {
        FrontierSnapshot::Exact {
            consumed_through, ..
        } => Some(*consumed_through),
        FrontierSnapshot::Indeterminate { .. } => None,
    }
}

/// The frontend's ledger of one stream: which serials it issued, which it
/// cancelled, and how far the evaluator's frontier has been observed to reach.
#[derive(Clone, Debug)]
pub struct InputStream(Arc<StreamLedger>);

#[derive(Debug)]
struct StreamLedger {
    id: InputStreamId,
    state: Mutex<LedgerState>,
}

/// Serials up to `observed_through` were passed by the evaluator (a frame
/// said so). Every serial in `observed_through + 1..=settled_through` was
/// cancelled: a contiguous run the evaluator will never consume. Neither
/// occupies capacity. `cancelled` holds the cancelled serials above that run.
#[derive(Debug, Default)]
struct LedgerState {
    issued: u64,
    observed_through: u64,
    settled_through: u64,
    cancelled: BTreeSet<u64>,
}

impl LedgerState {
    /// Extend the settled run over cancellations that now adjoin it.
    fn settle_cancelled_run(&mut self) {
        while self.cancelled.remove(&(self.settled_through + 1)) {
            self.settled_through += 1;
        }
    }
}

impl InputStream {
    #[must_use]
    pub fn new(id: InputStreamId) -> Self {
        Self(Arc::new(StreamLedger {
            id,
            state: Mutex::new(LedgerState::default()),
        }))
    }

    #[must_use]
    pub fn id(&self) -> InputStreamId {
        self.0.id
    }

    /// Refuse tracking when the bounded ledger is full. The caller must still
    /// deliver the input normally, but cannot predict untracked input.
    pub fn issue(&self) -> Option<InputDelivery> {
        let mut state = self.0.state.lock().unwrap();
        if state.issued - state.settled_through >= MAX_PENDING {
            return None;
        }
        let serial = NonZeroU64::new(state.issued.checked_add(1)?)?;
        state.issued = serial.get();
        Some(InputDelivery {
            receipt: InputReceipt::new(self.id(), InputSerial(serial)),
            ledger: self.clone(),
        })
    }

    /// Learn from a frame's frontier how far the evaluator got: the input it
    /// passed returns its capacity, and cancellations it passed are
    /// forgotten, because every later frame reports them reflected.
    pub fn observe(&self, frontier: &[FrontierSnapshot]) {
        let Some(snapshot) = frontier
            .iter()
            .find(|snapshot| snapshot.stream() == self.id())
        else {
            return;
        };
        let mut state = self.0.state.lock().unwrap();
        let passed = snapshot.passed_through().min(state.issued);
        if passed <= state.observed_through {
            return;
        }
        state.observed_through = passed;
        state.settled_through = state.settled_through.max(passed);
        let above_run = state.settled_through + 1;
        state.cancelled = state.cancelled.split_off(&above_run);
        state.settle_cancelled_run();
    }

    /// Whether this stream's ledger recorded the input as cancelled.
    #[must_use]
    pub fn cancelled(&self, receipt: InputReceipt) -> bool {
        if receipt.stream() != self.id() {
            return false;
        }
        let serial = receipt.serial().get();
        let state = self.0.state.lock().unwrap();
        serial > state.observed_through
            && (serial <= state.settled_through || state.cancelled.contains(&serial))
    }

    fn cancel(&self, receipt: InputReceipt) -> InputCancelled {
        let mut state = self.0.state.lock().unwrap();
        if receipt.serial().get() > state.settled_through {
            state.cancelled.insert(receipt.serial().get());
            state.settle_cancelled_run();
        }
        InputCancelled(receipt)
    }
}

/// The frontend's handle on one tracked input in flight: the value receipt
/// plus the route back to its stream's ledger. It never leaves the frontend:
/// the evaluator receives the [`InputReceipt`] alone, in process as over a
/// display link, and an input discarded before the evaluator's queue is
/// stated cancelled through [`Self::cancel`].
#[derive(Clone, Debug)]
pub struct InputDelivery {
    receipt: InputReceipt,
    ledger: InputStream,
}

impl InputDelivery {
    #[must_use]
    pub fn receipt(&self) -> InputReceipt {
        self.receipt
    }

    /// The ledger that learns this input's cancellation.
    #[must_use]
    pub fn ledger(&self) -> &InputStream {
        &self.ledger
    }

    /// State that this input will never be consumed (it was discarded before
    /// reaching the evaluator's queue). Dropping a delivery cancels nothing.
    pub fn cancel(&self) -> InputCancelled {
        self.ledger.cancel(self.receipt)
    }
}

/// Evaluator-owned input staging. No Lisp values or live buffer data enter
/// the frontier.
#[derive(Debug, Default)]
pub struct InputProgress {
    consumed_through: BTreeMap<InputStreamId, u64>,
    staging: Rc<RefCell<InputStaging>>,
}

#[derive(Debug, Default)]
struct InputStaging {
    pending: Vec<InputReceipt>,
    scopes: Vec<(u64, Vec<InputReceipt>)>,
    next_scope: u64,
}

impl InputProgress {
    /// Record that the evaluator took this input from its queue. A stream is
    /// consumed in order, so every lower serial is consumed or was discarded.
    pub fn consumed(&mut self, receipt: InputReceipt) {
        let through = self.consumed_through.entry(receipt.stream()).or_default();
        *through = (*through).max(receipt.serial().get());
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

    /// Observe the innermost executing command. Timers and callers outside a
    /// command have no preview inputs.
    pub fn current_command_receipts(&self) -> Vec<InputReceipt> {
        self.staging
            .borrow()
            .scopes
            .last()
            .map_or_else(Vec::new, |(_, inputs)| {
                if inputs.len() > 128 {
                    return Vec::new();
                }
                inputs.clone()
            })
    }

    /// The exact frontier of every stream this evaluator consumed from.
    pub fn checkpoint(&self) -> Vec<FrontierSnapshot> {
        let staging = self.staging.borrow();
        let open = staging
            .pending
            .iter()
            .chain(staging.scopes.iter().flat_map(|(_, inputs)| inputs))
            .copied()
            .collect::<Vec<_>>();
        self.consumed_through
            .iter()
            .map(|(stream, through)| FrontierSnapshot::Exact {
                stream: *stream,
                consumed_through: *through,
                in_progress: open
                    .iter()
                    .filter(|receipt| receipt.stream() == *stream)
                    .map(|receipt| receipt.serial())
                    .collect(),
            })
            .collect()
    }

    /// What redisplay keys on: the completed prefix of every stream. It moves
    /// when the pre-value frontier moved for consumed input. A frontend
    /// cancellation no longer moves it: the old shared ledger stepped past a
    /// cancelled serial at once, which changed the redisplay signature, but
    /// the evaluator never learns of a cancellation now. In process a tracked
    /// input is cancelled only when the evaluator's queue is gone or the input
    /// bridge converts it to nothing, which no tracked input kind does, and
    /// the compositor drops a cancelled prediction by itself.
    pub fn completed_prefixes(&self) -> Vec<CompletedPrefix> {
        self.checkpoint()
            .iter()
            .map(FrontierSnapshot::completed_prefix)
            .collect()
    }
}

/// One command's scope: its inputs stay in progress until it ends, by return
/// or by unwind.
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
            staging.scopes.remove(index);
        }
    }
}

#[cfg(test)]
#[path = "tests/input_progress_test.rs"]
mod tests;
