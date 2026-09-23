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
