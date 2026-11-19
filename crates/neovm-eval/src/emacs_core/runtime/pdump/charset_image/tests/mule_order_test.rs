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
        priority_identity_syms: Some(vec![DumpSymId(43), DumpSymId(42)]),
        alias_syms: Some(vec![(DumpSymId(42), DumpSymId(43))]),
        iso_designation_syms: Some(vec![((2, false, 65), DumpSymId(43))]),
        published_names: None,
        binding_slots: None,
        plist_slots: None,
        iso2022_members: None,
        plist_lisp: None,
        next_id: 922,
    }
}

#[test]
fn charset_section_mule_order_round_trip_keeps_definition_chronology() {
    let registry = overlapping_registry();
    let bytes = charset_section_bytes(&registry).expect("encode overlapping registry");
    assert_eq!(read_header(&bytes).expect("header").version, 6);
    let decoded = load_charset_section(&bytes).expect("decode overlapping registry");
    assert_eq!(
        decoded.emacs_mule_order_syms,
        registry.emacs_mule_order_syms
    );
    assert_eq!(decoded.priority, registry.priority);
    assert_eq!(
        decoded.priority_identity_syms,
        registry.priority_identity_syms
    );
    assert_eq!(decoded.alias_syms, registry.alias_syms);
    assert_eq!(decoded.iso_designation_syms, registry.iso_designation_syms);
    assert_eq!(format!("{decoded:?}"), format!("{registry:?}"));
}

#[test]
fn charset_section_accepts_legacy_and_derives_mule_priority() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new(crate::test_utils::unit_runtime());
    let mut registry = overlapping_registry();
    registry.emacs_mule_order_syms = None;
    registry.priority_identity_syms = None;
    registry.alias_syms = None;
    registry.iso_designation_syms = None;
    let mut bytes = charset_section_bytes(&registry).expect("encode registry");
    // Recreate the exact version-1 layout: identical header/records, without
    // versions 2/3/4's final optional metadata. No GNU oracle bytes are involved.
    assert_eq!(
        &bytes[bytes.len() - 8..],
        &ABSENT_EMACS_MULE_ORDER.to_ne_bytes()
    );
    // Versions 2 through 6 each end with an ABSENT marker when the optional
    // metadata is missing: mule order, identities, aliases, ISO winners,
    // published names, binding slots, plist slots, ISO-2022 members, plist
    // lisp.
    bytes.truncate(bytes.len() - 72);
    let mut header = read_header(&bytes).expect("version-6 header");
    header.version = 1;
    header.payload_len -= 72;
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
    let snapshot = neovm_text::emacs_core::charset::snapshot_charset_registry();
    assert_eq!(snapshot.emacs_mule_order, snapshot.priority);
    let expected = decoded
        .charsets
        .iter()
        .find(|info| !info.supplementary_p)
        .expect("ordinary fixture");
    assert_eq!(
        neovm_text::emacs_core::charset::prepared_mule::EmacsMuleEncoder::new()
            .encode_char(0x1f300),
        Some((
            expected.emacs_mule_id.expect("Mule fixture"),
            expected.dimension,
            expected.min_code
        ))
    );
}

#[test]
fn charset_section_accepts_version_two_without_identity_alias_metadata() {
    let registry = overlapping_registry();
    let mut bytes = charset_section_bytes(&registry).expect("encode registry");
    // v3 adds count + two identity symbols and count + one alias pair; v4
    // adds count + one ISO designation entry (8 + 1 + 8 + 4 bytes).
    // v3 identities and aliases, v4 ISO entry, three v5 ABSENT markers, and
    // two v6 ABSENT markers.
    let trailer_len = 8 + 2 * 4 + 8 + 2 * 4 + 8 + 21 + 24 + 16;
    bytes.truncate(bytes.len() - trailer_len);
    let mut header = read_header(&bytes).expect("version-4 header");
    header.version = 2;
    header.payload_len -= trailer_len as u64;
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    let decoded = load_charset_section(&bytes).expect("read version-2 section");
    assert_eq!(
        decoded.emacs_mule_order_syms,
        registry.emacs_mule_order_syms
    );
    assert_eq!(decoded.priority, registry.priority);
    assert!(decoded.priority_identity_syms.is_none());
    assert!(decoded.alias_syms.is_none());
    assert!(decoded.iso_designation_syms.is_none());
}

