//! A dump's physical byte count must be validated before owning its payload.

use neovm_value::heap_types::LispString;
use std::mem::ManuallyDrop;

#[test]
fn dump_string_rejects_inconsistent_storage_lengths() {
    for (bytes, chars, size_byte) in [
        (Vec::new(), 1000, 1000),
        (vec![b'a', b'b', b'c'], 1, 1),
        (Vec::new(), 1000, -1),
        (vec![b'a', b'b'], 1, -1),
    ] {
        let result = std::panic::catch_unwind(|| {
            // On the old release constructor, the record has inconsistent
            // metadata. Do not read its payload or run its destructor: the
            // regression observes only whether construction was refused.
            let _record = ManuallyDrop::new(LispString::from_dump(bytes, chars, size_byte));
        });
        assert!(
            result.is_err(),
            "accepted inconsistent dump storage lengths"
        );
    }
}

#[test]
fn dump_string_preserves_valid_owned_storage() {
    for (bytes, chars, size_byte) in [
        (Vec::new(), 0, 0),
        (Vec::new(), 0, -1),
        (vec![0, 0xff, b'a'], 3, -1),
        (b"abc".to_vec(), 3, 3),
        ("é".as_bytes().to_vec(), 1, 2),
    ] {
        let record = LispString::from_dump(bytes.clone(), chars, size_byte);
        assert_eq!(record.as_bytes(), bytes);
        assert_eq!(record.schars(), chars);
        assert_eq!(record.sbytes(), bytes.len());
    }
}
