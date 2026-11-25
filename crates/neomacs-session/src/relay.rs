//! The frozen relay records (DESIGN §6.3; R2-13, R2-31).
//!
//! They carry user data inside the stream instead of on a remote shell's
//! command line, and make the relay a typed participant. Each record is at
//! most 64 KiB, checked when encoding and before decoding.

use serde::{Deserialize, Serialize};

use crate::frozen;
use crate::handshake::{BuildId, DisplayLabel, Durability, Extension, NameError};

/// The relay preamble budget (DESIGN §6.18).
pub const MAX_RELAY_RECORD: usize = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RelayError {
    #[error("relay record of {len} bytes exceeds {MAX_RELAY_RECORD} bytes")]
    TooLarge { len: usize },
    #[error("malformed relay record: {0}")]
    Malformed(String),
}

/// A `server.el` server name: `[A-Za-z0-9._-]{1,64}`, or an absolute path.
/// `..` and other path tricks are refused in the name form.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ServerName(String);

impl ServerName {
    pub fn new(name: &str) -> Result<Self, NameError> {
        let simple = (1..=64).contains(&name.len())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
            && name != "."
            && name != "..";
        let absolute = name.starts_with('/')
            && !name.split('/').any(|part| part == "..")
            && !name.chars().any(|c| c.is_control());
        if simple || absolute {
            Ok(Self(name.to_owned()))
        } else {
            Err(NameError {
                kind: "server name",
                value: name.to_owned(),
                rule: "[A-Za-z0-9._-]{1,64} or an absolute path",
            })
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ServerName {
    type Error = NameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<ServerName> for String {
    fn from(value: ServerName) -> Self {
        value.0
    }
}

/// One file to visit, as `emacsclient FILE` or `+LINE:COLUMN FILE`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RelayFile {
    pub path: Vec<u8>,
    pub line: Option<u32>,
    pub column: Option<u32>,
}

/// Whether the relay should ask `server.el` for a new frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelayResume {
    /// A new attach: the relay sends the `server.el` request.
    Fresh,
    /// A resume: no `server.el` request is sent.
    Resume,
}

/// Frontend → relay: the first record after the frontend's preface.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayRequest {
    /// `None` is `server`.
    pub server_name: Option<ServerName>,
    pub label: DisplayLabel,
    pub resume: RelayResume,
    /// The remote working directory for `-dir`, as bytes.
    pub dir: Option<Vec<u8>>,
    pub files: Vec<RelayFile>,
    pub ext: Vec<Extension>,
}

/// Relay → listener: the listener answers a relay's preface once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeReply {
    pub build: BuildId,
    pub durability: Durability,
    pub server_socket: Vec<u8>,
}

/// Whether the relay started the daemon it connected to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DaemonStart {
    AlreadyRunning,
    Started,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayRefusal {
    DaemonUnreachable { path: Vec<u8>, errno: i32 },
    ServerSocketUnreachable { path: Vec<u8> },
    UnsafeRendezvous { path: Vec<u8> },
    DaemonTooOld { build: BuildId },
    StartFailed { detail: String },
}

/// Relay → frontend, before any daemon byte is spliced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelayStatus {
    Connected {
        durability: Durability,
        daemon_build: BuildId,
        started: DaemonStart,
    },
    Refused {
        reason: RelayRefusal,
    },
}

fn encode_bounded<T: Serialize>(value: &T) -> Result<Vec<u8>, RelayError> {
    let bytes = frozen::encode(value);
    if bytes.len() > MAX_RELAY_RECORD {
        return Err(RelayError::TooLarge { len: bytes.len() });
    }
    Ok(bytes)
}

fn decode_bounded<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, RelayError> {
    if bytes.len() > MAX_RELAY_RECORD {
        return Err(RelayError::TooLarge { len: bytes.len() });
    }
    frozen::decode(bytes).map_err(|error| RelayError::Malformed(error.to_string()))
}

impl RelayRequest {
    pub fn encode(&self) -> Result<Vec<u8>, RelayError> {
        encode_bounded(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RelayError> {
        decode_bounded(bytes)
    }
}

impl RelayStatus {
    pub fn encode(&self) -> Result<Vec<u8>, RelayError> {
        encode_bounded(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RelayError> {
        decode_bounded(bytes)
    }
}

impl ProbeReply {
    pub fn encode(&self) -> Result<Vec<u8>, RelayError> {
        encode_bounded(self)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, RelayError> {
        decode_bounded(bytes)
    }
}
