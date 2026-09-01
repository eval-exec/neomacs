//! Raw mapped-value fixups for HeapImage words.
//!
//! GNU pdumper writes heap-shaped objects and records relocation work for Lisp
//! value fields that cannot be represented as a plain intra-dump pointer.  This
//! section is the Neomacs equivalent for mapped HeapImage words: each entry
//! names a HeapImage word offset and the logical DumpValue that should be
//! written there after the dump-local symbol table has been restored.

use bytemuck::{Pod, Zeroable};

use super::DumpError;
use super::object_value_codec;
use super::types::DumpValue;

const VALUE_FIXUPS_MAGIC: [u8; 16] = *b"NEOVALUEFIXUPS\0\0";
const VALUE_FIXUPS_FORMAT_VERSION: u32 = 3;
const FIXUP_KIND_BITS: u64 = 2;
const FIXUP_KIND_MASK: u64 = (1 << FIXUP_KIND_BITS) - 1;
const FIXUP_OFFSET_ALIGN_BITS: u64 = 3;
const FIXUP_VALUE: u64 = 1;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
struct ValueFixupsHeader {
    magic: [u8; 16],
    version: u32,
    header_size: u32,
    /// Number of Value-class entries in the varint payload (after the
    /// symbol-offset array).
    fixup_count: u64,
    /// Number of Symbol-class fixups: a flat little-endian u32 offset array
    /// at the start of the payload. A symbol fixup carries no other data --
    /// the dump-local SymId is already baked into the word at that offset --
    /// so the array form lets the loader apply them in a tight loop with the
    /// remap borrowed once (~86% of all fixups are this class).
    symbol_count: u64,
    payload_offset: u64,
    payload_len: u64,
}

const HEADER_SIZE: usize = std::mem::size_of::<ValueFixupsHeader>();

#[derive(Clone, Debug)]
pub(crate) enum RawValueFixup {
    Symbol {
        location_offset: u64,
    },
    Value {
        location_offset: u64,
        value: DumpValue,
    },
}

impl RawValueFixup {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn location_offset(&self) -> u64 {
        match self {
            Self::Symbol { location_offset }
            | Self::Value {
                location_offset, ..
            } => *location_offset,
        }
    }
}

pub(crate) fn value_fixups_section_bytes(fixups: &[RawValueFixup]) -> Result<Vec<u8>, DumpError> {
    let mut bytes = vec![0u8; HEADER_SIZE];
    let mut symbol_count: u64 = 0;
    for fixup in fixups {
        if let RawValueFixup::Symbol { location_offset } = fixup {
            let alignment_mask = (1u64 << FIXUP_OFFSET_ALIGN_BITS) - 1;
            if location_offset & alignment_mask != 0 {
                return Err(DumpError::SerializationError(format!(
                    "symbol fixup location offset {location_offset} is not word-aligned"
                )));
            }
            let offset = u32::try_from(*location_offset).map_err(|_| {
                DumpError::SerializationError(format!(
                    "symbol fixup location offset {location_offset} overflows u32"
                ))
            })?;
            bytes.extend_from_slice(&offset.to_le_bytes());
            symbol_count += 1;
        }
    }
    let mut value_count: u64 = 0;
    for fixup in fixups {
        if let RawValueFixup::Value {
            location_offset,
            value,
        } = fixup
        {
            object_value_codec::write_u64(
                &mut bytes,
                pack_fixup_location(*location_offset, FIXUP_VALUE)?,
            );
            object_value_codec::write_value(&mut bytes, value)?;
            value_count += 1;
        }
    }

    let payload_len = bytes.len() - HEADER_SIZE;
    let header = ValueFixupsHeader {
        magic: VALUE_FIXUPS_MAGIC,
        version: VALUE_FIXUPS_FORMAT_VERSION,
        header_size: HEADER_SIZE as u32,
        fixup_count: value_count,
        symbol_count,
        payload_offset: HEADER_SIZE as u64,
        payload_len: payload_len as u64,
    };
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    Ok(bytes)
}

/// The two payload regions of a v3 value-fixups section.
pub(crate) struct SectionParts<'a> {
    /// Flat little-endian u32 heap-word offsets, one per Symbol-class fixup.
    pub(crate) symbol_offsets: &'a [u8],
    value_count: usize,
    value_payload: &'a [u8],
}

