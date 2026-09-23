use super::super::types::DumpHeapRef;
use super::*;

#[test]
fn charset_section_round_trips_registry_state() {
    let registry = DumpCharsetRegistry {
        charsets: vec![
            DumpCharsetInfo {
                id: 1,
                name_sym: Some(DumpSymId(2)),
                name: Some("charset-one".into()),
                dimension: 2,
                code_space: [0, 127, 128, 255, 0, 0, 0, 0],
                min_code: 0,
                max_code: 255,
                iso_final_char: Some(65),
                iso_revision: Some(1),
                emacs_mule_id: None,
                ascii_compatible_p: true,
                supplementary_p: false,
                unified_p: true,
                invalid_code: Some(-1),
                unify_map: DumpValue::Vector(DumpHeapRef { index: 3 }),
                method: DumpCharsetMethod::Subset(DumpCharsetSubsetSpec {
                    parent_sym: Some(DumpSymId(4)),
                    parent: Some("parent".into()),
                    parent_min_code: 10,
                    parent_max_code: 20,
                    offset: 30,
                }),
                plist_syms: vec![(DumpSymId(5), DumpValue::Int(6))],
                plist: vec![("prop".into(), DumpValue::True)],
            },
            DumpCharsetInfo {
                id: 7,
                name_sym: None,
                name: Some("charset-two".into()),
                dimension: 1,
                code_space: [0, 255, 0, 0, 0, 0, 0, 0],
                min_code: 0,
                max_code: 255,
                iso_final_char: None,
                iso_revision: None,
                emacs_mule_id: Some(9),
                ascii_compatible_p: false,
                supplementary_p: true,
                unified_p: false,
                invalid_code: None,
                unify_map: DumpValue::Nil,
                method: DumpCharsetMethod::SupersetSyms(vec![(DumpSymId(8), 9)]),
                plist_syms: Vec::new(),
                plist: Vec::new(),
            },
        ],
        priority_syms: vec![DumpSymId(10)],
        priority: vec!["charset-one".into()],
        next_id: 11,
    };

    let bytes = charset_section_bytes(&registry).expect("encode charset registry");
    let decoded = load_charset_section(&bytes).expect("decode charset registry");

    assert_eq!(format!("{decoded:?}"), format!("{registry:?}"));
}

#[test]
fn charset_section_rejects_bad_magic() {
    let mut bytes =
        charset_section_bytes(&empty_charset_registry()).expect("encode charset registry");
    bytes[0] ^= 1;
    let err = load_charset_section(&bytes).expect_err("bad magic should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}
