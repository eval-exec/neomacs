//! The sans-I/O session state machines (DESIGN §6.5, §6.19; D34).
//!
//! A *link* is one transport connection: preface, records, scheduling and
//! keepalive. A *session* outlives its links: implicit per-lane counters, the
//! resume token, the frontend incarnation and the input replay buffer.
//!
//! [`FrontendEndpoint`] dials and resumes one session. [`EvaluatorEndpoint`]
//! is the listener side: it owns every session and every link, so takeover,
//! two-phase token rotation and resume are decisions inside one state machine
//! rather than coordination between objects.
//!
//! Both are driven the same way: `receive(bytes, now)`, `receive_eof(now)`,
//! `poll_transmit(now)`, `tick(now)`, `poll_event()`.

use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroU64;
use std::time::Duration;

use crate::SessionTime;
use crate::handshake::{
    BuildId, BuildPolicy, CloseReason, DisplayLabel, Durability, FrontendIncarnation, LabelState,
    Limits, MinorRange, OfferMode, Resumability, ResumeCursors, ResumeOutcome, ResumeToken,
    ServiceAccept, ServiceName, ServiceOffer, SessionHello, SessionId, SessionIntent,
    SessionMessage, SessionRefusal, SessionWelcome, negotiate,
};
use crate::keepalive::{DEFAULT_KEEPALIVE_TIMEOUT, HANDSHAKE_DEADLINE, Keepalive, KeepaliveAction};
use crate::lane::{
    FlowBudget, Lane, LaneCounts, LaneFlow, MessageIndex, RateEstimator, Scheduler, byte_window_for,
};
use crate::preface::{
    PREFACE_LEN, PeerBytes, Preface, PrefaceError, REFUSAL_LINE, Role, SESSION_MAJOR,
};
use crate::record::{
    AssembledMessage, Compression, HEADER_LEN, PRE_WELCOME_MAX_RECORD, ProtocolError, Reassembler,
    RecordReader, ServiceSlot,
};

/// How the evaluator obtained a transport. Decided by the evaluator, never
/// from the peer's claims (DESIGN §6.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkLocality {
    InProcess,
    SpawnedChild,
    Dialed,
}

/// One transport connection on an [`EvaluatorEndpoint`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LinkId(u64);

/// Why a link ended. Every way a link can end is one of these.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseOutcome {
    /// The peer said goodbye.
    Goodbye(CloseReason),
    /// This side closed the link.
    Local(CloseReason),
    /// End of stream at a record boundary.
    PeerClosed,
    /// End of stream inside a record or a chunked message.
    Truncated {
        buffered: usize,
    },
    KeepaliveTimeout,
    HandshakeTimeout,
    Protocol(ProtocolError),
    BadPreface(PrefaceError),
    /// The peer refused this side.
    RefusedByPeer(SessionRefusal),
    /// This side refused the peer.
    RefusedPeer(SessionRefusal),
    TransportFailed(std::io::ErrorKind),
}

/// A complete service message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivered {
    pub lane: Lane,
    pub service: ServiceSlot,
    pub index: MessageIndex,
    pub compression: Compression,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SendError {
    #[error("no established session")]
    NotConnected,
    #[error("the session is detached")]
    Detached,
    #[error("no such session")]
    UnknownSession,
    #[error("service slot {0:?} was not accepted")]
    UnknownService(ServiceSlot),
    #[error("message of {len} bytes exceeds the agreed {limit}-byte limit")]
    TooLarge { len: usize, limit: u32 },
}

const ACK_EVERY: u64 = 8;
const ACK_DELAY: Duration = Duration::from_millis(10);
const CREDIT_EVERY: u64 = 16 * 1024;
const CREDIT_DELAY: Duration = Duration::from_millis(5);
/// Bulk bytes a sender may write before the first credit arrives.
const INITIAL_BULK_CREDIT: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handshake {
    /// Initiator: Hello sent, waiting for the evaluator's preface and Welcome.
    AwaitWelcome,
    /// Acceptor: waiting for the frontend's Hello.
    AwaitHello,
    Established,
}

#[derive(Clone, Copy, Debug, Default)]
struct AckDebt {
    unacked: u64,
    since: Option<SessionTime>,
}

/// Sender-side accounting of one lane.
#[derive(Clone, Debug, Default)]
struct SentLane {
    /// The session-wide index of the last message written before this link.
    base: u64,
    /// Bytes of complete messages not yet acknowledged, oldest first.
    unacked: VecDeque<u64>,
    /// Bytes already written of the message in progress.
    partial: u64,
    /// Complete messages written on this link.
    completed: u64,
}

/// One transport connection: preface, records, scheduling, flow control and
/// keepalive. It does not know what a session is beyond its counters.
#[derive(Debug)]
struct Link {
    role: Role,
    handshake: Handshake,
    preface_in: Vec<u8>,
    peer_preface: Option<Preface>,
    reader: RecordReader,
    reassembler: Reassembler,
    raw_out: VecDeque<Vec<u8>>,
    scheduler: Scheduler,
    keepalive: Keepalive,
    handshake_deadline: SessionTime,
    rate: RateEstimator,
    sent: [SentLane; 6],
    acks: [AckDebt; 6],
    received_since_credit: u64,
    credit_since: Option<SessionTime>,
    bulk_sent: u64,
    bulk_credited: u64,
    received_payload: [u64; 6],
    /// The session-wide index of the next incoming message on each lane,
    /// when a `ReplayFrom` on this link made it differ from `received + 1`.
    /// A replay ends with its link: the next link continues from what the
    /// resume reported, or from its own `ReplayFrom`.
    replaying: [Option<MessageIndex>; 6],
    own_preface_sent: bool,
    /// The latest `now` this link was given.
    last_now: SessionTime,
    /// Set once the link is ending; the final bytes may still be queued.
    close: Option<CloseOutcome>,
    /// Bytes after which the link is done (a Goodbye or a refusal).
    final_record_pending: bool,
}

/// What a link produced while reading.
enum Incoming {
    Session(Box<SessionMessage>),
    Service(AssembledMessage),
}

impl Incoming {
    /// Whether a frontend sends this only on a link whose Welcome it applied,
    /// so that it proves the frontend holds the token the Welcome issued
    /// (two-phase rotation, DESIGN §6.5). Service messages, acknowledgements,
    /// credit, keepalive and `ReplayFrom` need an established link. A Hello
    /// comes before the Welcome, and a Goodbye or a Refusal can end a link
    /// whose Welcome the frontend never read, or refused.
    fn proves_welcome_applied(&self) -> bool {
        match self {
            Self::Service(_) => true,
            Self::Session(message) => match **message {
                SessionMessage::Ping { .. }
                | SessionMessage::Pong { .. }
                | SessionMessage::Ack { .. }
                | SessionMessage::Credit { .. }
                | SessionMessage::ReplayFrom { .. } => true,
                SessionMessage::Hello(_)
                | SessionMessage::Welcome(_)
                | SessionMessage::Refusal(_)
                | SessionMessage::Goodbye { .. } => false,
            },
        }
    }
}

