use super::*;

#[test]
fn character_count_kind_baked_image_regenerates_masked_size_without_schema_change() {
    for (bytes, count, supplied) in [
        (&b"abcde"[..], 0, true),
        (&b"abcde"[..], 2, true),
        (&b"abcde"[..], 5, false),
        ("é".as_bytes(), 1, false),
        // Equal counts mean one byte per character to compiled code, so a
        // non-ASCII payload keeps its count supplied even when its lead
        // bytes happen to add up to it.
        (&[0xe9, 0xe9, 0xe9][..], 3, true),
    ] {
        let mut builder = MappedHeapBuilder {
            bytes: vec![0; std::mem::size_of::<RawStringObj>()],
            relocations: Vec::new(),
            value_fixups: Vec::new(),
        };
        let span = builder.push_bytes(bytes);
        let data = DumpByteData::mapped(span.offset, span.len);
        builder.write_raw_string_obj(0, count, bytes.len() as i64, &data);
        let offset = std::mem::offset_of!(RawStringObj, size);
        let end = offset + std::mem::size_of::<usize>();
        let encoded = usize::from_ne_bytes(builder.bytes[offset..end].try_into().unwrap());
        assert_eq!(encoded & LispString::CHARACTER_COUNT_MASK, count);
        assert_eq!(encoded & !LispString::CHARACTER_COUNT_MASK != 0, supplied);
        let DumpByteData::Mapped(saved) = data else {
            unreachable!()
        };
        assert_eq!(saved.len as usize, bytes.len());
        assert_eq!(
            &builder.bytes[saved.offset as usize..saved.offset as usize + bytes.len()],
            bytes
        );
        assert_eq!(builder.bytes[saved.offset as usize + bytes.len()], 0);
    }
}
