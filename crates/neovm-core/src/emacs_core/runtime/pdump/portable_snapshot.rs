//! Target-independent runtime images for hosts without native memory maps.
//!
//! Native pdump deliberately stores target-width heap objects and relocatable
//! pointers so it can map them directly. Those bytes are neither safe nor
//! useful as a browser asset. A portable snapshot instead serializes the
//! pointer-free [`DumpContextState`] mirror and reconstructs runtime objects
//! through the same conversion path used by in-memory evaluator cloning.

use std::io::{Cursor, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::types::DumpContextState;
use super::{Context, DumpError, mark_after_pdump_load_hook_pending, restore_snapshot};
use crate::emacs_core::eval::{SubrEntry, registered_global_subr_entries};
use crate::emacs_core::intern::resolve_name;
use crate::tagged::header::SubrDispatchKind;

const MAGIC: [u8; 16] = *b"NEOMACS-PRTDUMP!";
// Version 3 defines `DumpValue::Int` by Lisp integer value rather than by the
// producer's immediate representation. A narrower consumer may materialize a
// producer fixnum as a bignum without changing its Lisp value.
const SCHEMA_VERSION: u32 = 3;
const MAGIC_END: usize = MAGIC.len();
const VERSION_END: usize = MAGIC_END + size_of::<u32>();
const PAYLOAD_LEN_END: usize = VERSION_END + size_of::<u64>();
const CHECKSUM_END: usize = PAYLOAD_LEN_END + 32;

/// Pointer-free envelope payload. The native-subr contract is deliberately a
/// minimum requirement rather than a build fingerprint: a consumer may offer
/// additional primitives, but every primitive the producer could have placed
/// in the image must exist with the same Lisp call ABI.
#[derive(Serialize, Deserialize)]
struct PortableSnapshotPayload {
    required_subrs: Vec<PortableSubrAbi>,
    state: DumpContextState,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
struct PortableSubrAbi {
    name: String,
    min_args: u16,
    max_args: Option<u16>,
    dispatch: PortableSubrDispatch,
    interactive: bool,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
enum PortableSubrDispatch {
    Builtin,
    ContextCallable,
    SpecialForm,
}

impl From<SubrDispatchKind> for PortableSubrDispatch {
    fn from(value: SubrDispatchKind) -> Self {
        match value {
            SubrDispatchKind::Builtin => Self::Builtin,
            SubrDispatchKind::ContextCallable => Self::ContextCallable,
            SubrDispatchKind::SpecialForm => Self::SpecialForm,
        }
    }
}

impl PortableSubrAbi {
    fn from_entry(entry: SubrEntry) -> Self {
        Self {
            name: resolve_name(entry.name_id).to_owned(),
            min_args: entry.min_args,
            max_args: entry.max_args,
            dispatch: entry.dispatch_kind.into(),
            interactive: entry.interactive_spec.is_some(),
        }
    }

    fn describe(&self) -> String {
        let maximum = self
            .max_args
            .map_or_else(|| "many".to_owned(), |maximum| maximum.to_string());
        format!(
            "{} ({}..{}, {:?}, interactive={})",
            self.name, self.min_args, maximum, self.dispatch, self.interactive
        )
    }
}

fn compiled_subr_contract() -> Vec<PortableSubrAbi> {
    let mut entries = registered_global_subr_entries()
        .into_iter()
        .filter(|entry| entry.portability == crate::emacs_core::subr::SubrPortability::AllTargets)
        .map(PortableSubrAbi::from_entry)
        .collect::<Vec<_>>();
    entries.sort();
    entries.dedup();
    entries
}

fn validate_subr_contract(required: &[PortableSubrAbi]) -> Result<(), DumpError> {
    let available = compiled_subr_contract();
    for requirement in required {
        match available.binary_search(requirement) {
            Ok(_) => {}
            Err(_) => {
                if let Some(actual) = available
                    .iter()
                    .find(|entry| entry.name == requirement.name)
                {
                    return Err(DumpError::PortableRuntimeContractMismatch(format!(
                        "native subr `{}` has incompatible ABI: image requires {}, consumer provides {}",
                        requirement.name,
                        requirement.describe(),
                        actual.describe(),
                    )));
                }
                return Err(DumpError::PortableRuntimeContractMismatch(format!(
                    "consumer does not provide required native subr `{}` ({})",
                    requirement.name,
                    requirement.describe(),
                )));
            }
        }
    }
    Ok(())
}

/// Build-internal environment variable naming a companion portable image.
///
/// This is consumed only while the final `dump-emacs-portable` call runs; it
/// is not a runtime configuration switch.
pub const PORTABLE_RUNTIME_IMAGE_ENV: &str = "NEOVM_PORTABLE_RUNTIME_IMAGE";

/// Encode an evaluator as a target-independent runtime image.
///
/// This is the release-asset format for hosts that cannot map a native pdump,
/// notably browser WebAssembly. It is intentionally separate from the native
/// mmap format: neither format pays for the other's ownership model.
pub fn encode_portable_snapshot(eval: &Context) -> Result<Vec<u8>, DumpError> {
    let payload_value = PortableSnapshotPayload {
        required_subrs: compiled_subr_contract(),
        state: super::snapshot_evaluator(eval),
    };
    let mut payload = Vec::new();
    ciborium::ser::into_writer(&payload_value, &mut payload)
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

#[cfg(test)]
pub(crate) fn portable_required_subr_names(image: &[u8]) -> Vec<String> {
    let mut reader = Cursor::new(&image[CHECKSUM_END..]);
    let payload: PortableSnapshotPayload =
        ciborium::de::from_reader(&mut reader).expect("decode test portable snapshot");
    payload
        .required_subrs
        .into_iter()
        .map(|subr| subr.name)
        .collect()
}

/// Atomically publish a target-independent runtime image.
///
/// Release tooling calls this from the same dump-time evaluator state used by
/// the native final pdump. Encoding and flushing complete before the rename,
/// so an interrupted producer leaves the previous valid artifact in place.
pub fn dump_portable_snapshot_to_file(eval: &Context, path: &Path) -> Result<(), DumpError> {
    let image = encode_portable_snapshot(eval)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&image)?;
    temporary.as_file().sync_all()?;
    let published = temporary
        .persist(path)
        .map_err(|error| DumpError::Io(error.error))?;
    published.sync_all()?;
    #[cfg(unix)]
    std::fs::File::open(parent)?.sync_all()?;
    Ok(())
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
    let payload: PortableSnapshotPayload = ciborium::de::from_reader(&mut reader)
        .map_err(|err| DumpError::DeserializationError(err.to_string()))?;
    if reader.position() != actual_len {
        return Err(DumpError::DeserializationError(format!(
            "portable snapshot payload has {} trailing bytes",
            actual_len - reader.position()
        )));
    }

    let mut eval = restore_snapshot(&payload.state)?;
    eval.rebind_compiled_target_identity();
    validate_subr_contract(&payload.required_subrs)?;
    mark_after_pdump_load_hook_pending(&mut eval);
    Ok(eval)
}