impl Link {
    fn new(role: Role, now: SessionTime) -> Self {
        Self {
            role,
            handshake: match role {
                Role::Frontend | Role::Relay => Handshake::AwaitWelcome,
                Role::Evaluator => Handshake::AwaitHello,
            },
            preface_in: Vec::with_capacity(PREFACE_LEN),
            peer_preface: None,
            reader: RecordReader::new(PRE_WELCOME_MAX_RECORD),
            reassembler: Reassembler::new(PRE_WELCOME_MAX_RECORD),
            raw_out: VecDeque::new(),
            scheduler: Scheduler::default(),
            keepalive: Keepalive::new(DEFAULT_KEEPALIVE_TIMEOUT, now),
            handshake_deadline: now.saturating_add(HANDSHAKE_DEADLINE),
            rate: RateEstimator::default(),
            sent: Default::default(),
            acks: Default::default(),
            received_since_credit: 0,
            credit_since: None,
            bulk_sent: 0,
            bulk_credited: 0,
            received_payload: [0; 6],
            replaying: [None; 6],
            own_preface_sent: false,
            last_now: now,
            close: None,
            final_record_pending: false,
        }
    }

    fn is_open(&self) -> bool {
        self.close.is_none()
    }

    fn established(&self) -> bool {
        self.handshake == Handshake::Established && self.is_open()
    }

    /// Queue a session record on the control lane.
    fn push_session(&mut self, message: &SessionMessage) {
        self.scheduler.push(
            Lane::Control,
            ServiceSlot::SESSION,
            Compression::Raw,
            message.encode(),
        );
    }

    /// End the link: drop everything unsent and, when `last` is given, send
    /// that one record before closing.
    fn end(&mut self, outcome: CloseOutcome, last: Option<SessionMessage>) {
        if self.close.is_some() {
            return;
        }
        tracing::debug!(role = ?self.role, ?outcome, "session link ends");
        self.scheduler = Scheduler::default();
        self.final_record_pending = false;
        if let Some(message) = last {
            if self.handshake == Handshake::AwaitHello && self.peer_preface.is_some() {
                // The acceptor has not yet sent its preface in this state.
                self.ensure_own_preface();
            }
            self.push_session(&message);
            self.final_record_pending = true;
        }
        self.close = Some(outcome);
    }

    fn ensure_own_preface(&mut self) {
        if self.role == Role::Evaluator && self.raw_out.is_empty() && !self.own_preface_sent {
            self.raw_out
                .push_back(Preface::current(Role::Evaluator).to_bytes().to_vec());
            self.own_preface_sent = true;
        }
    }

    /// Whether the link has nothing more to write and has ended.
    fn finished(&self) -> Option<&CloseOutcome> {
        self.close
            .as_ref()
            .filter(|_| self.raw_out.is_empty() && !self.final_record_pending)
    }

    fn budget(&self) -> FlowBudget {
        let window = byte_window_for(self.rate.bytes_per_second(), self.keepalive.rtt());
        let window_in_flight = Lane::ALL
            .iter()
            .filter(|lane| lane.flow() == LaneFlow::ByteWindow)
            .map(|lane| {
                let sent = &self.sent[lane.index()];
                sent.unacked.iter().sum::<u64>() + sent.partial
            })
            .sum();
        FlowBudget {
            window_in_flight,
            window,
            bulk_uncredited: self.bulk_sent.saturating_sub(self.bulk_credited),
            bulk_credit_available: (INITIAL_BULK_CREDIT + self.bulk_credited)
                .saturating_sub(self.bulk_sent),
        }
    }

    /// Queue acknowledgements and credits that are due.
    fn queue_feedback(&mut self, now: SessionTime, received: &LaneCounts) {
        if !self.established() {
            return;
        }
        for lane in Lane::ALL {
            let debt = self.acks[lane.index()];
            let due = debt.unacked >= ACK_EVERY
                || debt
                    .since
                    .is_some_and(|since| now.saturating_since(since) >= ACK_DELAY);
            if debt.unacked > 0 && due {
                self.push_session(&SessionMessage::Ack {
                    lane,
                    through: received.get(lane),
                });
                self.acks[lane.index()] = AckDebt::default();
            }
        }
        let credit_due = self.received_since_credit >= CREDIT_EVERY
            || self
                .credit_since
                .is_some_and(|since| now.saturating_since(since) >= CREDIT_DELAY);
        if self.received_since_credit > 0 && credit_due {
            self.push_session(&SessionMessage::Credit {
                lane: Lane::Bulk,
                bytes: self.received_since_credit,
            });
            self.received_since_credit = 0;
            self.credit_since = None;
        }
    }

    fn poll_transmit(&mut self, now: SessionTime, received: &LaneCounts) -> Option<Vec<u8>> {
        self.last_now = now;
        if let Some(bytes) = self.raw_out.pop_front() {
            self.keepalive.record_sent(now);
            return Some(bytes);
        }
        if self.close.is_some() && !self.final_record_pending {
            return None;
        }
        if self.close.is_none() {
            self.queue_feedback(now, received);
        }
        let chunk = self
            .scheduler
            .next_chunk(self.rate.chunk_size(), self.budget())?;
        let lane = chunk.header.lane;
        let len = chunk.body.len() as u64;
        if chunk.header.service != ServiceSlot::SESSION {
            let sent = &mut self.sent[lane.index()];
            match chunk.completes {
                Some(total) => {
                    sent.partial = 0;
                    sent.completed += 1;
                    if lane.flow() == LaneFlow::ByteWindow {
                        sent.unacked.push_back(total as u64);
                    }
                }
                None if lane.flow() == LaneFlow::ByteWindow => sent.partial += len,
                None => {}
            }
            if lane == Lane::Bulk {
                self.bulk_sent += len;
            }
        }
        let mut bytes = Vec::with_capacity(HEADER_LEN + chunk.body.len());
        bytes.extend_from_slice(&chunk.header.to_bytes());
        bytes.extend_from_slice(&chunk.body);
        self.keepalive.record_sent(now);
        self.rate
            .on_write(now, bytes.len(), !self.scheduler.is_empty());
        if self.final_record_pending && self.scheduler.is_empty() {
            self.final_record_pending = false;
        }
        Some(bytes)
    }

    /// Feed bytes from the transport: the peer's preface first, then record
    /// bytes, which wait in the reader until [`Self::next_incoming`] parses
    /// them.
    fn push_bytes(&mut self, mut bytes: &[u8], now: SessionTime) -> Result<(), CloseOutcome> {
        self.last_now = now;
        if self.close.is_some() {
            return Ok(());
        }
        if self.peer_preface.is_none() {
            let wanted = PREFACE_LEN - self.preface_in.len();
            let take = wanted.min(bytes.len());
            self.preface_in.extend_from_slice(&bytes[..take]);
            bytes = &bytes[take..];
            if self.preface_in.len() < PREFACE_LEN {
                return Ok(());
            }
            let mut raw = [0u8; PREFACE_LEN];
            raw.copy_from_slice(&self.preface_in);
            let preface = Preface::parse(&raw).map_err(CloseOutcome::BadPreface)?;
            let expected = match self.role {
                Role::Evaluator => [Role::Frontend, Role::Frontend],
                Role::Frontend | Role::Relay => [Role::Evaluator, Role::Evaluator],
            };
            if !expected.contains(&preface.role) {
                return Err(CloseOutcome::BadPreface(PrefaceError::UnknownRole(
                    preface.role as u8,
                )));
            }
            self.peer_preface = Some(preface);
            if preface.major != SESSION_MAJOR {
                return Err(CloseOutcome::RefusedPeer(SessionRefusal::ProtocolMajor {
                    ours: SESSION_MAJOR,
                    theirs: preface.major,
                }));
            }
            self.ensure_own_preface();
        }
        self.reader.push(bytes);
        Ok(())
    }

