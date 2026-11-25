//! The 16-byte preface both sides send first; frozen forever (DESIGN §6.3).
//!
//! ```text
//! 0..8    magic          "NEOSESS\0"
//! 8..10   session_major  u16 LE = 1
//! 10..12  session_minor  u16 LE = 0
//! 12      role           u8: 1 evaluator, 2 frontend, 3 relay
//! 13      flags          u8 = 0 (non-zero → refuse)
//! 14..16  reserved       u16 = 0
//! ```

pub const PREFACE_LEN: usize = 16;
pub const MAGIC: [u8; 8] = *b"NEOSESS\0";
pub const SESSION_MAJOR: u16 = 1;
pub const SESSION_MINOR: u16 = 0;

/// The one line an acceptor writes, instead of its preface, to a peer whose
/// first bytes are not a preface, so a wrong ssh command or a `server.el`
/// socket gives a readable error.
pub const REFUSAL_LINE: &str = "neomacs: this endpoint speaks the Neomacs display session \
                                protocol (NEOSESS/1); the peer sent something else\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Role {
    Evaluator = 1,
    Frontend = 2,
    Relay = 3,
}

impl Role {
    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Evaluator),
            2 => Some(Self::Frontend),
            3 => Some(Self::Relay),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preface {
    pub major: u16,
    pub minor: u16,
    pub role: Role,
}

/// The first bytes a peer sent, rendered printable for an error message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerBytes(String);

impl PeerBytes {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(bytes.escape_ascii().to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PrefaceError {
    #[error("the peer is not a Neomacs display session; it sent \"{}\"", received.as_str())]
    BadMagic { received: PeerBytes },
    #[error("the peer closed after {} of the 16 preface bytes (\"{}\")", received_len, received.as_str())]
    Truncated {
        received_len: usize,
        received: PeerBytes,
    },
    #[error("unknown session role {0}")]
    UnknownRole(u8),
    #[error("preface flags {0:#04x} are not zero")]
    NonZeroFlags(u8),
    #[error("preface reserved bytes {0:#06x} are not zero")]
    NonZeroReserved(u16),
}

impl Preface {
    #[must_use]
    pub const fn current(role: Role) -> Self {
        Self {
            major: SESSION_MAJOR,
            minor: SESSION_MINOR,
            role,
        }
    }

    #[must_use]
    pub fn to_bytes(self) -> [u8; PREFACE_LEN] {
        let mut bytes = [0u8; PREFACE_LEN];
        bytes[0..8].copy_from_slice(&MAGIC);
        bytes[8..10].copy_from_slice(&self.major.to_le_bytes());
        bytes[10..12].copy_from_slice(&self.minor.to_le_bytes());
        bytes[12] = self.role as u8;
        bytes
    }

    pub fn parse(bytes: &[u8; PREFACE_LEN]) -> Result<Self, PrefaceError> {
        if bytes[0..8] != MAGIC {
            return Err(PrefaceError::BadMagic {
                received: PeerBytes::of(bytes),
            });
        }
        let role = Role::from_byte(bytes[12]).ok_or(PrefaceError::UnknownRole(bytes[12]))?;
        if bytes[13] != 0 {
            return Err(PrefaceError::NonZeroFlags(bytes[13]));
        }
        let reserved = u16::from_le_bytes([bytes[14], bytes[15]]);
        if reserved != 0 {
            return Err(PrefaceError::NonZeroReserved(reserved));
        }
        Ok(Self {
            major: u16::from_le_bytes([bytes[8], bytes[9]]),
            minor: u16::from_le_bytes([bytes[10], bytes[11]]),
            role,
        })
    }
}
