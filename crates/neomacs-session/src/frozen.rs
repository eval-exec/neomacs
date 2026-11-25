//! The one codec of the frozen session records: bincode 2 `standard()`
//! (varint, little endian), as for every message body (DESIGN §6.9).
//!
//! Decoding is total and its allocation bounded: bincode claims each value's
//! in-memory size against its limit, and a string's or byte vector's declared
//! length before allocating it. The limit is the smallest bucket of at least
//! 16 times the input, so valid input always fits and an oversized declared
//! length is refused unallocated.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Why a frozen record could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrozenDecodeError {
    #[error("malformed session record: {0}")]
    Malformed(String),
    #[error("{trailing} bytes follow a complete session record")]
    TrailingBytes { trailing: usize },
}

pub(crate) fn encode<T: Serialize>(value: &T) -> Vec<u8> {
    bincode::serde::encode_to_vec(value, bincode::config::standard())
        .expect("session records are plain data and always encode")
}

macro_rules! decode_with_limit {
    ($bytes:expr, $($limit:literal),+) => {{
        let len = $bytes.len();
        // bincode claims up to 16 bytes per input byte (a one-byte `u128`
        // varint), so the limit is at least 16 times the input.
        let claim = len.saturating_mul(16);
        $(
            if claim <= $limit {
                return bincode::serde::decode_from_slice(
                    $bytes,
                    bincode::config::standard().with_limit::<$limit>(),
                )
                .map_err(|error| FrozenDecodeError::Malformed(error.to_string()))
                .and_then(|(value, consumed)| finish(value, consumed, len));
            }
        )+
        Err(FrozenDecodeError::Malformed(format!("{len} bytes exceed every session limit")))
    }};
}

fn finish<T>(value: T, consumed: usize, len: usize) -> Result<T, FrozenDecodeError> {
    if consumed == len {
        Ok(value)
    } else {
        Err(FrozenDecodeError::TrailingBytes {
            trailing: len - consumed,
        })
    }
}

/// Decode one complete frozen record body; trailing bytes are an error.
pub(crate) fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, FrozenDecodeError> {
    decode_with_limit!(bytes, 16384, 1048576, 16777216, 268435456, 1073741824)
}