    /// Parse buffered records up to the next session record or complete
    /// service message. The endpoint handles each item before the next one
    /// is parsed: a Welcome or Hello changes the handshake state and the
    /// record limits, and the records after it in the same read must be read
    /// under the new ones.
    fn next_incoming(&mut self, now: SessionTime) -> Result<Option<Incoming>, CloseOutcome> {
        while self.close.is_none() {
            let Some(record) = self.reader.next_record().map_err(CloseOutcome::Protocol)? else {
                break;
            };
            self.keepalive.record_received(now);
            self.received_payload[record.header.lane.index()] += record.body.len() as u64;
            if record.header.service == ServiceSlot::SESSION {
                if record.header.lane != Lane::Control
                    || record.header.compression != Compression::Raw
                    || record.header.continuation != crate::record::Continuation::Last
                {
                    return Err(CloseOutcome::Protocol(
                        ProtocolError::MalformedSessionRecord,
                    ));
                }
                let message = SessionMessage::decode(&record.body).map_err(|error| {
                    CloseOutcome::Protocol(ProtocolError::SessionRecord(error.to_string()))
                })?;
                return Ok(Some(Incoming::Session(Box::new(message))));
            }
            if !self.established() {
                return Err(CloseOutcome::Protocol(ProtocolError::UnexpectedRecord {
                    record: "service message",
                    state: "handshake",
                }));
            }
            if record.header.lane == Lane::Bulk {
                self.received_since_credit += record.body.len() as u64;
                self.credit_since.get_or_insert(now);
            }
            if let Some(message) = self
                .reassembler
                .accept(record)
                .map_err(CloseOutcome::Protocol)?
            {
                return Ok(Some(Incoming::Service(message)));
            }
        }
        Ok(None)
    }

    /// End of stream from the transport.
    fn eof_outcome(&self) -> CloseOutcome {
        if self.peer_preface.is_none() {
            if self.preface_in.is_empty() {
                return CloseOutcome::PeerClosed;
            }
            return CloseOutcome::BadPreface(PrefaceError::Truncated {
                received_len: self.preface_in.len(),
                received: PeerBytes::of(&self.preface_in),
            });
        }
        let buffered = self.reader.buffered_len() + self.reassembler.pending_bytes();
        if buffered == 0 {
            CloseOutcome::PeerClosed
        } else {
            CloseOutcome::Truncated { buffered }
        }
    }

    fn note_delivered(&mut self, lane: Lane, now: SessionTime) {
        let debt = &mut self.acks[lane.index()];
        debt.unacked += 1;
        debt.since.get_or_insert(now);
    }

    /// The peer acknowledged messages on `lane` through `through` (a
    /// session-wide count); release the bytes this link still holds for them.
    fn acknowledge(
        &mut self,
        lane: Lane,
        through: u64,
        sent_total: u64,
    ) -> Result<(), ProtocolError> {
        if through > sent_total {
            return Err(ProtocolError::AckBeyondSent {
                lane,
                through,
                sent: sent_total,
            });
        }
        let sent = &mut self.sent[lane.index()];
        while !sent.unacked.is_empty() {
            let front = sent.base + sent.completed - sent.unacked.len() as u64 + 1;
            if front > through {
                break;
            }
            sent.unacked.pop_front();
        }
        Ok(())
    }

    fn tick(&mut self, now: SessionTime) -> Option<CloseOutcome> {
        self.last_now = now;
        if self.close.is_some() {
            return None;
        }
        if self.handshake != Handshake::Established {
            if now >= self.handshake_deadline {
                return Some(CloseOutcome::HandshakeTimeout);
            }
            return None;
        }
        match self.keepalive.poll(now) {
            KeepaliveAction::Nothing => None,
            KeepaliveAction::SendPing { nonce } => {
                self.push_session(&SessionMessage::Ping { nonce });
                None
            }
            KeepaliveAction::Expired => Some(CloseOutcome::KeepaliveTimeout),
        }
    }

    /// `bases` are the session-wide indices of the last message each lane
    /// sent before this link; queued replays come after them.
    fn establish(
        &mut self,
        limits: &Limits,
        keepalive_timeout: Duration,
        bases: LaneCounts,
        now: SessionTime,
    ) {
        for lane in Lane::ALL {
            self.sent[lane.index()].base = bases.get(lane);
        }
        self.handshake = Handshake::Established;
        self.reader.set_limit(limits.max_record);
        self.reassembler.set_max_message(limits.max_message);
        self.keepalive = Keepalive::new(keepalive_timeout, now);
    }
}

/// Counters shared by every link of one session.
#[derive(Clone, Debug, Default)]
struct Ledger {
    sent: LaneCounts,
    received: LaneCounts,
}

impl Ledger {
    /// Number the next incoming message on `lane`: deliver it, with the
    /// indices a replay skipped, or drop it as a duplicate of a message
    /// already delivered. `replaying` is the link's replay position on the
    /// lane ([`Link::replaying`]). A lane whose indices are used up is a
    /// typed error.
    fn admit(
        &mut self,
        lane: Lane,
        replaying: &mut Option<MessageIndex>,
    ) -> Result<Admit, ProtocolError> {
        let received = self.received.get(lane);
        let index = match replaying.take() {
            Some(index) => index,
            None => MessageIndex::after(received).ok_or(ProtocolError::LaneExhausted { lane })?,
        };
        if index.get() <= received {
            // The replay continues below `received` until it reaches it.
            *replaying = MessageIndex::after(index.get()).filter(|next| next.get() <= received);
            return Ok(Admit::Duplicate);
        }
        // `index > received` here, so `received + 1` cannot overflow; the
        // replay skipped the messages between them, if any.
        let gap = (index.get() - received > 1).then(|| (received + 1, index.get() - 1));
        self.received.set(lane, index.get());
        Ok(Admit::Deliver { index, gap })
    }

    /// A peer's count of messages received on each lane (a resume cursor or
    /// a warm Welcome) never exceeds what this side sent on the session.
    fn check_peer_received(&self, received: &LaneCounts) -> Result<(), ProtocolError> {
        match Lane::ALL
            .into_iter()
            .find(|lane| received.get(*lane) > self.sent.get(*lane))
        {
            Some(lane) => Err(ProtocolError::ReceivedBeyondSent {
                lane,
                received: received.get(lane),
                sent: self.sent.get(lane),
            }),
            None => Ok(()),
        }
    }
}

enum Admit {
    Deliver {
        index: MessageIndex,
        gap: Option<(u64, u64)>,
    },
    Duplicate,
}

// ---------------------------------------------------------------------------
// Frontend
// ---------------------------------------------------------------------------

/// How much input the frontend keeps for replay (DESIGN §6.19: 5 s / 256).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReplayBudget {
    pub events: usize,
    pub age: Duration,
}

