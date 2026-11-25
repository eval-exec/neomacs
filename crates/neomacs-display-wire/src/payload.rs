//! One payload type, two representations (DESIGN §6.1).
//!
//! In process a payload is the shared value; on a byte stream it is its
//! encoding. Both serialize to the same bytes (a length-prefixed byte
//! string), so the session and the sync layers never look inside, and a
//! receiver decodes the value only when it uses it.

use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::codec::{WireError, decode_bounded, encode_canonical};

#[derive(Debug)]
pub enum Payload<T: ?Sized> {
    Shared(Arc<T>),
    Encoded(Arc<[u8]>),
}

impl<T: ?Sized> Clone for Payload<T> {
    fn clone(&self) -> Self {
        match self {
            Self::Shared(value) => Self::Shared(Arc::clone(value)),
            Self::Encoded(bytes) => Self::Encoded(Arc::clone(bytes)),
        }
    }
}

impl<T> Payload<T> {
    #[must_use]
    pub fn shared(value: T) -> Self {
        Self::Shared(Arc::new(value))
    }
}

impl<T: Serialize + DeserializeOwned> Payload<T> {
    /// The value, decoding an encoded payload.
    pub fn value(&self) -> Result<Arc<T>, WireError> {
        match self {
            Self::Shared(value) => Ok(Arc::clone(value)),
            Self::Encoded(bytes) => decode_bounded(bytes).map(Arc::new),
        }
    }

    /// The encoded bytes, encoding a shared payload.
    pub fn bytes(&self) -> Result<Arc<[u8]>, WireError> {
        match self {
            Self::Shared(value) => encode_canonical(&**value).map(Arc::from),
            Self::Encoded(bytes) => Ok(Arc::clone(bytes)),
        }
    }
}

impl Payload<[u8]> {
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::Shared(Arc::from(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Shared(bytes) | Self::Encoded(bytes) => bytes,
        }
    }
}

impl PartialEq for Payload<[u8]> {
    fn eq(&self, other: &Self) -> bool {
        self.as_bytes() == other.as_bytes()
    }
}

impl<T: Serialize> Serialize for Payload<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Shared(value) => {
                let bytes = encode_canonical(&**value).map_err(serde::ser::Error::custom)?;
                serializer.serialize_bytes(&bytes)
            }
            Self::Encoded(bytes) => serializer.serialize_bytes(bytes),
        }
    }
}

impl Serialize for Payload<[u8]> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.as_bytes())
    }
}

struct EncodedBytes;

impl serde::de::Visitor<'_> for EncodedBytes {
    type Value = Arc<[u8]>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a byte string")
    }

    fn visit_bytes<E: serde::de::Error>(self, bytes: &[u8]) -> Result<Self::Value, E> {
        Ok(Arc::from(bytes))
    }

    fn visit_byte_buf<E: serde::de::Error>(self, bytes: Vec<u8>) -> Result<Self::Value, E> {
        Ok(Arc::from(bytes))
    }
}

/// The name under which a payload deserializes as a newtype. The schema
/// tracer reads a newtype of this name as a one-element sequence.
pub(crate) const PAYLOAD_NEWTYPE: &str = "Payload";

struct PayloadVisitor<T>(PhantomData<fn() -> T>);

impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for PayloadVisitor<T> {
    type Value = Payload<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a payload")
    }

    /// The wire form: the value's encoding as a byte string, decoded only
    /// when the receiver uses it.
    fn visit_newtype_struct<D: Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer
            .deserialize_byte_buf(EncodedBytes)
            .map(Payload::Encoded)
    }

    /// A format that presents a newtype as a one-element sequence holds the
    /// value itself. Only the schema tracer (`shape`) reads `T`'s shape this
    /// way, so the path exists only with the tracer; bincode never takes it,
    /// and a payload a product decodes always stays encoded until used.
    #[cfg(feature = "testing")]
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let value = seq
            .next_element::<T>()?
            .ok_or_else(|| serde::de::Error::invalid_length(0, &self))?;
        Ok(Payload::shared(value))
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Payload<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_newtype_struct(PAYLOAD_NEWTYPE, PayloadVisitor(PhantomData))
    }
}

impl<'de> Deserialize<'de> for Payload<[u8]> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer
            .deserialize_byte_buf(EncodedBytes)
            .map(Self::Encoded)
    }
}
