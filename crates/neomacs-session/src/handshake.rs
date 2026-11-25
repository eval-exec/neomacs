//! The frozen session records and the build policy (DESIGN §6.5-§6.6).
//!
//! `SessionHello`, `SessionWelcome`, `SessionRefusal` and `Goodbye` never
//! change shape, so any two versions can refuse each other cleanly. Additions
//! travel in the `ext` TLV lists.

use std::num::NonZeroU64;

use serde::{Deserialize, Serialize};

use crate::frozen;
use crate::lane::{Lane, LaneCounts, MessageIndex};
use crate::record::{
    Compression, Continuation, DEFAULT_MAX_MESSAGE, DEFAULT_MAX_RECORD, HARD_MAX_MESSAGE,
    HARD_MAX_RECORD, HEADER_LEN, PRE_WELCOME_MAX_RECORD, RecordHeader, ServiceSlot,
};

/// The SHA-256, truncated to 128 bits, of a service's canonical golden
/// fixture corpus. The session layer compares it opaquely; the display wire
/// crate computes the display service's value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct WireSchemaId([u8; 16]);

impl WireSchemaId {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn to_bytes(self) -> [u8; 16] {
        self.0
    }
}

/// What a peer is: its version, git commit and wire schema. Both are always
/// exchanged and shown.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BuildId {
    pub version: String,
    pub commit: String,
    pub wire_schema: WireSchemaId,
}

impl BuildId {
    #[must_use]
    pub fn new(version: &str, commit: &str, wire_schema: WireSchemaId) -> Self {
        Self {
            version: version.to_owned(),
            commit: commit.to_owned(),
            wire_schema,
        }
    }
}

impl std::fmt::Display for BuildId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "neomacs {} ({}, schema ", self.version, self.commit)?;
        for byte in self.wire_schema.0 {
            write!(f, "{byte:02x}")?;
        }
        f.write_str(")")
    }
}

/// Which peers an evaluator accepts (`neomacs-display-version-policy`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildPolicy {
    /// Same session major and the same `WireSchemaId`; services intersected.
    Schema,
    /// Additionally the same version and commit.
    ExactBuild,
}

/// Random per frontend process start; a change means a cold resume (D34).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FrontendIncarnation(u64);