pub(crate) fn section_parts(section: &[u8]) -> Result<SectionParts<'_>, DumpError> {
    let (value_count, symbol_count, payload) = value_fixups_payload(section)?;
    let symbol_bytes = symbol_count.checked_mul(4).ok_or_else(|| {
        DumpError::ImageFormatError("value-fixups symbol array length overflows usize".into())
    })?;
    if symbol_bytes > payload.len() {
        return Err(DumpError::ImageFormatError(format!(
            "value-fixups symbol array of {symbol_bytes} bytes exceeds payload of {}",
            payload.len()
        )));
    }
    let (symbol_offsets, value_payload) = payload.split_at(symbol_bytes);
    Ok(SectionParts {
        symbol_offsets,
        value_count,
        value_payload,
    })
}

/// Iterate the Value-class entries of a v3 section (Symbol-class fixups are
/// the flat offset array in [`SectionParts::symbol_offsets`]).
pub(crate) fn for_each_value_entry(
    parts: &SectionParts<'_>,
    mut f: impl FnMut(u64, DumpValue) -> Result<(), DumpError>,
) -> Result<(), DumpError> {
    let mut cursor = object_value_codec::Cursor::new(parts.value_payload);
    for _ in 0..parts.value_count {
        let packed = cursor.read_u64("value-fixup location")?;
        let location_offset = unpack_fixup_location(packed)?;
        if packed & FIXUP_KIND_MASK != FIXUP_VALUE {
            return Err(DumpError::ImageFormatError(format!(
                "unexpected value-fixup kind {} in value payload",
                packed & FIXUP_KIND_MASK
            )));
        }
        f(location_offset, cursor.read_value()?)?;
    }
    ensure_fixup_cursor_empty(&cursor)
}

fn value_fixups_payload(section: &[u8]) -> Result<(usize, usize, &[u8]), DumpError> {
    if section.len() < HEADER_SIZE {
        return Err(DumpError::ImageFormatError(
            "value-fixups section too small for header".into(),
        ));
    }

    let header = *bytemuck::from_bytes::<ValueFixupsHeader>(&section[..HEADER_SIZE]);
    if header.magic != VALUE_FIXUPS_MAGIC {
        return Err(DumpError::ImageFormatError(
            "value-fixups section has bad magic".into(),
        ));
    }
    if header.version != VALUE_FIXUPS_FORMAT_VERSION {
        return Err(DumpError::UnsupportedVersion(header.version));
    }
    if header.header_size != HEADER_SIZE as u32 {
        return Err(DumpError::ImageFormatError(format!(
            "value-fixups header size {} does not match runtime header size {HEADER_SIZE}",
            header.header_size
        )));
    }

    let payload_start = usize::try_from(header.payload_offset).map_err(|_| {
        DumpError::ImageFormatError("value-fixups payload offset overflows usize".into())
    })?;
    let payload_len = usize::try_from(header.payload_len).map_err(|_| {
        DumpError::ImageFormatError("value-fixups payload length overflows usize".into())
    })?;
    let payload_end = payload_start.checked_add(payload_len).ok_or_else(|| {
        DumpError::ImageFormatError("value-fixups payload range overflows".into())
    })?;
    if payload_start < HEADER_SIZE || payload_end > section.len() {
        return Err(DumpError::ImageFormatError(
            "value-fixups payload range is outside section".into(),
        ));
    }

    let fixup_count = usize::try_from(header.fixup_count)
        .map_err(|_| DumpError::ImageFormatError("value-fixups count overflows usize".into()))?;
    let symbol_count = usize::try_from(header.symbol_count).map_err(|_| {
        DumpError::ImageFormatError("value-fixups symbol count overflows usize".into())
    })?;
    Ok((
        fixup_count,
        symbol_count,
        &section[payload_start..payload_end],
    ))
}

