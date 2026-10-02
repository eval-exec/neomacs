use super::*;

fn overlapping_registry() -> DumpCharsetRegistry {
    let info = |id, name: &str, code, mule_id, supplementary_p| DumpCharsetInfo {
        id,
        name_sym: None,
        name: Some(name.into()),
        dimension: 1,
        code_space: [code, 126, 0, 0, 0, 0, 0, 0],
        min_code: code,
        max_code: 126,
        iso_final_char: None,
        iso_revision: None,
        emacs_mule_id: Some(mule_id),
        ascii_compatible_p: false,
        supplementary_p,
        unified_p: false,
        invalid_code: None,
        unify_map: DumpValue::Nil,
        method: DumpCharsetMethod::Offset(0x1f300),
        plist_syms: Vec::new(),
        plist: Vec::new(),
    };
    DumpCharsetRegistry {
        charsets: vec![
            info(920, "mule-section-older", 33, 150, true),
            info(921, "mule-section-newer", 34, 151, false),
        ],
        priority_syms: Vec::new(),
        priority: vec!["mule-section-newer".into(), "mule-section-older".into()],
        emacs_mule_order_syms: Some(vec![DumpSymId(42), DumpSymId(43)]),
        next_id: 922,
    }
}

#[test]
fn charset_section_mule_order_round_trip_keeps_definition_chronology() {
    let registry = overlapping_registry();
    let bytes = charset_section_bytes(&registry).expect("encode overlapping registry");
    assert_eq!(read_header(&bytes).expect("header").version, 2);
    let decoded = load_charset_section(&bytes).expect("decode overlapping registry");
    assert_eq!(
        decoded.emacs_mule_order_syms,
        registry.emacs_mule_order_syms
    );
    assert_eq!(decoded.priority, registry.priority);
    assert_eq!(format!("{decoded:?}"), format!("{registry:?}"));
}

#[test]
fn charset_section_accepts_legacy_and_derives_mule_priority() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new();
    let mut registry = overlapping_registry();
    registry.emacs_mule_order_syms = None;
    let mut bytes = charset_section_bytes(&registry).expect("encode registry");
    // Recreate the exact version-1 layout: identical header/records, without
    // version 2's final optional-order count. No GNU oracle bytes are involved.
    assert_eq!(
        &bytes[bytes.len() - 8..],
        &ABSENT_EMACS_MULE_ORDER.to_ne_bytes()
    );
    bytes.truncate(bytes.len() - 8);
    let mut header = read_header(&bytes).expect("version-2 header");
    header.version = 1;
    header.payload_len -= 8;
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    let decoded = load_charset_section(&bytes).expect("read legacy charset section");
    assert!(decoded.emacs_mule_order_syms.is_none());
    assert_eq!(format!("{decoded:?}"), format!("{registry:?}"));

    let heap = super::super::types::DumpTaggedHeap {
        objects: Vec::new(),
        mapped_cons: Vec::new(),
        mapped_floats: Vec::new(),
        mapped_strings: Vec::new(),
        mapped_veclikes: Vec::new(),
        mapped_slots: Vec::new(),
    };
    let mut decoder = super::super::convert::LoadDecoder::new(&heap);
    super::super::convert::load_charset_registry(&mut decoder, &decoded);
    let snapshot = crate::emacs_core::charset::snapshot_charset_registry();
    assert_eq!(snapshot.emacs_mule_order, snapshot.priority);
    let expected = decoded
        .charsets
        .iter()
        .find(|info| !info.supplementary_p)
        .expect("ordinary fixture");
    assert_eq!(
        crate::emacs_core::charset::EmacsMuleEncoder::new().encode_char(0x1f300),
        Some((
            expected.emacs_mule_id.expect("Mule fixture"),
            expected.dimension,
            expected.min_code
        ))
    );
}

#[test]
fn charset_section_rejects_mule_count_exceeding_payload() {
    let mut bytes =
        charset_section_bytes(&empty_charset_registry()).expect("encode empty registry");
    bytes[HEADER_SIZE..HEADER_SIZE + 8].copy_from_slice(&1_u64.to_ne_bytes());
    let err = load_charset_section(&bytes).expect_err("missing order symbol should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}
