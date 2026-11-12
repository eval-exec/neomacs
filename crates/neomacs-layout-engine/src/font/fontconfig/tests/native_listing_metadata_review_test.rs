use crate::font::fontconfig::test_config::TestFontconfig;
use crate::font::probe::otf_cache_tests::Fixture;
use crate::font_backend::{FontFamilyName, NativeFontOtfFeature, NativeFontOtfRequirement};
use std::path::PathBuf;

fn config(files: &[PathBuf]) -> TestFontconfig {
    let generic = FontFamilyName::new("Hack").unwrap();
    TestFontconfig::new(files, &generic, &generic, &generic).unwrap()
}

#[test]
fn native_otf_listing_decodes_woff_before_checking_both_layout_tables() {
    let sfnt = std::fs::read(neomacs_test_fonts::dejavu_2_37().sans_regular()).unwrap();
    let fixture = Fixture::new(&neomacs_test_fonts::uncompressed_woff(&sfnt));
    let config = config(&[PathBuf::from(fixture.path())]);
    let unfiltered = config.list_otf_candidates(Some("DejaVu Sans"), None);
    assert!(
        !unfiltered.is_empty(),
        "native Fontconfig must admit the WOFF fixture"
    );
    let request = NativeFontOtfRequirement {
        script: *b"latn",
        language: None,
        gsub: vec![NativeFontOtfFeature::Required(*b"ccmp")],
        gpos: vec![NativeFontOtfFeature::Required(*b"kern")],
    };
    assert_eq!(
        config.list_otf_candidates(Some("DejaVu Sans"), Some(&request)),
        unfiltered,
        "pinned GNU/FreeType admits this WOFF with both requested features"
    );
    let absent = NativeFontOtfRequirement {
        script: *b"zzzz",
        ..request
    };
    assert!(
        config
            .list_otf_candidates(Some("DejaVu Sans"), Some(&absent))
            .is_empty()
    );
}

#[test]
fn native_bitmap_listing_preserves_integer_average_width() {
    let fixtures = neomacs_test_fonts::spleen_2_2_0();
    for path in [fixtures.bdf(), fixtures.pcf(), fixtures.pcf_gz()] {
        let config = config(&[path]);
        let candidates = config.list_candidates(Some("Spleen"), &[], None, &[]);
        assert!(!candidates.is_empty());
        // The exact pinned BDF/PCF property is INTEGER 80, independently of
        // the strike size or its measured glyph advance.
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.average_width == Some(80))
        );
    }
}

#[test]
fn native_bitmap_listing_distinguishes_zero_missing_and_atom_widths() {
    let source = std::fs::read_to_string(neomacs_test_fonts::spleen_2_2_0().bdf()).unwrap();
    for (replacement, expected) in [
        ("AVERAGE_WIDTH 0", Some(0)),
        // FreeType's BDF schema fixes AVERAGE_WIDTH to INTEGER. A quoted
        // literal is parsed as integer zero, as pinned GNU also publishes.
        ("AVERAGE_WIDTH \"80\"", Some(0)),
        ("CUSTOM_WIDTH 80", None),
    ] {
        let fixture = Fixture::new(source.replace("AVERAGE_WIDTH 80", replacement).as_bytes());
        let candidates = config(&[PathBuf::from(fixture.path())]).list_candidates(
            Some("Spleen"),
            &[],
            None,
            &[],
        );
        assert!(!candidates.is_empty());
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.average_width == expected),
            "{replacement}"
        );
    }

    // PCF has an explicit isString bit and string-table offsets, so this is a
    // real ATOM property rather than inferring its type from BDF syntax.
    let mut pcf = std::fs::read(neomacs_test_fonts::spleen_2_2_0().pcf()).unwrap();
    assert_eq!(&pcf[..4], b"\x01fcp");
    let tables = u32::from_le_bytes(pcf[4..8].try_into().unwrap()) as usize;
    let properties = (0..tables)
        .map(|index| &pcf[8 + 16 * index..24 + 16 * index])
        .find(|record| u32::from_le_bytes(record[..4].try_into().unwrap()) == 1)
        .unwrap();
    let format = u32::from_le_bytes(properties[4..8].try_into().unwrap());
    let offset = u32::from_le_bytes(properties[12..16].try_into().unwrap()) as usize;
    let big_endian = format & 4 != 0;
    let read = |bytes: &[u8]| {
        if big_endian {
            u32::from_be_bytes(bytes.try_into().unwrap())
        } else {
            u32::from_le_bytes(bytes.try_into().unwrap())
        }
    };
    let count = read(&pcf[offset + 4..offset + 8]) as usize;
    let strings = offset + 8 + ((count * 9 + 3) & !3) + 4;
    let mut average = None;
    let mut atom_offset = None;
    for index in 0..count {
        let at = offset + 8 + index * 9;
        let name_offset = read(&pcf[at..at + 4]) as usize;
        let name = pcf[strings + name_offset..]
            .split(|byte| *byte == 0)
            .next()
            .unwrap();
        if name == b"AVERAGE_WIDTH" {
            assert_eq!(pcf[at + 4], 0);
            average = Some(at);
        } else if name == b"FAMILY_NAME" {
            assert_eq!(pcf[at + 4], 1);
            atom_offset = Some(read(&pcf[at + 5..at + 9]));
        }
    }
    let at = average.unwrap();
    let value = atom_offset.unwrap();
    pcf[at + 4] = 1;
    pcf[at + 5..at + 9].copy_from_slice(&if big_endian {
        value.to_be_bytes()
    } else {
        value.to_le_bytes()
    });
    let fixture = Fixture::new(&pcf);
    let candidates =
        config(&[PathBuf::from(fixture.path())]).list_candidates(Some("Spleen"), &[], None, &[]);
    assert!(!candidates.is_empty());
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.average_width.is_none()),
        "PCF ATOM is not an INTEGER average width"
    );
}

#[test]
fn native_scalability_publishes_zero_width_for_outline_and_color_bitmap() {
    let config = config(&[
        neomacs_test_fonts::hack_regular_3_003().to_path_buf(),
        neomacs_test_fonts::noto_color_emoji_2_051().to_path_buf(),
    ]);
    for family in ["Hack", "Noto Color Emoji"] {
        let candidates = config.list_candidates(Some(family), &[], None, &[]);
        assert!(!candidates.is_empty());
        assert!(
            candidates
                .iter()
                .all(|candidate| candidate.average_width == Some(0))
        );
    }
}
