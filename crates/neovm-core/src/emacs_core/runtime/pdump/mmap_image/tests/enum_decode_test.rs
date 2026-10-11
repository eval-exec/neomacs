use super::*;

#[test]
fn mmap_section_kind_rejects_unknown_code_with_typed_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("invalid-section-kind.pdump");
    write_image(
        &path,
        &[ImageSection {
            kind: DumpSectionKind::Metadata,
            flags: 0,
            bytes: b"metadata",
        }],
    )
    .unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[HEADER_SIZE..HEADER_SIZE + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    let checksum = checksum_body(&bytes);
    let header: &mut DumpImageHeader = bytemuck::from_bytes_mut(&mut bytes[..HEADER_SIZE]);
    header.checksum = checksum;
    std::fs::write(&path, bytes).unwrap();
    let error = load_image(&path)
        .err()
        .expect("invalid section kind should fail");
    assert!(matches!(error, DumpError::InvalidSectionKind(source) if source.number == u32::MAX));
}
