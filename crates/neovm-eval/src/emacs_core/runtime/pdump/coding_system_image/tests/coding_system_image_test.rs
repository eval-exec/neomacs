use super::super::types::DumpHeapRef;
use super::*;

#[test]
fn coding_system_section_round_trips_manager_state() {
    let manager = DumpCodingSystemManager {
        systems_syms: vec![(
            DumpSymId(1),
            DumpCodingSystemInfo {
                name_sym: Some(DumpSymId(2)),
                name: None,
                coding_type_sym: Some(DumpSymId(3)),
                coding_type: None,
                mnemonic: 'U',
                eol_type: DumpEolType::Unix,
                ascii_compatible_p: true,
                charset_list_syms: vec![DumpSymId(4), DumpSymId(5)],
                charset_list: Vec::new(),
                post_read_conversion_sym: Some(DumpSymId(6)),
                post_read_conversion: None,
                pre_write_conversion_sym: None,
                pre_write_conversion: Some("legacy-pre-write".into()),
                default_char: Some('?'),
                for_unibyte: false,
                properties_syms: vec![(DumpSymId(7), DumpValue::Int(8))],
                properties: vec![("legacy-prop".into(), DumpValue::True)],
                int_properties: vec![(9, DumpValue::Vector(DumpHeapRef { index: 10 }))],
            },
        )],
        systems: vec![(
            "legacy-system".into(),
            DumpCodingSystemInfo {
                name_sym: None,
                name: Some("legacy-system".into()),
                coding_type_sym: None,
                coding_type: Some("utf-8".into()),
                mnemonic: 'L',
                eol_type: DumpEolType::Dos,
                ascii_compatible_p: false,
                charset_list_syms: Vec::new(),
                charset_list: vec!["charset".into()],
                post_read_conversion_sym: None,
                post_read_conversion: None,
                pre_write_conversion_sym: None,
                pre_write_conversion: None,
                default_char: None,
                for_unibyte: true,
                properties_syms: Vec::new(),
                properties: Vec::new(),
                int_properties: Vec::new(),
            },
        )],
        aliases_syms: vec![(DumpSymId(11), DumpSymId(12))],
        aliases: vec![("legacy-alias".into(), "legacy-base".into())],
        alias_order_syms: vec![(DumpSymId(16), vec![DumpSymId(16), DumpSymId(17)])],
        alias_order: vec![(
            "legacy-base".into(),
            vec!["legacy-base".into(), "legacy-alias".into()],
        )],
        priority_syms: vec![DumpSymId(13)],
        priority: vec!["legacy-priority".into()],
        keyboard_coding_sym: Some(DumpSymId(14)),
        keyboard_coding: Some("legacy-keyboard".into()),
        terminal_coding_sym: Some(DumpSymId(15)),
        terminal_coding: Some("legacy-terminal".into()),
    };

    let bytes = coding_system_section_bytes(&manager).expect("encode coding systems");
    let decoded = load_coding_system_section(&bytes).expect("decode coding systems");

    assert_eq!(format!("{decoded:?}"), format!("{manager:?}"));
}

#[test]
fn coding_system_section_rejects_bad_magic() {
    let mut bytes =
        coding_system_section_bytes(&empty_coding_system_manager()).expect("encode coding");
    bytes[0] ^= 1;
    let err = load_coding_system_section(&bytes).expect_err("bad magic should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}
