use super::{GnuDeclaredCharacterCount, LispString, StringCharacterCountKind};

fn declared(bytes: &[u8], count: i128) -> LispString {
    LispString::from_emacs_bytes_with_declared_chars(
        bytes.to_vec(),
        GnuDeclaredCharacterCount::try_from(count).unwrap(),
    )
}

#[test]
fn character_count_kind_normal_constructors_have_computed_proof() {
    for string in [
        LispString::from_utf8("éλ"),
        LispString::from_emacs_bytes("éλ".as_bytes().to_vec()),
        LispString::from_emacs_bytes_with_chars("éλ".as_bytes().to_vec(), 2),
        LispString::from_unibyte(vec![233]),
        LispString::from_rodata_unibyte(b"abc\0"),
        LispString::from_utf8(""),
    ] {
        assert_eq!(
            string.character_count_kind(),
            StringCharacterCountKind::Computed
        );
        assert_eq!(string.size, string.schars());
    }
}

#[test]
fn character_count_kind_supplied_count_is_masked_independently_of_bytes() {
    for (bytes, count) in [(&b"abcde"[..], 0), (&b"abcde"[..], 2), ("é".as_bytes(), 1)] {
        let string = declared(bytes, count);
        assert_eq!(
            string.character_count_kind(),
            StringCharacterCountKind::Supplied
        );
        assert_eq!(string.schars(), count as usize);
        assert_eq!(string.sbytes(), bytes.len());
        assert_eq!(string.as_bytes(), bytes);
        assert_ne!(string.size, string.schars());
    }
    let maximum = declared(b"a", isize::MAX as i128);
    assert_eq!(maximum.schars(), isize::MAX as usize);
    assert_eq!(maximum.sbytes(), 1);
}

#[test]
fn character_count_kind_copies_preserve_the_encoded_word() {
    let source = declared(b"abcde", 2);
    for copy in [source.clone(), source.without_properties()] {
        assert_eq!(
            copy.character_count_kind(),
            StringCharacterCountKind::Supplied
        );
        assert_eq!(copy.size, source.size);
        assert_eq!(copy.schars(), 2);
        assert_eq!(copy.sbytes(), 5);
    }
    let rodata = LispString::from_rodata_unibyte(b"abc\0");
    let alias = rodata.borrowed_alias().unwrap();
    assert_eq!(
        alias.character_count_kind(),
        StringCharacterCountKind::Computed
    );
    assert_eq!(alias.schars(), 3);
}

#[test]
fn character_count_kind_character_slices_preserve_unverified_proof() {
    let supplied = declared(b"abcde", 2);
    let slice = supplied
        .slice_no_properties_with_char_bounds(0, 3, 0, 1)
        .unwrap();
    assert_eq!(
        slice.character_count_kind(),
        StringCharacterCountKind::Supplied
    );
    assert_eq!(slice.schars(), 1);
    assert_eq!(slice.sbytes(), 3);
    let computed = LispString::from_utf8("éa");
    let slice = computed
        .slice_no_properties_with_char_bounds(0, 2, 0, 1)
        .unwrap();
    assert_eq!(
        slice.character_count_kind(),
        StringCharacterCountKind::Computed
    );
    assert_eq!(slice.schars(), 1);
}

#[test]
fn character_count_kind_byte_slices_regenerate_computed_proof() {
    let supplied = declared(b"abcde", 2);
    let slice = supplied.slice_no_properties(1, 4).unwrap();
    assert_eq!(
        slice.character_count_kind(),
        StringCharacterCountKind::Computed
    );
    assert_eq!(slice.schars(), 3);
    assert_eq!(slice.as_bytes(), b"bcd");
}

#[test]
fn character_count_kind_recounting_mutation_regenerates_computed_proof() {
    let mut supplied = declared(b"abcde", 2);
    supplied.set_byte_same_char_count(0, b'z');
    assert_eq!(
        supplied.character_count_kind(),
        StringCharacterCountKind::Supplied
    );
    supplied.mutate_bytes(|bytes| bytes.push(b'f'));
    assert_eq!(
        supplied.character_count_kind(),
        StringCharacterCountKind::Computed
    );
    assert_eq!(supplied.schars(), 6);
}

#[test]
fn character_count_kind_dump_reconstruction_regenerates_proof_once() {
    for count in [0, 2, 5] {
        let string = LispString::from_dump(b"abcde".to_vec(), count, 5);
        assert_eq!(string.schars(), count);
        assert_eq!(
            string.character_count_kind(),
            if count == 5 {
                StringCharacterCountKind::Computed
            } else {
                StringCharacterCountKind::Supplied
            }
        );
    }
    let empty = declared(b"", 9);
    assert_eq!(empty.schars(), 0);
    assert_eq!(
        empty.character_count_kind(),
        StringCharacterCountKind::Supplied
    );
}

#[test]
fn character_count_kind_interner_reconstruction_uses_the_declared_count() {
    let string = LispString::from_bytes_with_schars(b"abcde", 2);
    assert_eq!(
        string.character_count_kind(),
        StringCharacterCountKind::Supplied
    );
    assert_eq!(string.schars(), 2);
    let unicode = LispString::from_bytes_with_schars("é".as_bytes(), 1);
    assert_eq!(
        unicode.character_count_kind(),
        StringCharacterCountKind::Computed
    );
}

#[test]
fn one_byte_characters_require_a_computed_count_equal_to_the_byte_count() {
    for (string, one_byte) in [
        (LispString::from_utf8("abc"), true),
        (LispString::from_utf8(""), true),
        (LispString::from_unibyte(vec![0xe9, 0x41]), true),
        (LispString::from_utf8("aé"), false),
        // A supplied count never qualifies, even one equal to the bytes.
        (declared(b"abc", 3), false),
        (declared(&[0xe9, 0xe9, 0xe9], 3), false),
        // An image string whose equal count cannot be decoded stays supplied.
        (LispString::from_dump(vec![0xe9, 0xe9, 0xe9], 3, 3), false),
        (LispString::from_dump(b"abc".to_vec(), 3, 3), true),
    ] {
        assert_eq!(
            string.has_one_byte_characters(),
            one_byte,
            "{:?}",
            string.as_bytes()
        );
    }
}