#[test]
fn charset_section_accepts_version_three_without_iso_designations() {
    let registry = overlapping_registry();
    let mut bytes = charset_section_bytes(&registry).expect("encode registry");
    // v4 ISO entry, three v5 ABSENT markers, and two v6 ABSENT markers.
    let trailer_len = 8 + 21 + 24 + 16;
    bytes.truncate(bytes.len() - trailer_len);
    let mut header = read_header(&bytes).expect("version-4 header");
    header.version = 3;
    header.payload_len -= trailer_len as u64;
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    let decoded = load_charset_section(&bytes).expect("read version-3 section");
    assert_eq!(decoded.alias_syms, registry.alias_syms);
    assert!(decoded.iso_designation_syms.is_none());
    assert!(decoded.binding_slots.is_none());
}

#[test]
fn charset_section_accepts_version_four_without_plist_slot_ids() {
    let registry = overlapping_registry();
    let mut bytes = charset_section_bytes(&registry).expect("encode registry");
    // Three v5 ABSENT markers plus two v6 ABSENT markers.
    let trailer_len = 24 + 16;
    bytes.truncate(bytes.len() - trailer_len);
    let mut header = read_header(&bytes).expect("version-6 header");
    header.version = 4;
    header.payload_len -= trailer_len as u64;
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    let decoded = load_charset_section(&bytes).expect("read version-4 section");
    assert_eq!(decoded.iso_designation_syms, registry.iso_designation_syms);
    assert!(decoded.published_names.is_none());
    assert!(decoded.binding_slots.is_none());
    assert!(decoded.plist_slots.is_none());
}

#[test]
fn charset_section_accepts_version_five_without_member_or_lisp_trailer() {
    let registry = overlapping_registry();
    let mut bytes = charset_section_bytes(&registry).expect("encode registry");
    // Two ABSENT markers: ISO-2022 members and cached plist objects.
    let trailer_len = 16;
    bytes.truncate(bytes.len() - trailer_len);
    let mut header = read_header(&bytes).expect("version-6 header");
    header.version = 5;
    header.payload_len -= trailer_len as u64;
    bytes[..HEADER_SIZE].copy_from_slice(bytemuck::bytes_of(&header));
    let decoded = load_charset_section(&bytes).expect("read version-5 section");
    assert_eq!(decoded.iso_designation_syms, registry.iso_designation_syms);
    assert!(decoded.iso2022_members.is_none());
    assert!(decoded.plist_lisp.is_none());
    assert!(decoded.published_names.is_none());
}

#[test]
fn charset_section_rejects_mule_count_exceeding_payload() {
    let mut bytes =
        charset_section_bytes(&empty_charset_registry()).expect("encode empty registry");
    bytes[HEADER_SIZE..HEADER_SIZE + 8].copy_from_slice(&100_u64.to_ne_bytes());
    let err = load_charset_section(&bytes).expect_err("missing order symbol should fail");
    assert!(matches!(err, DumpError::ImageFormatError(_)));
}

#[test]
fn charset_section_rejects_identity_and_alias_counts_exceeding_payload() {
    let original = charset_section_bytes(&empty_charset_registry()).expect("encode empty registry");
    // Each absent optional count occupies eight bytes. The huge non-sentinel
    // count would overflow a Vec capacity if allocated before payload bounds
    // are checked; the new trailers must return a format error instead.
    for count_offset in [HEADER_SIZE + 8, HEADER_SIZE + 16, HEADER_SIZE + 24] {
        let mut bytes = original.clone();
        bytes[count_offset..count_offset + 8].copy_from_slice(&(u64::MAX - 1).to_ne_bytes());
        let err = load_charset_section(&bytes).expect_err("new trailer count exceeds payload");
        assert!(matches!(err, DumpError::ImageFormatError(_)));
    }
}
