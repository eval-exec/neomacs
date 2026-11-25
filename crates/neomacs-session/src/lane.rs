//! Lanes, implicit sequence numbers, the priority scheduler and flow control
//! (DESIGN §6.3-§6.4, §6.12).
//!
//! The stream is totally ordered; lanes only steer the sender's scheduler and
//! per-lane chunk reassembly. Sequence numbers are implicit: each side counts
//! complete messages per lane, and acknowledgements, resume cursors and replay
//! refer to those counts.

use std::collections::VecDeque;
use std::num::NonZeroU64;
use std::time::Duration;

use crate::SessionTime;
use crate::record::{Compression, Continuation, RecordHeader, ServiceSlot};

/// The six lanes of a session (record header byte 4).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(u8)]
pub enum Lane {
    Control = 0,
    Input = 1,
    Present = 2,
    Facts = 3,
    Hint = 4,
    Bulk = 5,
}

impl Lane {
    pub const ALL: [Lane; 6] = [
        Lane::Control,
        Lane::Input,
        Lane::Present,
        Lane::Facts,
        Lane::Hint,
        Lane::Bulk,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Control),
            1 => Some(Self::Input),
            2 => Some(Self::Present),
            3 => Some(Self::Facts),
            4 => Some(Self::Hint),
            5 => Some(Self::Bulk),
            _ => None,
        }
    }

    /// Sender priority: lower is sent first (DESIGN §6.4).
    #[must_use]
    pub const fn priority(self) -> u8 {
        match self {
            Self::Control | Self::Input => 0,
            Self::Present | Self::Facts => 1,
            Self::Hint => 2,
            Self::Bulk => 3,
        }
    }

    /// How the sender's flow control treats the lane.
    #[must_use]
    pub const fn flow(self) -> LaneFlow {
        match self {
            Self::Control | Self::Input => LaneFlow::Exempt,
            Self::Present | Self::Facts | Self::Hint => LaneFlow::ByteWindow,
            Self::Bulk => LaneFlow::Credit,
        }
    }
}

/// Flow-control class of a lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneFlow {
    /// Always sent: control and input must flow even when the window is full,
    /// so pings, acks and a late C-g are never stuck.
    Exempt,
    /// A new message starts only while unacknowledged bytes are under the
    /// stream window; a started message always finishes, so a message larger
    /// than the window cannot deadlock.
    ByteWindow,
    /// Sent only when every other lane is empty and the receiver's credit and
    /// half the window allow it.
    Credit,
}

/// The 1-based index of a complete message on one lane of one session.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct MessageIndex(NonZeroU64);

