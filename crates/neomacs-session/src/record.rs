//! Records: the 8-byte header, incremental parsing with limits checked before
//! any allocation, and per-lane chunk reassembly (DESIGN §6.3, §6.18).
//!
//! ```text
//! u32 LE  len       body bytes; ≤ 64 KiB before Welcome, then ≤ max_record
//! u8      lane      0 control · 1 input · 2 present · 3 facts · 4 hint · 5 bulk
//! u8      flags     bit0 COMPRESSED · bit1 MORE · bits 2–7 must be 0
//! u8      service   0 session · otherwise assigned in Welcome
//! u8      reserved  0
//! ```

use crate::lane::Lane;

pub const HEADER_LEN: usize = 8;
/// Every record before `Welcome` (both directions) is at most this long.
pub const PRE_WELCOME_MAX_RECORD: u32 = 64 * 1024;
pub const DEFAULT_MAX_RECORD: u32 = 1024 * 1024;
pub const HARD_MAX_RECORD: u32 = 16 * 1024 * 1024;
pub const DEFAULT_MAX_MESSAGE: u32 = 8 * 1024 * 1024;
pub const HARD_MAX_MESSAGE: u32 = 64 * 1024 * 1024;

const FLAG_COMPRESSED: u8 = 0b01;
const FLAG_MORE: u8 = 0b10;

/// Whether a message body is raw or `varint len ‖ raw deflate` (the service
/// codec compresses; the session only carries the bit).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Compression {
    Raw,
    Deflate,
}

/// Whether another chunk of the same message follows on the same lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Continuation {
    Last,
    More,
}

/// The service a record belongs to: 0 is the session layer itself; the
/// evaluator assigns the others in `Welcome`.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub struct ServiceSlot(u8);

impl ServiceSlot {
    pub const SESSION: Self = Self(0);

