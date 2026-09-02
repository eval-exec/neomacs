use strum::IntoEnumIterator;

use super::super::DumpError;
use super::super::image_format::*;

#[test]
fn every_section_kind_round_trips_through_its_discriminant() {
    for kind in DumpSectionKind::iter() {
        assert_eq!(DumpSectionKind::from_raw(u32::from(kind)).unwrap(), kind);
    }
    assert!(matches!(
        DumpSectionKind::from_raw(9),
        Err(DumpError::ImageFormatError(_))
    ));
    assert!(matches!(
        DumpSectionKind::from_raw(0),
        Err(DumpError::ImageFormatError(_))
    ));
}

#[test]
fn relocation_entries_pack_to_one_word_and_round_trip() {
    let relocations = [
        ImageRelocation {
            location_offset: 0,
            addend: 0,
        },
        ImageRelocation {
            location_offset: 8,
            addend: 3,
        },
        ImageRelocation {
            location_offset: u64::MAX >> RELOCATION_TAG_BITS,
            addend: RELOCATION_TAG_MASK as u8,
        },
    ];
    let bytes = relocation_section_bytes(&relocations);
    assert_eq!(bytes.len(), relocations.len() * RELOCATION_SIZE);
    assert_eq!(RELOCATION_SIZE, 8);
    for (index, expected) in relocations.iter().enumerate() {
        let raw = *bytemuck::from_bytes::<DumpImageRelocation>(
            &bytes[index * RELOCATION_SIZE..(index + 1) * RELOCATION_SIZE],
        );
        assert_eq!(ImageRelocation::unpack(raw), *expected);
    }
}