impl MessageIndex {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    /// The index of the message that follows `count` messages; `None` when
    /// the lane's indices are used up.
    #[must_use]
    pub const fn after(count: u64) -> Option<Self> {
        match NonZeroU64::MIN.checked_add(count) {
            Some(index) => Some(Self(index)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// Complete messages per lane; the implicit sequence numbers.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct LaneCounts([u64; 6]);

impl LaneCounts {
    #[must_use]
    pub const fn get(&self, lane: Lane) -> u64 {
        self.0[lane.index()]
    }

    pub fn set(&mut self, lane: Lane, count: u64) {
        self.0[lane.index()] = count;
    }

    /// Count one more message on `lane` and return its index.
    pub fn advance(&mut self, lane: Lane) -> MessageIndex {
        let next = self.0[lane.index()].saturating_add(1);
        self.0[lane.index()] = next;
        MessageIndex(NonZeroU64::new(next).expect("a successor is never zero"))
    }
}

/// `clamp(rate × 5 ms, 1 KiB, 64 KiB)` (P3 §4.7).
#[must_use]
pub fn chunk_size_for_rate(bytes_per_second: u64) -> usize {
    let per_five_ms = bytes_per_second / 200;
    per_five_ms.clamp(1024, 64 * 1024) as usize
}

/// Chunk size used before any rate has been measured.
pub const DEFAULT_CHUNK: usize = 16 * 1024;

/// `clamp(2 × BDP_est, 64 KiB, 8 MiB)` (P4 §4.8).
#[must_use]
pub fn byte_window_for(bytes_per_second: Option<u64>, rtt: Option<Duration>) -> u64 {
    const MIN: u64 = 64 * 1024;
    const MAX: u64 = 8 * 1024 * 1024;
    match (bytes_per_second, rtt) {
        (Some(rate), Some(rtt)) => {
            let bdp = (u128::from(rate) * rtt.as_micros() / 1_000_000) as u64;
            bdp.saturating_mul(2).clamp(MIN, MAX)
        }
        _ => MIN,
    }
}

/// Throughput measured while the sender was backlogged: the bytes of one
/// write divided by the time until the transport accepted the next write.
#[derive(Clone, Debug, Default)]
pub(crate) struct RateEstimator {
    previous: Option<(SessionTime, usize)>,
    bytes_per_second: Option<f64>,
}

impl RateEstimator {
    pub(crate) fn on_write(&mut self, now: SessionTime, bytes: usize, backlogged_after: bool) {
        if let Some((at, previous_bytes)) = self.previous {
            let elapsed = now.saturating_since(at);
            if !elapsed.is_zero() {
                let sample = previous_bytes as f64 / elapsed.as_secs_f64();
                self.bytes_per_second = Some(match self.bytes_per_second {
                    Some(rate) => rate * 0.5 + sample * 0.5,
                    None => sample,
                });
            }
        }
        self.previous = backlogged_after.then_some((now, bytes));
    }

    pub(crate) fn bytes_per_second(&self) -> Option<u64> {
        self.bytes_per_second.map(|rate| rate as u64)
    }

    pub(crate) fn chunk_size(&self) -> usize {
        self.bytes_per_second()
            .map_or(DEFAULT_CHUNK, chunk_size_for_rate)
    }
}

/// One message queued for sending.
#[derive(Debug)]
pub(crate) struct Outgoing {
    pub(crate) service: ServiceSlot,
    pub(crate) compression: Compression,
    pub(crate) body: Vec<u8>,
    pub(crate) offset: usize,
    pub(crate) order: u64,
}

impl Outgoing {
    fn started(&self) -> bool {
        self.offset > 0
    }

    fn remaining(&self) -> usize {
        self.body.len() - self.offset
    }
}

/// What the scheduler may send now, decided by the link's flow control.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FlowBudget {
    /// Bytes of window-controlled lanes written but not acknowledged.
    pub(crate) window_in_flight: u64,
    pub(crate) window: u64,
    /// Bulk bytes written but not yet credited by the receiver.
    pub(crate) bulk_uncredited: u64,
    /// Bulk bytes the receiver has granted beyond what was written.
    pub(crate) bulk_credit_available: u64,
}

/// One record the scheduler chose.
#[derive(Debug)]
pub(crate) struct Chunk {
    pub(crate) header: RecordHeader,
    pub(crate) body: Vec<u8>,
    /// The message this chunk completes, with its total body length.
    pub(crate) completes: Option<usize>,
}

/// Per-lane FIFO queues drained in priority order, one chunk at a time, so a
/// control or input record enqueued behind megabytes of bulk waits at most for
/// the chunk being written.
#[derive(Debug, Default)]
pub(crate) struct Scheduler {
    queues: [VecDeque<Outgoing>; 6],
    next_order: u64,
}

impl Scheduler {
    pub(crate) fn push(
        &mut self,
        lane: Lane,
        service: ServiceSlot,
        compression: Compression,
        body: Vec<u8>,
    ) {
        let order = self.next_order;
        self.next_order += 1;
        self.queues[lane.index()].push_back(Outgoing {
            service,
            compression,
            body,
            offset: 0,
            order,
        });
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.queues.iter().all(VecDeque::is_empty)
    }

    fn admissible(&self, lane: Lane, budget: FlowBudget, chunk: usize) -> bool {
        let Some(front) = self.queues[lane.index()].front() else {
            return false;
        };
        match lane.flow() {
            LaneFlow::Exempt => true,
            LaneFlow::ByteWindow => front.started() || budget.window_in_flight < budget.window,
            LaneFlow::Credit => {
                let others_empty = Lane::ALL
                    .iter()
                    .filter(|other| **other != Lane::Bulk)
                    .all(|other| self.queues[other.index()].is_empty());
                let next = chunk.min(front.remaining()) as u64;
                others_empty
                    && budget.bulk_uncredited.saturating_add(next) <= budget.window / 2
                    && next <= budget.bulk_credit_available
            }
        }
    }

    /// The next record to write, or `None` when nothing is admissible.
    pub(crate) fn next_chunk(&mut self, chunk_size: usize, budget: FlowBudget) -> Option<Chunk> {
        let lane = Lane::ALL
            .iter()
            .copied()
            .filter(|lane| self.admissible(*lane, budget, chunk_size))
            .min_by_key(|lane| {
                let front = self.queues[lane.index()]
                    .front()
                    .expect("admissible lanes are non-empty");
                (lane.priority(), front.order)
            })?;
        let queue = &mut self.queues[lane.index()];
        let front = queue.front_mut().expect("admissible lanes are non-empty");
        let take = chunk_size.max(1).min(front.remaining());
        let start = front.offset;
        front.offset += take;
        let last = front.offset == front.body.len();
        let body = front.body[start..front.offset].to_vec();
        let header = RecordHeader {
            len: body.len() as u32,
            lane,
            compression: front.compression,
            continuation: if last {
                Continuation::Last
            } else {
                Continuation::More
            },
            service: front.service,
        };
        let completes = if last {
            let total = front.body.len();
            queue.pop_front();
            Some(total)
        } else {
            None
        };
        Some(Chunk {
            header,
            body,
            completes,
        })
    }
}
