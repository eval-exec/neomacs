use super::LispString;

#[test]
fn lisp_string_layout_keeps_gnu_fields_before_storage_metadata() {
    assert_eq!(std::mem::offset_of!(LispString, size), 0);
    assert!(std::mem::offset_of!(LispString, size_byte) > std::mem::offset_of!(LispString, size));
    assert!(
        std::mem::offset_of!(LispString, intervals) > std::mem::offset_of!(LispString, size_byte)
    );
    assert!(std::mem::offset_of!(LispString, data) > std::mem::offset_of!(LispString, intervals));
    assert!(
        std::mem::offset_of!(LispString, storage_capacity) > std::mem::offset_of!(LispString, data)
    );
    #[cfg(target_pointer_width = "64")]
    assert_eq!(std::mem::size_of::<LispString>(), 40);
    // The interval field is an AtomicPtr (concurrent GC null-check reads):
    // same size + null niche as the GNU-compatible raw interval pointer,
    // and as the Option<Box<_>> it replaced.
    assert_eq!(
        std::mem::size_of::<std::sync::atomic::AtomicPtr<crate::buffer::TextPropertyTable>>(),
        std::mem::size_of::<usize>()
    );
}

#[test]
fn mapped_lisp_string_borrows_until_mutation() {
    let bytes = b"abc\0".to_vec();
    let mut string = unsafe { LispString::from_mapped_bytes(bytes.as_ptr(), 3, 3, 3) };

    assert_eq!(string.as_bytes(), b"abc");
    assert!(string.has_trailing_nul());
    string.mutate_bytes(|bytes| bytes.push(b'd'));

    drop(bytes);
    assert_eq!(string.as_bytes(), b"abcd");
    assert_eq!(string.schars(), 4);
    assert_eq!(string.sbytes(), 4);
    assert!(string.has_trailing_nul());
}

#[test]
fn mapped_lisp_string_clone_is_owned() {
    let bytes = b"abc\0".to_vec();
    let string = unsafe { LispString::from_mapped_bytes(bytes.as_ptr(), 3, 3, 3) };
    let cloned = string.clone();

    drop(bytes);
    assert_eq!(cloned.as_bytes(), b"abc");
    assert!(cloned.has_trailing_nul());
}

#[test]
fn gnu_unibyte_size_byte_states_are_distinct() {
    let normal = LispString::from_unibyte(b"abc".to_vec());
    assert_eq!(normal.size_byte(), -1);
    assert!(!normal.is_multibyte());
    assert!(!normal.is_rodata());
    assert!(!normal.is_immovable());

    let rodata = LispString::from_rodata_unibyte(b"abc\0");
    assert_eq!(rodata.size_byte(), -2);
    assert!(!rodata.is_multibyte());
    assert!(rodata.is_rodata());
    assert!(!rodata.is_immovable());
    assert_eq!(rodata.as_bytes(), b"abc");
    assert!(rodata.has_trailing_nul());

    let mut immovable = LispString::from_unibyte(b"abc".to_vec());
    immovable.pin_immovable();
    assert_eq!(immovable.size_byte(), -3);
    assert!(!immovable.is_multibyte());
    assert!(!immovable.is_rodata());
    assert!(immovable.is_immovable());
}

#[test]
fn rodata_unibyte_demotes_to_normal_on_mutation() {
    let mut string = LispString::from_rodata_unibyte(b"abc\0");
    string.mutate_bytes(|bytes| bytes[0] = b'X');

    assert_eq!(string.as_bytes(), b"Xbc");
    assert_eq!(string.size_byte(), -1);
    assert!(!string.is_rodata());
    assert!(string.has_trailing_nul());
}

#[test]
fn replacing_string_contents_updates_cached_lengths() {
    let mut multibyte = LispString::from_utf8("é");
    multibyte.set_from_str("longer");
    assert_eq!(multibyte.as_bytes(), b"longer");
    assert_eq!(multibyte.schars(), 6);
    assert_eq!(multibyte.sbytes(), 6);
    assert!(multibyte.has_trailing_nul());

    let mut rodata = LispString::from_rodata_unibyte(b"abc\0");
    rodata.set_from_str("longer");
    assert_eq!(rodata.as_bytes(), b"longer");
    assert_eq!(rodata.size_byte(), -1);
    assert!(rodata.has_owned_storage());
    assert!(rodata.has_trailing_nul());
}

#[test]
fn set_byte_same_char_count_copies_rodata_before_writing() {
    static DATA: &[u8] = b"abc\0";
    let mut string = LispString::from_rodata_unibyte(DATA);
    string.set_byte_same_char_count(1, b'z');
    assert_eq!(string.as_bytes(), b"azc");
    assert!(
        !string.is_rodata(),
        "a written string no longer points at rodata"
    );
    assert_eq!(string.schars(), 3);
    assert_eq!(DATA, b"abc\0", "the rodata itself is untouched");
    let mut multibyte = LispString::from_utf8("aéc");
    multibyte.set_byte_same_char_count(3, b'x');
    assert_eq!(multibyte.as_bytes(), "aéx".as_bytes());
    assert_eq!((multibyte.schars(), multibyte.sbytes()), (3, 4));
}

#[test]
fn mutate_bytes_recomputes_multibyte_size_and_preserves_nul() {
    let mut string = LispString::from_utf8("é");
    assert_eq!(string.schars(), 1);
    assert_eq!(string.sbytes(), 2);

    string.mutate_bytes(|bytes| bytes.extend_from_slice("x".as_bytes()));

    assert_eq!(string.as_bytes(), "éx".as_bytes());
    assert_eq!(string.schars(), 2);
    assert_eq!(string.sbytes(), 3);
    assert_eq!(string.size_byte(), 3);
    assert!(string.has_trailing_nul());
}

#[test]
fn owned_and_dump_strings_have_gnu_trailing_nul_after_sbytes() {
    let strings = [
        LispString::from_utf8("abc"),
        LispString::from_unibyte(b"abc".to_vec()),
        LispString::from_dump(b"abc".to_vec(), 3, 3),
    ];

    for string in strings {
        assert_eq!(string.as_bytes(), b"abc");
        assert!(string.has_trailing_nul());
    }
}

#[test]
fn owned_dump_data_cannot_claim_rodata_size_byte() {
    let string = LispString::from_dump(b"abc".to_vec(), 3, -2);

    assert_eq!(string.as_bytes(), b"abc");
    assert_eq!(string.size_byte(), -1);
    assert!(!string.is_rodata());
    assert!(string.has_trailing_nul());
}

#[test]
fn equal_unibyte_storage_classes_hash_identically() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let normal = LispString::from_unibyte(b"abc".to_vec());
    let mut immovable = LispString::from_unibyte(b"abc".to_vec());
    immovable.pin_immovable();

    assert_eq!(normal, immovable);

    let mut normal_hash = DefaultHasher::new();
    normal.hash(&mut normal_hash);
    let mut immovable_hash = DefaultHasher::new();
    immovable.hash(&mut immovable_hash);
    assert_eq!(normal_hash.finish(), immovable_hash.finish());
}
