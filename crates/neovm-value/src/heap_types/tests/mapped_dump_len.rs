//! A mapped dump string must refuse a header that disagrees with its bytes.

use super::{LispString, MappedStringLengthError};

#[test]
fn mapped_dump_string_rejects_a_header_that_disagrees_with_the_mapping() {
    // The allocation is larger than every header below, so a release build
    // that skips the length check reads only this buffer and returns Ok.
    let storage = vec![0u8; 32];
    let cases = [(3usize, 8usize, 8i64), (2, 8, -1), (8, 3, 3)];
    for (len, size, size_byte) in cases {
        let expected_header = if size_byte < 0 {
            size
        } else {
            size_byte as usize
        };
        let result =
            unsafe { LispString::from_mapped_bytes(storage.as_ptr(), len, size, size_byte) };
        assert_eq!(
            result.err(),
            Some(MappedStringLengthError {
                mapped_len: len,
                header_len: expected_header,
            }),
            "accepted mapped len {len} with size {size} size_byte {size_byte}"
        );
    }
}

#[test]
fn mapped_dump_string_accepts_a_header_that_matches_the_mapping() {
    let storage = b"abc\0".to_vec();
    let string = unsafe { LispString::from_mapped_bytes(storage.as_ptr(), 3, 3, 3) }
        .expect("matching header");
    assert_eq!(string.as_bytes(), b"abc");
    assert_eq!(string.sbytes(), 3);
    assert_eq!(string.schars(), 3);
}