fn pack_fixup_location(location_offset: u64, kind: u64) -> Result<u64, DumpError> {
    if kind > FIXUP_KIND_MASK {
        return Err(DumpError::SerializationError(format!(
            "value-fixup kind {kind} exceeds kind mask"
        )));
    }
    let alignment_mask = (1 << FIXUP_OFFSET_ALIGN_BITS) - 1;
    if location_offset & alignment_mask != 0 {
        return Err(DumpError::SerializationError(format!(
            "value-fixup location offset {location_offset} is not word-aligned"
        )));
    }
    let raw_offset = location_offset >> FIXUP_OFFSET_ALIGN_BITS;
    if raw_offset > (u64::MAX >> FIXUP_KIND_BITS) {
        return Err(DumpError::SerializationError(
            "value-fixup location offset is out of range".into(),
        ));
    }
    Ok((raw_offset << FIXUP_KIND_BITS) | kind)
}

fn unpack_fixup_location(packed: u64) -> Result<u64, DumpError> {
    let raw_offset = packed >> FIXUP_KIND_BITS;
    raw_offset
        .checked_shl(FIXUP_OFFSET_ALIGN_BITS as u32)
        .ok_or_else(|| DumpError::ImageFormatError("value-fixup location overflow".into()))
}

fn ensure_fixup_cursor_empty(cursor: &object_value_codec::Cursor<'_>) -> Result<(), DumpError> {
    if !cursor.is_empty() {
        return Err(DumpError::ImageFormatError(format!(
            "value-fixups section has {} trailing payload bytes",
            cursor.remaining()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emacs_core::pdump::types::{DumpHeapRef, DumpNameId, DumpSymId};

    #[test]
    fn value_fixups_round_trip_representative_values() {
        let fixups = vec![
            RawValueFixup::Value {
                location_offset: 8,
                value: DumpValue::Symbol(DumpSymId(3)),
            },
            RawValueFixup::Value {
                location_offset: 16,
                value: DumpValue::Subr(DumpNameId(4)),
            },
            RawValueFixup::Value {
                location_offset: 24,
                value: DumpValue::HashTable(DumpHeapRef { index: 5 }),
            },
        ];

        let bytes = value_fixups_section_bytes(&fixups).expect("encode value fixups");
        let parts = section_parts(&bytes).expect("decode value fixups");
        assert!(parts.symbol_offsets.is_empty());

        let mut decoded = Vec::new();
        for_each_value_entry(&parts, |offset, value| {
            decoded.push((offset, value));
            Ok(())
        })
        .expect("iterate value entries");

        assert_eq!(decoded.len(), fixups.len());
        assert!(matches!(decoded[0], (8, DumpValue::Symbol(DumpSymId(3)))));
        assert!(matches!(decoded[1], (16, DumpValue::Subr(DumpNameId(4)))));
        assert!(matches!(
            decoded[2],
            (24, DumpValue::HashTable(DumpHeapRef { index: 5 }))
        ));
    }

    #[test]
    fn symbol_value_fixups_encode_as_flat_u32_offset_array() {
        let fixups = vec![
            RawValueFixup::Symbol {
                location_offset: 16,
            },
            RawValueFixup::Value {
                location_offset: 8,
                value: DumpValue::Symbol(DumpSymId(3)),
            },
            RawValueFixup::Symbol {
                location_offset: 4096,
            },
        ];

        let bytes = value_fixups_section_bytes(&fixups).expect("encode mixed fixups");
        let parts = section_parts(&bytes).expect("decode mixed fixups");

        // Two symbol entries at 4 bytes each, regardless of interleaving.
        assert_eq!(parts.symbol_offsets.len(), 8);
        let offsets: Vec<u32> = parts
            .symbol_offsets
            .chunks_exact(4)
            .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
            .collect();
        assert_eq!(offsets, vec![16, 4096]);

        let mut value_entries = Vec::new();
        for_each_value_entry(&parts, |offset, value| {
            value_entries.push((offset, value));
            Ok(())
        })
        .expect("iterate value entries");
        assert!(matches!(
            value_entries.as_slice(),
            [(8, DumpValue::Symbol(DumpSymId(3)))]
        ));
    }

    #[test]
    fn unaligned_symbol_fixup_offset_is_rejected_at_encode() {
        let fixups = vec![RawValueFixup::Symbol {
            location_offset: 12,
        }];
        assert!(value_fixups_section_bytes(&fixups).is_err());
    }
}
