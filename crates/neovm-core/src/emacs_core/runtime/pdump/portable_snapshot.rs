//! Target-independent runtime images for hosts without native memory maps.
//!
//! Native pdump deliberately stores target-width heap objects and relocatable
//! pointers so it can map them directly. Those bytes are neither safe nor
//! useful as a browser asset. A portable snapshot instead serializes the
//! pointer-free [`DumpContextState`] mirror and reconstructs runtime objects
//! through the same conversion path used by in-memory evaluator cloning.

use std::io::Cursor;

use sha2::{Digest, Sha256};

use super::types::DumpContextState;
use super::{Context, DumpError, mark_after_pdump_load_hook_pending, restore_snapshot};

const MAGIC: [u8; 16] = *b"NEOMACS-PRTDUMP!";
const SCHEMA_VERSION: u32 = 1;
const MAGIC_END: usize = MAGIC.len();
const VERSION_END: usize = MAGIC_END + size_of::<u32>();
const PAYLOAD_LEN_END: usize = VERSION_END + size_of::<u64>();
const CHECKSUM_END: usize = PAYLOAD_LEN_END + 32;

/// Encode an evaluator as a target-independent runtime image.
///
/// This is the release-asset format for hosts that cannot map a native pdump,
/// notably browser WebAssembly. It is intentionally separate from the native
/// mmap format: neither format pays for the other's ownership model.
pub fn encode_portable_snapshot(eval: &Context) -> Result<Vec<u8>, DumpError> {
    let state = super::snapshot_evaluator(eval);
    let mut payload = Vec::new();
    ciborium::ser::into_writer(&state, &mut payload)
        .map_err(|err| DumpError::SerializationError(err.to_string()))?;

    let payload_len = u64::try_from(payload.len()).map_err(|_| {
        DumpError::SerializationError("portable snapshot payload exceeds u64".into())
    })?;
    let checksum = Sha256::digest(&payload);
    let mut image = Vec::with_capacity(CHECKSUM_END + payload.len());
    image.extend_from_slice(&MAGIC);
    image.extend_from_slice(&SCHEMA_VERSION.to_le_bytes());
    image.extend_from_slice(&payload_len.to_le_bytes());
    image.extend_from_slice(&checksum);
    image.extend_from_slice(&payload);
    Ok(image)
}

/// Restore an evaluator from a target-independent runtime image.
///
/// The complete envelope is validated before the deserializer sees payload
/// bytes. Restoration marks the normal `after-pdump-load-hook` as pending, so
/// host startup can share the same post-image initialization path as native.
pub fn load_from_portable_snapshot(image: &[u8]) -> Result<Context, DumpError> {
    if image.len() < CHECKSUM_END {
        return Err(DumpError::ImageFormatError(
            "portable snapshot header is truncated".into(),
        ));
    }
    if image[..MAGIC_END] != MAGIC {
        return Err(DumpError::BadMagic);
    }

    let version = u32::from_le_bytes(
        image[MAGIC_END..VERSION_END]
            .try_into()
            .expect("fixed version range"),
    );
    if version != SCHEMA_VERSION {
        return Err(DumpError::UnsupportedVersion(version));
    }

    let declared_len = u64::from_le_bytes(
        image[VERSION_END..PAYLOAD_LEN_END]
            .try_into()
            .expect("fixed payload-length range"),
    );
    let payload = &image[CHECKSUM_END..];
    let actual_len = u64::try_from(payload.len())
        .map_err(|_| DumpError::ImageFormatError("portable snapshot payload exceeds u64".into()))?;
    if declared_len != actual_len {
        return Err(DumpError::ImageFormatError(format!(
            "portable snapshot declares {declared_len} payload bytes but contains {actual_len}"
        )));
    }

    let expected_checksum = &image[PAYLOAD_LEN_END..CHECKSUM_END];
    if Sha256::digest(payload).as_slice() != expected_checksum {
        return Err(DumpError::ChecksumMismatch);
    }

    let mut reader = Cursor::new(payload);
    let state: DumpContextState = ciborium::de::from_reader(&mut reader)
        .map_err(|err| DumpError::DeserializationError(err.to_string()))?;
    if reader.position() != actual_len {
        return Err(DumpError::DeserializationError(format!(
            "portable snapshot payload has {} trailing bytes",
            actual_len - reader.position()
        )));
    }

    let mut eval = restore_snapshot(&state)?;
    mark_after_pdump_load_hook_pending(&mut eval);
    Ok(eval)
}