impl FrontendIncarnation {
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// A fresh incarnation for this process start.
    pub fn random() -> Result<Self, getrandom::Error> {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes)?;
        Ok(Self(u64::from_le_bytes(bytes)))
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One session on one evaluator; allocated by the evaluator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct SessionId(NonZeroU64);

impl SessionId {
    #[must_use]
    pub const fn new(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// 256 bits from the operating system's CSPRNG. Never printed.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResumeToken([u8; 32]);

impl ResumeToken {
    pub fn random() -> Result<Self, getrandom::Error> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for ResumeToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ResumeToken(..)")
    }
}

/// Why a name was rejected.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid {kind} {value:?}: {rule}")]
pub struct NameError {
    pub kind: &'static str,
    pub value: String,
    pub rule: &'static str,
}

/// A service name: `[a-z][a-z0-9-]{0,31}`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ServiceName(String);

impl ServiceName {
    pub fn new(name: &str) -> Result<Self, NameError> {
        let mut chars = name.chars();
        let valid = chars.next().is_some_and(|first| first.is_ascii_lowercase())
            && name.len() <= 32
            && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if valid {
            Ok(Self(name.to_owned()))
        } else {
            Err(NameError {
                kind: "service name",
                value: name.to_owned(),
                rule: "[a-z][a-z0-9-]{0,31}",
            })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ServiceName {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<ServiceName> for String {
    fn from(value: ServiceName) -> Self {
        value.0
    }
}

/// A display label: `[A-Za-z][A-Za-z0-9._@+-]{0,62}` (DESIGN §7.3). It never
/// contains a colon, so `neo:LABEL` can never look like an X display name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DisplayLabel(String);

impl DisplayLabel {
    pub fn new(label: &str) -> Result<Self, NameError> {
        let mut chars = label.chars();
        let valid = chars
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic())
            && label.len() <= 63
            && chars.all(|c| c.is_ascii_alphanumeric() || "._@+-".contains(c));
        if valid {
            Ok(Self(label.to_owned()))
        } else {
            Err(NameError {
                kind: "display label",
                value: label.to_owned(),
                rule: "[A-Za-z][A-Za-z0-9._@+-]{0,62}",
            })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for DisplayLabel {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<DisplayLabel> for String {
    fn from(value: DisplayLabel) -> Self {
        value.0
    }
}

/// An inclusive, non-empty range of service minors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "(u16, u16)", into = "(u16, u16)")]
pub struct MinorRange {
    min: u16,
    max: u16,
}

impl MinorRange {
    #[must_use]
    pub const fn new(min: u16, max: u16) -> Option<Self> {
        if min <= max {
            Some(Self { min, max })
        } else {
            None
        }
    }

    #[must_use]
    pub const fn min(self) -> u16 {
        self.min
    }

    #[must_use]
    pub const fn max(self) -> u16 {
        self.max
    }
}

impl TryFrom<(u16, u16)> for MinorRange {
    type Error = &'static str;

    fn try_from((min, max): (u16, u16)) -> Result<Self, Self::Error> {
        Self::new(min, max).ok_or("an empty minor range")
    }
}

impl From<MinorRange> for (u16, u16) {
    fn from(range: MinorRange) -> Self {
        (range.min, range.max)
    }
}

/// A service the sender can speak: one major, a range of minors.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceOffer {
    pub name: ServiceName,
    pub major: u16,
    pub minors: MinorRange,
}

impl ServiceOffer {
    #[must_use]
    pub const fn new(name: ServiceName, major: u16, minors: MinorRange) -> Self {
        Self {
            name,
            major,
            minors,
        }
    }
}

/// A service the evaluator accepted, the agreed revision, and its slot.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ServiceAccept {
    pub name: ServiceName,
    pub major: u16,
    pub minor: u16,
    pub slot: ServiceSlot,
}

/// Whether a token-less offer may adopt a label another session holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OfferMode {
    Exclusive,
    Takeover,
}

/// Counts of complete messages a resuming side has received, per lane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResumeCursors {
    pub received: LaneCounts,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionIntent {
    InProcess,
    /// A spawned child on an inherited descriptor.
    Inherited,
    /// A dial-in through the display listener.
    Offer {
        label: DisplayLabel,
        mode: OfferMode,
    },
    Resume {
        session: SessionId,
        token: ResumeToken,
        cursors: ResumeCursors,
    },
}

/// Limits a side can handle; the minimum of both applies (DESIGN §6.18).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Limits {
    pub max_record: u32,
    pub max_message: u32,
    pub store_rows: u32,
    pub store_bytes: u64,
    pub asset_cache_bytes: u64,
    pub input_edges: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_record: DEFAULT_MAX_RECORD,
            max_message: DEFAULT_MAX_MESSAGE,
            store_rows: 16_384,
            store_bytes: 64 * 1024 * 1024,
            asset_cache_bytes: 512 * 1024 * 1024,
            input_edges: 4096,
        }
    }
}

impl Limits {
    /// The minimum of both sides, with the hard caps applied.
    #[must_use]
    pub fn agree(self, other: Self) -> Self {
        Self {
            max_record: self
                .max_record
                .min(other.max_record)
                .clamp(PRE_WELCOME_MAX_RECORD, HARD_MAX_RECORD),
            max_message: self
                .max_message
                .min(other.max_message)
                .clamp(PRE_WELCOME_MAX_RECORD, HARD_MAX_MESSAGE),
            store_rows: self.store_rows.min(other.store_rows),
            store_bytes: self.store_bytes.min(other.store_bytes),
            asset_cache_bytes: self.asset_cache_bytes.min(other.asset_cache_bytes),
            input_edges: self.input_edges.min(other.input_edges),
        }
    }
}

/// A TLV extension; unknown keys are ignored.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Extension {
    pub key: u16,
    pub value: Vec<u8>,
}

/// Frontend → evaluator: the first record on every transport.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionHello {
    pub build: BuildId,
    pub incarnation: FrontendIncarnation,
    pub services: Vec<ServiceOffer>,
    pub intent: SessionIntent,
    pub limits: Limits,
    pub ext: Vec<Extension>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Resumability {
    Resumable {
        token: ResumeToken,
        grace_secs: Option<u32>,
    },
    NotResumable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DurabilityReason {
    SessionScope,
    NoLinger,
    RuntimeDirRemovedAtLogout,
}

/// Whether the daemon outlives the user's login (D31).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Durability {
    Persistent,
    UntilLogout { reason: DurabilityReason },
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResumeOutcome {
    /// Same incarnation: lanes continue; `received` are the evaluator's counts,
    /// so the frontend replays input above `received[Input]`.
    Warm { received: LaneCounts },
    /// A new incarnation or an adopted label: everything starts afresh.
    Cold,
}

/// Evaluator → frontend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWelcome {
    pub session: SessionId,
    pub resumability: Resumability,
    pub accepted: Vec<ServiceAccept>,
    pub limits: Limits,
    pub keepalive_timeout_ms: u32,
    pub durability: Durability,
    pub build: BuildId,
    pub resume: Option<ResumeOutcome>,
    pub ext: Vec<Extension>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LabelState {
    Connected,
    Detached,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LimitKind {
    Handshakes,
    Sessions,
    Requests,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum SessionRefusal {
    #[error("protocol major {theirs} is not {ours}")]
    ProtocolMajor { ours: u16, theirs: u16 },
    #[error("wire schema differs: this side is {ours}, the peer is {theirs}")]
    SchemaMismatch {
        ours: Box<BuildId>,
        theirs: Box<BuildId>,
    },
    #[error(
        "build differs under the exact-build policy: this side is {ours}, the peer is {theirs}"
    )]
    BuildMismatch {
        ours: Box<BuildId>,
        theirs: Box<BuildId>,
    },
    #[error("service {} is not offered", name.as_str())]
    ServiceUnavailable { name: ServiceName },
    #[error("display label {} is in use ({state:?})", label.as_str())]
    LabelInUse {
        label: DisplayLabel,
        state: LabelState,
    },
    #[error("no such session to resume")]
    ResumeUnknown,
    #[error("the session's resume grace expired")]
    ResumeExpired,
    #[error("the offer expired before a display claimed it")]
    OfferExpired,
    #[error("the resume token is not valid")]
    Unauthorized,
    #[error("resource limit reached: {kind:?}")]
    ResourceLimit { kind: LimitKind },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CloseReason {
    Normal,
    TakenOver,
    ProtocolViolation,
    KeepaliveTimeout,
    Detached,
    Shutdown,
}

/// Every record a session itself sends, on lane 0 with service slot 0.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionMessage {
    Hello(SessionHello),
    Welcome(SessionWelcome),
    Refusal(SessionRefusal),
    Goodbye {
        reason: CloseReason,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    /// Complete messages received on `lane`, through this count.
    Ack {
        lane: Lane,
        through: u64,
    },
    /// More bytes the receiver accepts on a credit-controlled lane.
    Credit {
        lane: Lane,
        bytes: u64,
    },
    /// The next message on `lane` is message `first` of the stream: a resumed
    /// sender replays from here and the receiver drops what it already has.
    /// Indices start at 1, so a zero is undecodable.
    ReplayFrom {
        lane: Lane,
        first: MessageIndex,
    },
}

impl SessionMessage {
    /// The name used in protocol errors.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Hello(_) => "Hello",
            Self::Welcome(_) => "Welcome",
            Self::Refusal(_) => "Refusal",
            Self::Goodbye { .. } => "Goodbye",
            Self::Ping { .. } => "Ping",
            Self::Pong { .. } => "Pong",
            Self::Ack { .. } => "Ack",
            Self::Credit { .. } => "Credit",
            Self::ReplayFrom { .. } => "ReplayFrom",
        }
    }

    /// The record body.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        frozen::encode(self)
    }