impl Default for ReplayBudget {
    fn default() -> Self {
        Self {
            events: 256,
            age: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug)]
pub struct FrontendConfig {
    pub build: BuildId,
    pub incarnation: FrontendIncarnation,
    pub services: Vec<ServiceOffer>,
    pub limits: Limits,
    pub replay: ReplayBudget,
}

impl FrontendConfig {
    /// Offers display service 1.0 with the default limits.
    #[must_use]
    pub fn new(build: BuildId, incarnation: FrontendIncarnation) -> Self {
        Self {
            build,
            incarnation,
            services: vec![ServiceOffer::new(
                ServiceName::new("display").expect("a valid service name"),
                1,
                MinorRange::new(0, 0).expect("a non-empty range"),
            )],
            limits: Limits::default(),
            replay: ReplayBudget::default(),
        }
    }
}

/// How a frontend attaches when it has no session to resume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConnectIntent {
    InProcess,
    Inherited,
    Offer {
        label: DisplayLabel,
        mode: OfferMode,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontendEvent {
    Established {
        session: SessionId,
        welcome: Box<SessionWelcome>,
    },
    Refused(SessionRefusal),
    Message(Delivered),
    Closed(CloseOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResumeError {
    #[error("no session to resume")]
    NoSession,
    #[error("the session is not resumable")]
    NotResumable,
}

#[derive(Clone, Debug)]
struct ReplayEntry {
    index: MessageIndex,
    service: ServiceSlot,
    compression: Compression,
    body: Vec<u8>,
    sent_at: SessionTime,
}

#[derive(Debug)]
struct FrontendSession {
    id: SessionId,
    token: Option<ResumeToken>,
    ledger: Ledger,
    accepted: Vec<ServiceAccept>,
    limits: Limits,
    replay: VecDeque<ReplayEntry>,
}

/// The dialing side of one session.
#[derive(Debug)]
pub struct FrontendEndpoint {
    config: FrontendConfig,
    link: Option<Link>,
    session: Option<FrontendSession>,
    events: VecDeque<FrontendEvent>,
    closed_reported: bool,
    last_now: SessionTime,
}

impl FrontendEndpoint {
    #[must_use]
    pub fn new(config: FrontendConfig) -> Self {
        Self {
            config,
            link: None,
            session: None,
            events: VecDeque::new(),
            closed_reported: false,
            last_now: SessionTime::ZERO,
        }
    }

    /// Start a new link that asks for a new session.
    pub fn connect(&mut self, intent: ConnectIntent, now: SessionTime) {
        self.session = None;
        let intent = match intent {
            ConnectIntent::InProcess => SessionIntent::InProcess,
            ConnectIntent::Inherited => SessionIntent::Inherited,
            ConnectIntent::Offer { label, mode } => SessionIntent::Offer { label, mode },
        };
        self.start_link(intent, now);
    }

    /// Start a new link that resumes the current session.
    pub fn resume(&mut self, now: SessionTime) -> Result<(), ResumeError> {
        let session = self.session.as_ref().ok_or(ResumeError::NoSession)?;
        let token = session.token.clone().ok_or(ResumeError::NotResumable)?;
        let intent = SessionIntent::Resume {
            session: session.id,
            token,
            cursors: ResumeCursors {
                received: session.ledger.received,
            },
        };
        self.start_link(intent, now);
        Ok(())
    }

    /// Adopt a session persisted by an earlier process (its token is stored
    /// in the state directory, never on a command line). A resume after this
    /// is cold when the incarnation changed.
    pub fn restore_session(&mut self, session: SessionId, token: ResumeToken) {
        self.session = Some(FrontendSession {
            id: session,
            token: Some(token),
            ledger: Ledger::default(),
            accepted: Vec::new(),
            limits: self.config.limits,
            replay: VecDeque::new(),
        });
    }

    fn start_link(&mut self, intent: SessionIntent, now: SessionTime) {
        self.last_now = now;
        let mut link = Link::new(Role::Frontend, now);
        link.raw_out
            .push_back(Preface::current(Role::Frontend).to_bytes().to_vec());
        link.own_preface_sent = true;
        let hello = SessionMessage::Hello(SessionHello {
            build: self.config.build.clone(),
            incarnation: self.config.incarnation,
            services: self.config.services.clone(),
            intent,
            limits: self.config.limits,
            ext: Vec::new(),
        });
        link.push_session(&hello);
        self.link = Some(link);
        self.closed_reported = false;
    }

    #[must_use]
    pub fn session(&self) -> Option<SessionId> {
        self.session.as_ref().map(|session| session.id)
    }

    #[must_use]
    pub fn resume_token(&self) -> Option<&ResumeToken> {
        self.session
            .as_ref()
            .and_then(|session| session.token.as_ref())
    }

    /// Payload bytes received on `lane` over the current link, partial
    /// chunks included.
    #[must_use]
    pub fn received_payload_bytes(&self, lane: Lane) -> u64 {
        self.link
            .as_ref()
            .map_or(0, |link| link.received_payload[lane.index()])
    }

    pub fn poll_event(&mut self) -> Option<FrontendEvent> {
        self.events.pop_front()
    }

    /// Send a service message. Input sent while the link is down waits in the
    /// replay buffer and goes out on the next warm resume.
    pub fn send(
        &mut self,
        lane: Lane,
        service: ServiceSlot,
        body: Vec<u8>,
    ) -> Result<MessageIndex, SendError> {
        let now = self.last_now;
        let session = self.session.as_mut().ok_or(SendError::NotConnected)?;
        if !session.accepted.iter().any(|accept| accept.slot == service) {
            return Err(SendError::UnknownService(service));
        }
        if body.len() > session.limits.max_message as usize {
            return Err(SendError::TooLarge {
                len: body.len(),
                limit: session.limits.max_message,
            });
        }
        let connected = self.link.as_ref().is_some_and(Link::established);
        if !connected && lane != Lane::Input {
            return Err(SendError::NotConnected);
        }
        let index = session.ledger.sent.advance(lane);
        if lane == Lane::Input {
            session.replay.push_back(ReplayEntry {
                index,
                service,
                compression: Compression::Raw,
                body: body.clone(),
                sent_at: now,
            });
            let budget = self.config.replay;
            while session.replay.len() > budget.events
                || session
                    .replay
                    .front()
                    .is_some_and(|entry| now.saturating_since(entry.sent_at) > budget.age)
            {
                session.replay.pop_front();
            }
        }
        if connected && let Some(link) = self.link.as_mut() {
            link.scheduler.push(lane, service, Compression::Raw, body);
        }
        Ok(index)
    }

    /// Close the link with a Goodbye. `Normal` ends the session.
    pub fn close(&mut self, reason: CloseReason, now: SessionTime) {
        self.last_now = now;
        if let Some(link) = self.link.as_mut() {
            link.end(
                CloseOutcome::Local(reason),
                Some(SessionMessage::Goodbye { reason }),
            );
        }
        self.report_close_if_ended();
    }

    pub fn poll_transmit(&mut self, now: SessionTime) -> Option<Vec<u8>> {
        self.last_now = now;
        let received = self
            .session
            .as_ref()
            .map(|session| session.ledger.received)
            .unwrap_or_default();
        let bytes = self.link.as_mut()?.poll_transmit(now, &received);
        self.report_close_if_ended();
        bytes
    }

    pub fn tick(&mut self, now: SessionTime) {
        self.last_now = now;
        if let Some(link) = self.link.as_mut()
            && let Some(outcome) = link.tick(now)
        {
            link.end(outcome, None);
        }
        self.report_close_if_ended();
    }

    pub fn receive_eof(&mut self, now: SessionTime) {
        self.last_now = now;
        if let Some(link) = self.link.as_mut()
            && link.is_open()
        {
            let outcome = link.eof_outcome();
            link.end(outcome, None);
        }
        self.report_close_if_ended();
    }

    pub fn transport_failed(&mut self, kind: std::io::ErrorKind, now: SessionTime) {
        self.last_now = now;
        if let Some(link) = self.link.as_mut() {
            link.end(CloseOutcome::TransportFailed(kind), None);
        }
        self.report_close_if_ended();
    }

    pub fn receive(&mut self, bytes: &[u8], now: SessionTime) {
        self.last_now = now;
        let Some(link) = self.link.as_mut() else {
            return;
        };
        if let Err(outcome) = link.push_bytes(bytes, now) {
            self.fail(outcome);
        }
        // One record at a time: handling a Welcome establishes the link
        // before the records that follow it in the same read are parsed.
        while let Some(link) = self.link.as_mut() {
            match link.next_incoming(now) {
                Ok(Some(item)) => {
                    if let Err(outcome) = self.handle(item, now) {
                        self.fail(outcome);
                        break;
                    }
                }
                Ok(None) => break,
                Err(outcome) => {
                    self.fail(outcome);
                    break;
                }
            }
        }
        self.report_close_if_ended();
    }

    fn fail(&mut self, outcome: CloseOutcome) {
        let Some(link) = self.link.as_mut() else {
            return;
        };
        let last = match &outcome {
            CloseOutcome::Protocol(_) => Some(SessionMessage::Goodbye {
                reason: CloseReason::ProtocolViolation,
            }),
            CloseOutcome::RefusedPeer(refusal) => Some(SessionMessage::Refusal(refusal.clone())),
            _ => None,
        };
        if let CloseOutcome::RefusedByPeer(refusal) = &outcome {
            self.events
                .push_back(FrontendEvent::Refused(refusal.clone()));
        }
        link.end(outcome, last);
    }

    fn report_close_if_ended(&mut self) {
        if self.closed_reported {
            return;
        }
        if let Some(outcome) = self.link.as_ref().and_then(|link| link.close.clone()) {
            self.closed_reported = true;
            self.events.push_back(FrontendEvent::Closed(outcome));
        }
    }

    fn handle(&mut self, item: Incoming, now: SessionTime) -> Result<(), CloseOutcome> {
        let handshake = self
            .link
            .as_ref()
            .expect("handled only while a link exists")
            .handshake;
        match item {
            Incoming::Session(message) => match *message {
                SessionMessage::Welcome(welcome) if handshake == Handshake::AwaitWelcome => {
                    self.establish(welcome, now)
                }
                message => self.handle_session(handshake, message, now),
            },
            Incoming::Service(message) => {
                let link = self
                    .link
                    .as_mut()
                    .expect("handled only while a link exists");
                let session = self
                    .session
                    .as_mut()
                    .expect("established links have a session");
                if !session
                    .accepted
                    .iter()
                    .any(|accept| accept.slot == message.service)
                {
                    return Err(CloseOutcome::Protocol(ProtocolError::UnknownService(
                        message.service.get(),
                    )));
                }
                if let Admit::Deliver { index, .. } = session
                    .ledger
                    .admit(message.lane, &mut link.replaying[message.lane.index()])
                    .map_err(CloseOutcome::Protocol)?
                {
                    link.note_delivered(message.lane, now);
                    self.events.push_back(FrontendEvent::Message(Delivered {
                        lane: message.lane,
                        service: message.service,
                        index,
                        compression: message.compression,
                        body: message.body,
                    }));
                }
                Ok(())
            }
        }
    }

    fn handle_session(
        &mut self,
        handshake: Handshake,
        message: SessionMessage,
        now: SessionTime,
    ) -> Result<(), CloseOutcome> {
        let link = self
            .link
            .as_mut()
            .expect("handled only while a link exists");
        match (handshake, message) {
            (_, SessionMessage::Refusal(refusal)) => Err(CloseOutcome::RefusedByPeer(refusal)),
            (_, SessionMessage::Goodbye { reason }) => Err(CloseOutcome::Goodbye(reason)),
            (Handshake::Established, SessionMessage::Ping { nonce }) => {
                link.push_session(&SessionMessage::Pong { nonce });
                Ok(())
            }
            (Handshake::Established, SessionMessage::Pong { nonce }) => {
                link.keepalive.record_pong(nonce, now);
                Ok(())
            }
            (Handshake::Established, SessionMessage::Ack { lane, through }) => {
                let session = self
                    .session
                    .as_mut()
                    .expect("established links have a session");
                link.acknowledge(lane, through, session.ledger.sent.get(lane))
                    .map_err(CloseOutcome::Protocol)?;
                if lane == Lane::Input {
                    while session
                        .replay
                        .front()
                        .is_some_and(|entry| entry.index.get() <= through)
                    {
                        session.replay.pop_front();
                    }
                }
                Ok(())
            }
            (
                Handshake::Established,
                SessionMessage::Credit {
                    lane: Lane::Bulk,
                    bytes,
                },
            ) => {
                link.bulk_credited = link.bulk_credited.saturating_add(bytes);
                Ok(())
            }
            (Handshake::Established, SessionMessage::ReplayFrom { lane, first }) => {
                link.replaying[lane.index()] = Some(first);
                Ok(())
            }
            (handshake, message) => Err(CloseOutcome::Protocol(ProtocolError::UnexpectedRecord {
                record: message.name(),
                state: handshake_name(handshake),
            })),
        }
    }

    fn establish(&mut self, welcome: SessionWelcome, now: SessionTime) -> Result<(), CloseOutcome> {
        // A Welcome carries the minimum of both offers; limits above this
        // side's offer would bound nothing it chose.
        if self.config.limits.agree(welcome.limits) != welcome.limits {
            return Err(CloseOutcome::Protocol(ProtocolError::LimitsBeyondOffer {
                offered: self.config.limits,
                welcome: welcome.limits,
            }));
        }
        let link = self.link.as_mut().expect("a Welcome arrives on a link");
        let token = match &welcome.resumability {
            Resumability::Resumable { token, .. } => Some(token.clone()),
            Resumability::NotResumable => None,
        };
        let keepalive = Duration::from_millis(u64::from(welcome.keepalive_timeout_ms));
        let same_session = self
            .session
            .as_ref()
            .is_some_and(|session| session.id == welcome.session);
        let mut bases = LaneCounts::default();
        match (&welcome.resume, same_session) {
            (Some(ResumeOutcome::Warm { received }), true) => {
                let session = self.session.as_mut().expect("checked above");
                // Checked before anything changes: a refused Welcome leaves
                // the session resumable with the token this side presented,
                // which the evaluator keeps accepting, because the Goodbye
                // that ends the link is no proof of the new one.
                session
                    .ledger
                    .check_peer_received(received)
                    .map_err(CloseOutcome::Protocol)?;
                // Lanes the evaluator re-synchronizes continue from what it
                // received; input is replayed above received_through.
                for lane in Lane::ALL {
                    if lane != Lane::Input {
                        session.ledger.sent.set(lane, received.get(lane));
                    }
                }
                let through = received.get(Lane::Input);
                while session
                    .replay
                    .front()
                    .is_some_and(|entry| entry.index.get() <= through)
                {
                    session.replay.pop_front();
                }
                bases = session.ledger.sent;
                if let Some(first) = session.replay.front().map(|entry| entry.index) {
                    bases.set(Lane::Input, first.get() - 1);
                    link.push_session(&SessionMessage::ReplayFrom {
                        lane: Lane::Input,
                        first,
                    });
                    for entry in &session.replay {
                        link.scheduler.push(
                            Lane::Input,
                            entry.service,
                            entry.compression,
                            entry.body.clone(),
                        );
                    }
                } else if session.ledger.sent.get(Lane::Input) > through
                    && let Some(first) = MessageIndex::after(session.ledger.sent.get(Lane::Input))
                {
                    // Input above what the evaluator has was dropped by the
                    // replay budget: say where the stream continues. (A lane
                    // whose indices are used up has nowhere to continue; the
                    // evaluator refuses its next message as `LaneExhausted`.)
                    link.push_session(&SessionMessage::ReplayFrom {
                        lane: Lane::Input,
                        first,
                    });
                }
                session.token = token;
                session.accepted = welcome.accepted.clone();
                session.limits = welcome.limits;
            }
            _ => {
                self.session = Some(FrontendSession {
                    id: welcome.session,
                    token,
                    ledger: Ledger::default(),
                    accepted: welcome.accepted.clone(),
                    limits: welcome.limits,
                    replay: VecDeque::new(),
                });
            }
        }
        link.establish(&welcome.limits, keepalive, bases, now);
        self.events.push_back(FrontendEvent::Established {
            session: welcome.session,
            welcome: Box::new(welcome),
        });
        Ok(())
    }

    /// Whether the current link has ended and flushed its last bytes.
    #[must_use]
    pub fn closed(&self) -> Option<&CloseOutcome> {
        self.link.as_ref().and_then(Link::finished)
    }
}

fn handshake_name(handshake: Handshake) -> &'static str {
    match handshake {
        Handshake::AwaitWelcome => "awaiting Welcome",
        Handshake::AwaitHello => "awaiting Hello",
        Handshake::Established => "established",
    }
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
pub struct EvaluatorConfig {
    pub build: BuildId,
    pub policy: BuildPolicy,
    /// Services this evaluator speaks.
    pub services: Vec<ServiceOffer>,
    pub limits: Limits,
    pub keepalive_timeout: Duration,
    pub durability: Durability,
    /// How long a detached dialed session stays resumable; `None` is forever.
    pub grace: Option<Duration>,
}

impl EvaluatorConfig {
    /// Schema policy, display service 1.0, default limits, 10 s keepalive.
    #[must_use]
    pub fn new(build: BuildId) -> Self {
        Self {
            build,
            policy: BuildPolicy::Schema,
            services: vec![ServiceOffer::new(
                ServiceName::new("display").expect("a valid service name"),
                1,
                MinorRange::new(0, 0).expect("a non-empty range"),
            )],
            limits: Limits::default(),
            keepalive_timeout: DEFAULT_KEEPALIVE_TIMEOUT,
            durability: Durability::Unknown,
            grace: None,
        }
    }
}

/// How a link attached to its session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Attachment {
    New,
    Warm { received: LaneCounts },
    Cold,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvaluatorEvent {
    Attached {
        session: SessionId,
        link: LinkId,
        attachment: Attachment,
    },
    Message {
        session: SessionId,
        message: Delivered,
    },
    /// A resumable session lost its link.
    Detached {
        session: SessionId,
        outcome: CloseOutcome,
    },
    /// A session ended: closed normally, not resumable, or its grace expired.
    SessionEnded {
        session: SessionId,
    },
    LinkClosed {
        link: LinkId,
        outcome: CloseOutcome,
    },
    /// A resumed sender could not replay messages `first..=last` of `lane`.
    ReplayGap {
        session: SessionId,
        lane: Lane,
        first: u64,
        last: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionState {
    Connected { link: LinkId },
    Detached { since: SessionTime },
}

#[derive(Debug)]
struct TokenPair {
    current: ResumeToken,
    /// The token presented to obtain `current`. It stays accepted until a
    /// record arrives that only a frontend which applied that Welcome sends
    /// ([`Incoming::proves_welcome_applied`]), so a Welcome lost in flight,
    /// unread or refused never strands the session (DESIGN §6.5). Only the
    /// session's connected link carries such a record.
    previous: Option<ResumeToken>,
}

#[derive(Debug)]
struct EvalSession {
    label: Option<DisplayLabel>,
    incarnation: FrontendIncarnation,
    tokens: Option<TokenPair>,
    state: SessionState,
    ledger: Ledger,
    accepted: Vec<ServiceAccept>,
    limits: Limits,
}

#[derive(Debug)]
struct EvalLink {
    link: Link,
    locality: LinkLocality,
    session: Option<SessionId>,
    closed_reported: bool,
}

/// The listener side: every session and every link of one evaluator.
#[derive(Debug)]
pub struct EvaluatorEndpoint {
    config: EvaluatorConfig,
    links: BTreeMap<LinkId, EvalLink>,
    sessions: BTreeMap<SessionId, EvalSession>,
    events: VecDeque<EvaluatorEvent>,
    next_link: u64,
    next_session: u64,
}

impl EvaluatorEndpoint {
    #[must_use]
    pub fn new(config: EvaluatorConfig) -> Self {
        Self {
            config,
            links: BTreeMap::new(),
            sessions: BTreeMap::new(),
            events: VecDeque::new(),
            next_link: 1,
            next_session: 1,
        }
    }

    /// A new transport arrived. The acceptor sends nothing until the peer's
    /// preface proves it speaks the protocol.
    pub fn accept(&mut self, locality: LinkLocality, now: SessionTime) -> LinkId {
        let id = LinkId(self.next_link);
        self.next_link += 1;
        self.links.insert(
            id,
            EvalLink {
                link: Link::new(Role::Evaluator, now),
                locality,
                session: None,
                closed_reported: false,
            },
        );
        id
    }

    pub fn poll_event(&mut self) -> Option<EvaluatorEvent> {
        self.events.pop_front()
    }

    #[must_use]
    pub fn session_state(&self, session: SessionId) -> Option<SessionState> {
        self.sessions.get(&session).map(|session| session.state)
    }

    /// The chunk size the session's current link uses.
    #[must_use]
    pub fn chunk_size(&self, session: SessionId) -> Option<usize> {
        let SessionState::Connected { link } = self.sessions.get(&session)?.state else {
            return None;
        };
        Some(self.links.get(&link)?.link.rate.chunk_size())
    }

    /// Whether `link` has ended and flushed its last bytes.
    #[must_use]
    pub fn link_closed(&self, link: LinkId) -> Option<&CloseOutcome> {
        self.links.get(&link).and_then(|link| link.link.finished())
    }

    pub fn send(
        &mut self,
        session: SessionId,
        lane: Lane,
        service: ServiceSlot,
        body: Vec<u8>,
    ) -> Result<MessageIndex, SendError> {
        let record = self
            .sessions
            .get_mut(&session)
            .ok_or(SendError::UnknownSession)?;
        if !record.accepted.iter().any(|accept| accept.slot == service) {
            return Err(SendError::UnknownService(service));
        }
        if body.len() > record.limits.max_message as usize {
            return Err(SendError::TooLarge {
                len: body.len(),
                limit: record.limits.max_message,
            });
        }
        let SessionState::Connected { link } = record.state else {
            return Err(SendError::Detached);
        };
        let link = self.links.get_mut(&link).ok_or(SendError::Detached)?;
        let index = record.ledger.sent.advance(lane);
        link.link
            .scheduler
            .push(lane, service, Compression::Raw, body);
        Ok(index)
    }

    /// End a session for good (GNU's `delete-terminal` path).
    pub fn close_session(&mut self, session: SessionId, reason: CloseReason) {
        let Some(record) = self.sessions.remove(&session) else {
            return;
        };
        if let SessionState::Connected { link } = record.state {
            self.end_link(
                link,
                CloseOutcome::Local(reason),
                Some(SessionMessage::Goodbye { reason }),
            );
        }
        self.events
            .push_back(EvaluatorEvent::SessionEnded { session });
    }

    pub fn poll_transmit(&mut self, link: LinkId, now: SessionTime) -> Option<Vec<u8>> {
        let entry = self.links.get_mut(&link)?;
        let received = entry
            .session
            .and_then(|session| self.sessions.get(&session))
            .map(|session| session.ledger.received)
            .unwrap_or_default();
        entry.link.poll_transmit(now, &received)
    }

    pub fn tick(&mut self, now: SessionTime) {
        let due: Vec<(LinkId, CloseOutcome)> = self
            .links
            .iter_mut()
            .filter_map(|(id, entry)| entry.link.tick(now).map(|outcome| (*id, outcome)))
            .collect();
        for (link, outcome) in due {
            self.end_link(link, outcome, None);
        }
        if let Some(grace) = self.config.grace {
            let expired: Vec<SessionId> = self
                .sessions
                .iter()
                .filter_map(|(id, session)| match session.state {
                    SessionState::Detached { since } if now.saturating_since(since) > grace => {
                        Some(*id)
                    }
                    _ => None,
                })
                .collect();
            for session in expired {
                self.sessions.remove(&session);
                self.events
                    .push_back(EvaluatorEvent::SessionEnded { session });
            }
        }
    }

    /// Forget a link that has ended and flushed its last bytes; its outcome
    /// was reported as [`EvaluatorEvent::LinkClosed`].
    pub fn release_link(&mut self, link: LinkId) {
        if self
            .links
            .get(&link)
            .is_some_and(|entry| entry.link.finished().is_some())
        {
            self.links.remove(&link);
        }
    }

    pub fn receive_eof(&mut self, link: LinkId, _now: SessionTime) {
        let Some(entry) = self.links.get(&link) else {
            return;
        };
        if entry.link.is_open() {
            let outcome = entry.link.eof_outcome();
            self.end_link(link, outcome, None);
        }
    }

    pub fn transport_failed(&mut self, link: LinkId, kind: std::io::ErrorKind, _now: SessionTime) {
        self.end_link(link, CloseOutcome::TransportFailed(kind), None);
    }

    pub fn receive(&mut self, link: LinkId, bytes: &[u8], now: SessionTime) {
        let Some(entry) = self.links.get_mut(&link) else {
            return;
        };
        if let Err(outcome) = entry.link.push_bytes(bytes, now) {
            self.fail_link(link, outcome);
            return;
        }
        // One record at a time: admitting a Hello establishes the link
        // before the records that follow it in the same read are parsed.
        while let Some(entry) = self.links.get_mut(&link) {
            let item = match entry.link.next_incoming(now) {
                Ok(Some(item)) => item,
                Ok(None) => return,
                Err(outcome) => {
                    self.fail_link(link, outcome);
                    return;
                }
            };
            if let Err(outcome) = self.handle(link, item, now) {
                self.fail_link(link, outcome);
                return;
            }
        }
    }

    fn fail_link(&mut self, link: LinkId, outcome: CloseOutcome) {
        let last = match &outcome {
            CloseOutcome::BadPreface(_) => {
                if let Some(entry) = self.links.get_mut(&link) {
                    entry.link.raw_out.clear();
                    entry
                        .link
                        .raw_out
                        .push_back(REFUSAL_LINE.as_bytes().to_vec());
                }
                None
            }
            CloseOutcome::Protocol(_) => Some(SessionMessage::Goodbye {
                reason: CloseReason::ProtocolViolation,
            }),
            CloseOutcome::RefusedPeer(refusal) => Some(SessionMessage::Refusal(refusal.clone())),
            _ => None,
        };
        self.end_link(link, outcome, last);
    }

    /// End `link` and detach or end its session.
    fn end_link(&mut self, link: LinkId, outcome: CloseOutcome, last: Option<SessionMessage>) {
        let Some(entry) = self.links.get_mut(&link) else {
            return;
        };
        if !entry.link.is_open() {
            return;
        }
        entry.link.end(outcome.clone(), last);
        if !entry.closed_reported {
            entry.closed_reported = true;
            self.events.push_back(EvaluatorEvent::LinkClosed {
                link,
                outcome: outcome.clone(),
            });
        }
        let Some(session) = entry.session else {
            return;
        };
        let Some(record) = self.sessions.get_mut(&session) else {
            return;
        };
        if record.state != (SessionState::Connected { link }) {
            return;
        }
        let ends_session = record.tokens.is_none()
            || matches!(
                outcome,
                CloseOutcome::Goodbye(CloseReason::Normal | CloseReason::Shutdown)
                    | CloseOutcome::Local(CloseReason::Normal | CloseReason::Shutdown)
            );
        if ends_session {
            self.sessions.remove(&session);
            self.events
                .push_back(EvaluatorEvent::SessionEnded { session });
        } else {
            let since = entry.link.last_now;
            record.state = SessionState::Detached { since };
            self.events
                .push_back(EvaluatorEvent::Detached { session, outcome });
        }
    }

    fn handle(
        &mut self,
        link: LinkId,
        item: Incoming,
        now: SessionTime,
    ) -> Result<(), CloseOutcome> {
        let entry = self
            .links
            .get_mut(&link)
            .expect("handled only for live links");
        if item.proves_welcome_applied()
            && let Some(session) = entry.session.and_then(|id| self.sessions.get_mut(&id))
            && let Some(tokens) = session.tokens.as_mut()
        {
            // The frontend holds the token its Welcome issued; the one it
            // presented stops being accepted.
            tokens.previous = None;
        }
        let handshake = entry.link.handshake;
        match item {
            Incoming::Session(message) => {
                if let SessionMessage::Hello(hello) = *message {
                    if handshake == Handshake::AwaitHello {
                        self.admit(link, hello, now);
                        return Ok(());
                    }
                    return Err(CloseOutcome::Protocol(ProtocolError::UnexpectedRecord {
                        record: "Hello",
                        state: handshake_name(handshake),
                    }));
                }
                let session = entry.session.and_then(|id| self.sessions.get_mut(&id));
                match (handshake, *message) {
                    (_, SessionMessage::Refusal(refusal)) => {
                        Err(CloseOutcome::RefusedByPeer(refusal))
                    }
                    (_, SessionMessage::Goodbye { reason }) => Err(CloseOutcome::Goodbye(reason)),
                    (Handshake::Established, SessionMessage::Ping { nonce }) => {
                        entry.link.push_session(&SessionMessage::Pong { nonce });
                        Ok(())
                    }
                    (Handshake::Established, SessionMessage::Pong { nonce }) => {
                        entry.link.keepalive.record_pong(nonce, now);
                        Ok(())
                    }
                    (Handshake::Established, SessionMessage::Ack { lane, through }) => {
                        let sent = session.map_or(0, |session| session.ledger.sent.get(lane));
                        entry
                            .link
                            .acknowledge(lane, through, sent)
                            .map_err(CloseOutcome::Protocol)
                    }
                    (
                        Handshake::Established,
                        SessionMessage::Credit {
                            lane: Lane::Bulk,
                            bytes,
                        },
                    ) => {
                        entry.link.bulk_credited = entry.link.bulk_credited.saturating_add(bytes);
                        Ok(())
                    }
                    (Handshake::Established, SessionMessage::ReplayFrom { lane, first }) => {
                        entry.link.replaying[lane.index()] = Some(first);
                        Ok(())
                    }
                    (handshake, message) => {
                        Err(CloseOutcome::Protocol(ProtocolError::UnexpectedRecord {
                            record: message.name(),
                            state: handshake_name(handshake),
                        }))
                    }
                }
            }
            Incoming::Service(message) => {
                let Some(session_id) = entry.session else {
                    return Err(CloseOutcome::Protocol(ProtocolError::UnexpectedRecord {
                        record: "service message",
                        state: "unattached",
                    }));
                };
                let session = self
                    .sessions
                    .get_mut(&session_id)
                    .expect("attached links name live sessions");
                if !session
                    .accepted
                    .iter()
                    .any(|accept| accept.slot == message.service)
                {
                    return Err(CloseOutcome::Protocol(ProtocolError::UnknownService(
                        message.service.get(),
                    )));
                }
                if let Admit::Deliver { index, gap } = session
                    .ledger
                    .admit(
                        message.lane,
                        &mut entry.link.replaying[message.lane.index()],
                    )
                    .map_err(CloseOutcome::Protocol)?
                {
                    entry.link.note_delivered(message.lane, now);
                    if let Some((first, last)) = gap {
                        self.events.push_back(EvaluatorEvent::ReplayGap {
                            session: session_id,
                            lane: message.lane,
                            first,
                            last,
                        });
                    }
                    self.events.push_back(EvaluatorEvent::Message {
                        session: session_id,
                        message: Delivered {
                            lane: message.lane,
                            service: message.service,
                            index,
                            compression: message.compression,
                            body: message.body,
                        },
                    });
                }
                Ok(())
            }
        }
    }

    fn refuse(&mut self, link: LinkId, refusal: SessionRefusal) {
        self.end_link(
            link,
            CloseOutcome::RefusedPeer(refusal.clone()),
            Some(SessionMessage::Refusal(refusal)),
        );
    }

    fn new_session_id(&mut self) -> SessionId {
        let id = SessionId::new(NonZeroU64::new(self.next_session).expect("starts at one"));
        self.next_session += 1;
        id
    }

    fn admit(&mut self, link: LinkId, hello: SessionHello, now: SessionTime) {
        let locality = self.links[&link].locality;
        let mut slot = 0u8;
        let accepted = match negotiate(
            &self.config.build,
            self.config.policy,
            &self.config.services,
            &hello,
            || {
                slot += 1;
                ServiceSlot::new(slot)
            },
        ) {
            Ok(accepted) => accepted,
            Err(refusal) => return self.refuse(link, refusal),
        };
        let limits = self.config.limits.agree(hello.limits);
        let resumable = locality == LinkLocality::Dialed;
        // Draw the token before any session state changes, so a failure
        // leaves nothing registered (no squatted label).
        let token = if resumable {
            match ResumeToken::random() {
                Ok(token) => Some(token),
                Err(_) => {
                    return self.refuse(
                        link,
                        SessionRefusal::ResourceLimit {
                            kind: crate::handshake::LimitKind::Sessions,
                        },
                    );
                }
            }
        } else {
            None
        };
        // The token the frontend presented stays valid until it proves it
        // holds the new one (two-phase rotation); new and adopted sessions
        // start without one.
        let mut previous_token = None;
        let (session, attachment) = match hello.intent {
            SessionIntent::InProcess if locality == LinkLocality::InProcess => {
                (self.new_session_id(), Attachment::New)
            }
            SessionIntent::Inherited if locality != LinkLocality::InProcess => {
                (self.new_session_id(), Attachment::New)
            }
            SessionIntent::Offer { label, mode } if locality == LinkLocality::Dialed => {
                let holder = self
                    .sessions
                    .iter()
                    .find(|(_, session)| session.label.as_ref() == Some(&label))
                    .map(|(id, session)| (*id, session.state));
                match (holder, mode) {
                    (Some((_, state)), OfferMode::Exclusive) => {
                        let state = match state {
                            SessionState::Connected { .. } => LabelState::Connected,
                            SessionState::Detached { .. } => LabelState::Detached,
                        };
                        return self.refuse(link, SessionRefusal::LabelInUse { label, state });
                    }
                    (Some((id, _)), OfferMode::Takeover) => (id, Attachment::Cold),
                    (None, _) => {
                        let id = self.new_session_id();
                        self.sessions.insert(
                            id,
                            EvalSession {
                                label: Some(label),
                                incarnation: hello.incarnation,
                                tokens: None,
                                state: SessionState::Detached { since: now },
                                ledger: Ledger::default(),
                                accepted: Vec::new(),
                                limits,
                            },
                        );
                        (id, Attachment::New)
                    }
                }
            }
            SessionIntent::Resume {
                session,
                token,
                cursors,
            } if locality == LinkLocality::Dialed => {
                let Some(record) = self.sessions.get(&session) else {
                    return self.refuse(link, SessionRefusal::ResumeUnknown);
                };
                if let (Some(grace), SessionState::Detached { since }) =
                    (self.config.grace, record.state)
                    && now.saturating_since(since) > grace
                {
                    self.sessions.remove(&session);
                    self.events
                        .push_back(EvaluatorEvent::SessionEnded { session });
                    return self.refuse(link, SessionRefusal::ResumeExpired);
                }
                let valid = record.tokens.as_ref().is_some_and(|tokens| {
                    tokens.current == token || tokens.previous.as_ref() == Some(&token)
                });
                if !valid {
                    return self.refuse(link, SessionRefusal::Unauthorized);
                }
                let attachment = if record.incarnation == hello.incarnation {
                    // Checked before anything changes: a refused resume
                    // leaves the session as it was.
                    if let Err(error) = record.ledger.check_peer_received(&cursors.received) {
                        return self.fail_link(link, CloseOutcome::Protocol(error));
                    }
                    Attachment::Warm {
                        received: record.ledger.received,
                    }
                } else {
                    Attachment::Cold
                };
                previous_token = Some(token);
                let record = self.sessions.get_mut(&session).expect("found above");
                if let Attachment::Warm { .. } = attachment {
                    for lane in Lane::ALL {
                        record.ledger.sent.set(lane, cursors.received.get(lane));
                    }
                }
                (session, attachment)
            }
            _ => return self.refuse(link, SessionRefusal::Unauthorized),
        };

        // Take the session over from a link that still holds it. The old link
        // is detached from the session first, so ending it detaches nothing.
        if let Some(SessionState::Connected { link: old }) =
            self.sessions.get(&session).map(|record| record.state)
            && old != link
        {
            if let Some(old_entry) = self.links.get_mut(&old) {
                old_entry.session = None;
            }
            self.end_link(
                old,
                CloseOutcome::Local(CloseReason::TakenOver),
                Some(SessionMessage::Goodbye {
                    reason: CloseReason::TakenOver,
                }),
            );
        }

        let record = self.sessions.entry(session).or_insert_with(|| EvalSession {
            label: None,
            incarnation: hello.incarnation,
            tokens: None,
            state: SessionState::Connected { link },
            ledger: Ledger::default(),
            accepted: Vec::new(),
            limits,
        });
        if attachment == Attachment::Cold {
            record.ledger = Ledger::default();
        }
        record.incarnation = hello.incarnation;
        record.state = SessionState::Connected { link };
        record.accepted = accepted.clone();
        record.limits = limits;
        record.tokens = token.clone().map(|current| TokenPair {
            current,
            previous: previous_token,
        });
        let welcome = SessionWelcome {
            session,
            resumability: match token {
                Some(token) => Resumability::Resumable {
                    token,
                    grace_secs: self
                        .config
                        .grace
                        .map(|grace| u32::try_from(grace.as_secs()).unwrap_or(u32::MAX)),
                },
                None => Resumability::NotResumable,
            },
            accepted,
            limits,
            keepalive_timeout_ms: u32::try_from(self.config.keepalive_timeout.as_millis())
                .unwrap_or(u32::MAX),
            durability: self.config.durability,
            build: self.config.build.clone(),
            resume: match &attachment {
                Attachment::New => None,
                Attachment::Warm { received } => Some(ResumeOutcome::Warm {
                    received: *received,
                }),
                Attachment::Cold => Some(ResumeOutcome::Cold),
            },
            ext: Vec::new(),
        };
        let keepalive = self.config.keepalive_timeout;
        let bases = record.ledger.sent;
        let entry = self.links.get_mut(&link).expect("admitting a live link");
        entry.link.push_session(&SessionMessage::Welcome(welcome));
        entry.link.establish(&limits, keepalive, bases, now);
        entry.session = Some(session);
        self.events.push_back(EvaluatorEvent::Attached {
            session,
            link,
            attachment,
        });
    }
}
