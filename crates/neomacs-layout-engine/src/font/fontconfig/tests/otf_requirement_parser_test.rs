use super::*;
use crate::font_backend::NativeFontOtfFeature::{Forbidden, Required};

// Reuse the existing constructed SFNT parser fixture, not a claim that these
// bytes were installed in Fontconfig or compared with GNU as a native TTC.
fn font(base: usize, feature: &[u8; 4]) -> Vec<u8> {
    let mut bytes = crate::font::probe::otf_cache_tests::sfnt(base, feature);
    let offset = bytes.windows(4).position(|tag| tag == b"DFLT").unwrap();
    bytes[offset..offset + 4].copy_from_slice(b"latn");
    bytes
}

fn request() -> NativeFontOtfRequirement {
    NativeFontOtfRequirement {
        script: *b"latn",
        language: None,
        gsub: vec![],
        gpos: vec![],
    }
}

#[test]
fn otf_listing_parser_uses_file_face_zero_for_collection_table_gate() {
    let first = font(20, b"liga");
    let second_offset = 20 + first.len();
    let second = font(second_offset, b"calt");
    let mut bytes = b"ttcf\0\x01\0\0\0\0\0\x02".to_vec();
    bytes.extend(20u32.to_be_bytes());
    bytes.extend((second_offset as u32).to_be_bytes());
    bytes.extend(first);
    bytes.extend(second);
    let selected = ttf_parser::Face::parse(&bytes, 1).unwrap();
    assert!(
        selected
            .tables()
            .gsub
            .unwrap()
            .features
            .find(ttf_parser::Tag::from_bytes(b"calt"))
            .is_some()
    );
    let mut requirement = request();
    requirement.gsub = vec![Required(*b"liga")];
    requirement.gpos = vec![Required(*b"kern")];
    assert!(
        matches_bytes(&bytes, &requirement),
        "GNU hbotf_open explicitly opens file face0"
    );
    requirement.gsub = vec![Required(*b"calt")];
    assert!(!matches_bytes(&bytes, &requirement));
}

#[test]
fn otf_listing_parser_preserves_missing_default_language_and_zero_tags() {
    let mut bytes = font(0, b"liga");
    let positions: Vec<_> = bytes
        .windows(4)
        .enumerate()
        .filter_map(|(index, tag)| (tag == b"latn").then_some(index))
        .collect();
    assert_eq!(positions.len(), 2);
    for position in positions {
        // This constructed ScriptRecord has its ScriptTable six bytes later.
        bytes[position + 6..position + 8].copy_from_slice(&[0, 0]);
    }
    let parsed = ttf_parser::Face::parse(&bytes, 0).unwrap();
    for table in [parsed.tables().gsub.unwrap(), parsed.tables().gpos.unwrap()] {
        assert!(
            table
                .scripts
                .find(ttf_parser::Tag::from_bytes(b"latn"))
                .unwrap()
                .default_language
                .is_none()
        );
    }
    let mut requirement = request();
    assert!(
        matches_bytes(&bytes, &requirement),
        "empty feature constraint does not need a defaultLangSys"
    );
    requirement.gsub = vec![Forbidden(*b"liga"), Required([0; 4])];
    assert!(matches_bytes(&bytes, &requirement));
    requirement.gsub = vec![Required(*b"liga")];
    assert!(!matches_bytes(&bytes, &requirement));
}

#[test]
fn otf_listing_capability_gate_keeps_gnu_substring_and_zero_termination() {
    assert!(capability_matches(b"prefixotlayout:latn-suffix", *b"latn"));
    assert!(!capability_matches(b"otlayout:latn", *b"zzzz"));
    assert!(capability_matches(b"otlayout:latn", [b'l', 0, b'x', b'x']));
    assert!(capability_matches(b"otlayout:la  ", *b"la  "));
    assert!(capability_matches(
        b"otlayout:\xffabc",
        [0xff, b'a', b'b', b'c']
    ));
    assert!(!matches_bytes(b"not a font", &request()));
}

#[test]
fn otf_listing_native_capability_keeps_raw_bytes_and_trailing_tag_spaces() {
    // SAFETY: the newly created native pattern is immediately owned by its guard.
    let pattern = super::super::FcPatternGuard(unsafe { fontconfig_sys::FcPatternCreate() });
    assert!(!pattern.0.is_null());
    let value = std::ffi::CString::new(b"otlayout:\xffabc otlayout:la  ".to_vec()).unwrap();
    // SAFETY: both pattern and C string are live; Fontconfig copies the value.
    assert_ne!(
        unsafe {
            fontconfig_sys::FcPatternAddString(
                pattern.0,
                fontconfig_sys::constants::FC_CAPABILITY.as_ptr(),
                value.as_ptr().cast(),
            )
        },
        0
    );
    assert!(super::super::raw_pattern_matches_otf_script(
        pattern.0,
        [0xff, b'a', b'b', b'c']
    ));
    assert!(super::super::raw_pattern_matches_otf_script(
        pattern.0, *b"la  "
    ));
    assert!(!super::super::raw_pattern_matches_otf_script(
        pattern.0, *b"zzzz"
    ));
}