    #[must_use]
    pub const fn new(raw: u8) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// A violation of the record or session grammar. Decoding never panics; every
/// malformed input is one of these, and the session closes with
/// `Goodbye{ProtocolViolation}`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("record of {len} bytes exceeds the {limit}-byte limit")]
    RecordTooLarge { len: u32, limit: u32 },
    #[error("message of at least {len} bytes exceeds the {limit}-byte limit")]
    MessageTooLarge { len: u64, limit: u32 },
    #[error("unknown lane {0}")]
    UnknownLane(u8),
    #[error("reserved record flags {0:#04x}")]
    ReservedFlags(u8),
    #[error("reserved record byte {0:#04x}")]
    ReservedByte(u8),
    #[error("record for unassigned service slot {0}")]
    UnknownService(u8),
    #[error("chunks of one message on lane {lane:?} disagree on service or compression")]
    InconsistentChunks { lane: Lane },
    #[error("session record is chunked or compressed")]
    MalformedSessionRecord,
    #[error("undecodable session record: {0}")]
    SessionRecord(String),
    #[error("unexpected session record {record} in state {state}")]
    UnexpectedRecord {
        record: &'static str,
        state: &'static str,
    },
    #[error("acknowledgement on lane {lane:?} through {through} exceeds the {sent} messages sent")]
    AckBeyondSent { lane: Lane, through: u64, sent: u64 },
    /// A resume cursor or a warm Welcome counts more messages received on
    /// `lane` than this side sent on the session.
    #[error("the peer counts {received} messages received on lane {lane:?}; {sent} were sent")]
    ReceivedBeyondSent {
        lane: Lane,
        received: u64,
        sent: u64,
    },
    /// The lane's message indices are used up: the next message has none.
    #[error("lane {lane:?} has no message index left")]
    LaneExhausted { lane: Lane },
    /// A Welcome's limits exceed what this side offered; a Welcome carries
    /// the minimum of both offers.
    #[error("the Welcome's limits {welcome:?} exceed the offered {offered:?}")]
    LimitsBeyondOffer {
        offered: crate::handshake::Limits,
        welcome: crate::handshake::Limits,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordHeader {
    pub len: u32,
    pub lane: Lane,
    pub compression: Compression,
    pub continuation: Continuation,
    pub service: ServiceSlot,
}

impl RecordHeader {
    #[must_use]
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut bytes = [0u8; HEADER_LEN];
        bytes[0..4].copy_from_slice(&self.len.to_le_bytes());
        bytes[4] = self.lane as u8;
        let mut flags = 0;
        if self.compression == Compression::Deflate {
            flags |= FLAG_COMPRESSED;
        }
        if self.continuation == Continuation::More {
            flags |= FLAG_MORE;
        }
        bytes[5] = flags;
        bytes[6] = self.service.get();
        bytes
    }

    pub fn parse(bytes: &[u8; HEADER_LEN]) -> Result<Self, ProtocolError> {
        let len = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let lane = Lane::from_byte(bytes[4]).ok_or(ProtocolError::UnknownLane(bytes[4]))?;
        let flags = bytes[5];
        if flags & !(FLAG_COMPRESSED | FLAG_MORE) != 0 {
            return Err(ProtocolError::ReservedFlags(
                flags & !(FLAG_COMPRESSED | FLAG_MORE),
            ));
        }
        if bytes[7] != 0 {
            return Err(ProtocolError::ReservedByte(bytes[7]));
        }
        Ok(Self {
            len,
            lane,
            compression: if flags & FLAG_COMPRESSED != 0 {
                Compression::Deflate
            } else {
                Compression::Raw
            },
            continuation: if flags & FLAG_MORE != 0 {
                Continuation::More
            } else {
                Continuation::Last
            },
            service: ServiceSlot(bytes[6]),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Record {
    pub header: RecordHeader,
    pub body: Vec<u8>,
}

/// Incremental record parser. The header is validated, including the length
/// limit, before the body buffer is reserved.
#[derive(Debug)]
pub struct RecordReader {
    limit: u32,
    pending: Vec<u8>,
    /// Bytes of `pending` already parsed.
    start: usize,
    header: Option<RecordHeader>,
    body: Vec<u8>,
}

impl RecordReader {
    #[must_use]
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            pending: Vec::new(),
            start: 0,
            header: None,
            body: Vec::new(),
        }
    }

    pub fn set_limit(&mut self, limit: u32) {
        self.limit = limit;
    }

    pub fn push(&mut self, bytes: &[u8]) {
        self.pending.extend_from_slice(bytes);
    }

    /// Bytes held between records or inside a partial one.
    #[must_use]
    pub fn buffered_len(&self) -> usize {
        self.pending.len() - self.start
            + self.body.len()
            + if self.header.is_some() { HEADER_LEN } else { 0 }
    }

    /// Capacity reserved by the reader, for allocation tests.
    #[must_use]
    pub fn buffered_capacity(&self) -> usize {
        self.pending.capacity() + self.body.capacity()
    }

    /// Whether the stream sits exactly at a record boundary.
    #[must_use]
    pub fn at_boundary(&self) -> bool {
        self.buffered_len() == 0
    }

    pub fn next_record(&mut self) -> Result<Option<Record>, ProtocolError> {
        let result = self.read_record();
        // Keep the unread tail at the front; buffers stay bounded by one
        // record plus one read.
        if self.start > 0 && self.start * 2 >= self.pending.len() {
            self.pending.drain(..self.start);
            self.start = 0;
        }
        if self.pending.is_empty() {
            self.pending = Vec::new();
        }
        result
    }

    fn read_record(&mut self) -> Result<Option<Record>, ProtocolError> {
        let header = match self.header {
            Some(header) => header,
            None => {
                let available = &self.pending[self.start..];
                if available.len() < HEADER_LEN {
                    return Ok(None);
                }
                let mut raw = [0u8; HEADER_LEN];
                raw.copy_from_slice(&available[..HEADER_LEN]);
                let header = RecordHeader::parse(&raw)?;
                if header.len > self.limit {
                    return Err(ProtocolError::RecordTooLarge {
                        len: header.len,
                        limit: self.limit,
                    });
                }
                self.start += HEADER_LEN;
                self.body = Vec::with_capacity(header.len as usize);
                self.header = Some(header);
                header
            }
        };
        let wanted = header.len as usize - self.body.len();
        let available = &self.pending[self.start..];
        let take = wanted.min(available.len());
        self.body.extend_from_slice(&available[..take]);
        self.start += take;
        if self.body.len() < header.len as usize {
            return Ok(None);
        }
        self.header = None;
        Ok(Some(Record {
            header,
            body: std::mem::take(&mut self.body),
        }))
    }
}

/// A complete message reassembled from one or more chunks of one lane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssembledMessage {
    pub lane: Lane,
    pub service: ServiceSlot,
    pub compression: Compression,
    pub body: Vec<u8>,
}

#[derive(Debug)]
struct Partial {
    service: ServiceSlot,
    compression: Compression,
    body: Vec<u8>,
}

/// Per-lane reassembly of chunked messages, bounded by `max_message` before
/// each chunk is appended.
#[derive(Debug)]
pub struct Reassembler {
    max_message: u32,
    partial: [Option<Partial>; 6],
}

impl Reassembler {
    #[must_use]
    pub fn new(max_message: u32) -> Self {
        Self {
            max_message,
            partial: Default::default(),
        }
    }

    pub fn set_max_message(&mut self, max_message: u32) {
        self.max_message = max_message;
    }

    /// Body bytes held in incomplete messages.
    #[must_use]
    pub fn pending_bytes(&self) -> usize {
        self.partial
            .iter()
            .flatten()
            .map(|partial| partial.body.len())
            .sum()
    }

    pub fn accept(&mut self, record: Record) -> Result<Option<AssembledMessage>, ProtocolError> {
        let Record { header, body } = record;
        let slot = &mut self.partial[header.lane.index()];
        let mut partial = match slot.take() {
            Some(partial) => {
                if partial.service != header.service || partial.compression != header.compression {
                    return Err(ProtocolError::InconsistentChunks { lane: header.lane });
                }
                partial
            }
            None => Partial {
                service: header.service,
                compression: header.compression,
                body: Vec::new(),
            },
        };
        let total = partial.body.len() as u64 + body.len() as u64;
        if total > u64::from(self.max_message) {
            return Err(ProtocolError::MessageTooLarge {
                len: total,
                limit: self.max_message,
            });
        }
        if partial.body.is_empty() {
            partial.body = body;
        } else {
            partial.body.extend_from_slice(&body);
        }
        match header.continuation {
            Continuation::More => {
                *slot = Some(partial);
                Ok(None)
            }
            Continuation::Last => Ok(Some(AssembledMessage {
                lane: header.lane,
                service: partial.service,
                compression: partial.compression,
                body: partial.body,
            })),
        }
    }
}
