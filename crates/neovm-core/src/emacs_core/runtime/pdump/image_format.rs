//! Runtime-image container vocabulary shared by every image owner.
//!
//! The section kinds and the relocation-entry encoding are the image FORMAT:
//! they describe bytes that a native host maps from a file and that a browser
//! host will one day copy into linear memory. Neither depends on whether this
//! build can memory-map files, so both `mmap_image` (native, file-backed) and
//! `mmap_image_portable` (no mapping capability) re-export this module. A new
//! section kind or a changed relocation encoding is therefore made exactly
//! once, and the wasm32/Android cross-checks compile the same definition the
//! native writer uses.

use bytemuck::{Pod, Zeroable};
use num_enum::{IntoPrimitive, TryFromPrimitive};

use super::DumpError;

/// Section kinds in a runtime image, in their on-disk discriminant order.
///
/// Discriminant `9` is retired and deliberately absent.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoPrimitive, TryFromPrimitive)]
#[cfg_attr(test, derive(strum::EnumIter))]
pub(crate) enum DumpSectionKind {
    Metadata = 1,
    HeapImage = 2,
    Roots = 3,
    Relocations = 4,
    ObjectStarts = 5,
    EmacsRelocations = 6,
    RuntimeState = 7,
    SymbolTable = 8,
    Obarray = 10,
    Autoloads = 11,
    CharsetRegistry = 12,
    CodingSystems = 13,
    FaceTable = 14,
    Buffers = 15,
    RuntimeManagers = 16,
    ObjectExtra = 17,
    ValueRelocations = 18,
}

impl DumpSectionKind {
    /// Decode a section-table discriminant, rejecting retired and unknown
    /// values with a typed image-format error.
    pub(super) fn from_raw(raw: u32) -> Result<Self, DumpError> {
        Self::try_from(raw)
            .map_err(|_| DumpError::ImageFormatError(format!("unknown section kind {raw}")))
    }
}

/// One section handed to the image writer.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ImageSection<'a> {
    pub kind: DumpSectionKind,
    pub flags: u32,
    pub bytes: &'a [u8],
}

/// A heap word that holds a heap-relative pointer plus a tag in its low bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImageRelocation {
    pub location_offset: u64,
    pub addend: u8,
}

/// Low bits of a packed relocation entry that carry the tag addend.
pub(super) const RELOCATION_TAG_BITS: u64 = 4;
/// Mask selecting the tag addend of a packed relocation entry.
pub(super) const RELOCATION_TAG_MASK: u64 = (1 << RELOCATION_TAG_BITS) - 1;

/// On-disk relocation entry: `location_offset << RELOCATION_TAG_BITS | addend`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub(super) struct DumpImageRelocation {
    pub(super) packed: u64,
}

/// Byte length of one on-disk relocation entry.
pub(super) const RELOCATION_SIZE: usize = std::mem::size_of::<DumpImageRelocation>();

impl ImageRelocation {
    /// Pack for the relocation section. The asserts are dump-time invariants:
    /// the heap image is far smaller than `2^60` bytes and every tag fits the
    /// low bits, so a violation is a writer bug rather than an input error.
    pub(super) fn pack(self) -> DumpImageRelocation {
        assert!(u64::from(self.addend) <= RELOCATION_TAG_MASK);
        assert!(self.location_offset <= (u64::MAX >> RELOCATION_TAG_BITS));
        DumpImageRelocation {
            packed: (self.location_offset << RELOCATION_TAG_BITS) | u64::from(self.addend),
        }
    }

    /// Inverse of [`Self::pack`].
    pub(super) fn unpack(raw: DumpImageRelocation) -> Self {
        Self {
            location_offset: raw.packed >> RELOCATION_TAG_BITS,
            addend: (raw.packed & RELOCATION_TAG_MASK) as u8,
        }
    }
}

/// Encode the relocation section exactly as the loader decodes it.
pub(crate) fn relocation_section_bytes(relocations: &[ImageRelocation]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(relocations.len() * RELOCATION_SIZE);
    for relocation in relocations {
        bytes.extend_from_slice(bytemuck::bytes_of(&relocation.pack()));
    }
    bytes
}
