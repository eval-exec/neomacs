//! The one codec of the display service (DESIGN §6.9).
//!
//! Bodies are bincode 2 `standard()` (varint, little endian) through serde.
//! Decoding is total: every malformed input is a [`WireError`], and
//! allocation is bounded in proportion to the input. bincode claims each
//! value's in-memory size against its limit (up to 16 bytes for one varint
//! byte, one byte per string byte) and a string's or byte vector's declared
//! length before allocating it; the limit is the smallest bucket of at least
//! 16 times the input, so valid input always fits and a declared length
//! beyond it is refused unallocated. Other sequences preallocate at most
//! serde's cautious 1 MiB. Compression is raw deflate at level 1 behind a
//! varint of the inflated length, which bounds inflation.

use std::io::{Read, Write};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Why a body could not be encoded or decoded.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum WireError {
    #[error("cannot encode: {0}")]
    Encode(String),
    #[error("malformed body: {0}")]
    Decode(String),
    #[error("{trailing} bytes follow a complete body")]
    TrailingBytes { trailing: usize },
    #[error("body of {len} bytes exceeds the {limit}-byte message limit")]
    TooLarge { len: usize, limit: usize },
    #[error("compressed body declares {declared} bytes, above the {limit}-byte limit")]
    InflatedTooLarge { declared: u64, limit: usize },
    #[error("compressed body inflates to {actual} bytes, not the declared {declared}")]
    InflatedLengthMismatch { declared: u64, actual: u64 },
    #[error("malformed compressed body: {0}")]
    Inflate(String),
}

/// The canonical encoding of plain data. Wire types contain no maps with an
/// unspecified order and no floats other than bit-exact ones, so two equal
/// values always encode to the same bytes (digests and golden fixtures rely
/// on it).
pub(crate) fn encode_canonical<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>, WireError> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .map_err(|error| WireError::Encode(error.to_string()))
}

/// The most bincode claims per input byte (a one-byte `u128` varint).
const CLAIM_PER_BYTE: usize = 16;

macro_rules! decode_with_limit {
    ($bytes:expr, $($limit:literal),+) => {{
        let len = $bytes.len();
        let claim = len.saturating_mul(CLAIM_PER_BYTE);
        $(
            if claim <= $limit {
                return bincode::serde::decode_from_slice(
                    $bytes,
                    bincode::config::standard().with_limit::<$limit>(),
                )
                .map_err(|error| WireError::Decode(error.to_string()))
                .and_then(|(value, consumed)| {
                    if consumed == len {
                        Ok(value)
                    } else {
                        Err(WireError::TrailingBytes { trailing: len - consumed })
                    }
                });
            }
        )+
        Err(WireError::TooLarge { len, limit: 64 << 20 })
    }};
}

/// Decode one complete body (at most 64 MiB, the hard message cap).
pub(crate) fn decode_bounded<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, WireError> {
    decode_with_limit!(
        bytes, 16384, 65536, 262144, 1048576, 4194304, 16777216, 67108864, 268435456, 1073741824
    )
}

/// The display service codec with the session's agreed message limit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WireCodec {
    max_message: usize,
}

impl Default for WireCodec {
    /// The default `max_message` (DESIGN §6.18).
    fn default() -> Self {
        Self::new(8 << 20)
    }
}

impl WireCodec {
    #[must_use]
    pub const fn new(max_message: usize) -> Self {
        Self { max_message }
    }

    #[must_use]
    pub const fn max_message(&self) -> usize {
        self.max_message
    }

    pub fn encode<T: Serialize + ?Sized>(&self, value: &T) -> Result<Vec<u8>, WireError> {
        let bytes = encode_canonical(value)?;
        if bytes.len() > self.max_message {
            return Err(WireError::TooLarge {
                len: bytes.len(),
                limit: self.max_message,
            });
        }
        Ok(bytes)
    }

    pub fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, WireError> {
        if bytes.len() > self.max_message {
            return Err(WireError::TooLarge {
                len: bytes.len(),
                limit: self.max_message,
            });
        }
        decode_bounded(bytes)
    }

    /// `varint inflated_len ‖ raw deflate` at level 1.
    pub fn deflate(&self, body: &[u8]) -> Result<Vec<u8>, WireError> {
        let mut out = encode_canonical(&(body.len() as u64))?;
        let mut encoder = flate2::write::DeflateEncoder::new(out, flate2::Compression::fast());
        encoder
            .write_all(body)
            .map_err(|error| WireError::Encode(error.to_string()))?;
        out = encoder
            .finish()
            .map_err(|error| WireError::Encode(error.to_string()))?;
        Ok(out)
    }

    /// Inflate a body; the declared length is checked against the message
    /// limit before inflating, and the output may not exceed it.
    pub fn inflate(&self, body: &[u8]) -> Result<Vec<u8>, WireError> {
        let (declared, consumed): (u64, usize) =
            bincode::serde::decode_from_slice(body, bincode::config::standard())
                .map_err(|error| WireError::Inflate(error.to_string()))?;
        if declared > self.max_message as u64 {
            return Err(WireError::InflatedTooLarge {
                declared,
                limit: self.max_message,
            });
        }
        let mut out = Vec::with_capacity(declared as usize);
        let mut decoder = flate2::read::DeflateDecoder::new(&body[consumed..]).take(declared + 1);
        decoder
            .read_to_end(&mut out)
            .map_err(|error| WireError::Inflate(error.to_string()))?;
        if out.len() as u64 != declared {
            return Err(WireError::InflatedLengthMismatch {
                declared,
                actual: out.len() as u64,
            });
        }
        Ok(out)
    }
}