    pub fn decode(body: &[u8]) -> Result<Self, frozen::FrozenDecodeError> {
        frozen::decode(body)
    }

    /// The complete record (header and body) as one unchunked control-lane
    /// record of the session service.
    pub fn to_record(&self) -> Result<Vec<u8>, SessionRecordTooLarge> {
        let body = self.encode();
        let len = u32::try_from(body.len())
            .ok()
            .filter(|len| *len <= PRE_WELCOME_MAX_RECORD)
            .ok_or(SessionRecordTooLarge { len: body.len() })?;
        let header = RecordHeader {
            len,
            lane: Lane::Control,
            compression: Compression::Raw,
            continuation: Continuation::Last,
            service: ServiceSlot::SESSION,
        };
        let mut record = Vec::with_capacity(HEADER_LEN + body.len());
        record.extend_from_slice(&header.to_bytes());
        record.extend_from_slice(&body);
        Ok(record)
    }
}

/// A session record must fit the pre-Welcome record limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("session record of {len} bytes exceeds the 64 KiB session record limit")]
pub struct SessionRecordTooLarge {
    pub len: usize,
}

/// The evaluator's decision on a Hello's build and services.
pub fn negotiate(
    ours: &BuildId,
    policy: BuildPolicy,
    supported: &[ServiceOffer],
    hello: &SessionHello,
    next_slot: impl FnMut() -> ServiceSlot,
) -> Result<Vec<ServiceAccept>, SessionRefusal> {
    if hello.build.wire_schema != ours.wire_schema {
        return Err(SessionRefusal::SchemaMismatch {
            ours: Box::new(ours.clone()),
            theirs: Box::new(hello.build.clone()),
        });
    }
    if policy == BuildPolicy::ExactBuild
        && (hello.build.version != ours.version || hello.build.commit != ours.commit)
    {
        return Err(SessionRefusal::BuildMismatch {
            ours: Box::new(ours.clone()),
            theirs: Box::new(hello.build.clone()),
        });
    }
    intersect_services(supported, &hello.services, next_slot)
}

fn intersect_services(
    supported: &[ServiceOffer],
    offered: &[ServiceOffer],
    mut next_slot: impl FnMut() -> ServiceSlot,
) -> Result<Vec<ServiceAccept>, SessionRefusal> {
    let mut accepted = Vec::new();
    let mut major_mismatch = None;
    for ours in supported {
        let Some(theirs) = offered.iter().find(|offer| offer.name == ours.name) else {
            continue;
        };
        if theirs.major != ours.major {
            major_mismatch.get_or_insert(SessionRefusal::ProtocolMajor {
                ours: ours.major,
                theirs: theirs.major,
            });
            continue;
        }
        let min = ours.minors.min().max(theirs.minors.min());
        let max = ours.minors.max().min(theirs.minors.max());
        if min > max {
            continue;
        }
        accepted.push(ServiceAccept {
            name: ours.name.clone(),
            major: ours.major,
            minor: max,
            slot: next_slot(),
        });
    }
    if !accepted.is_empty() {
        return Ok(accepted);
    }
    Err(
        major_mismatch.unwrap_or_else(|| SessionRefusal::ServiceUnavailable {
            name: supported
                .first()
                .map(|service| service.name.clone())
                .unwrap_or_else(|| ServiceName("display".to_owned())),
        }),
    )
}
