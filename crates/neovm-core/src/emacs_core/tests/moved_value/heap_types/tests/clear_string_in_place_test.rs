use crate::emacs_core::buffer::builtin_clear_string;
use neovm_value::heap_types::{GnuDeclaredCharacterCount, LispString, StringCharacterCountKind};
use neovm_value::tagged::gc::{TaggedHeap, set_tagged_heap};

#[test]
fn clear_string_erases_owned_payload_before_release() {
    for mut payload in [
        LispString::from_unibyte(b"secret".to_vec()),
        LispString::from_emacs_bytes("aé界".as_bytes().to_vec()),
        LispString::from_emacs_bytes("é".repeat(100_000).into_bytes()),
    ] {
        let data = payload.data();
        let capacity = payload.owned_capacity();
        let shape = (payload.schars(), payload.size_byte());
        payload.erase_owned_payload();
        // The payload remains owned and live while its erased preimage is
        // checked. Never inspect an allocation after the string releases it.
        assert_eq!(payload.data(), data);
        assert_eq!(payload.owned_capacity(), capacity);
        assert_eq!((payload.schars(), payload.size_byte()), shape);
        assert!(payload.as_bytes().iter().all(|byte| *byte == 0));
        assert!(payload.has_trailing_nul());
    }
}

#[test]
fn clear_string_replaces_supplied_count_proof_with_byte_count() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    // Retained in place, then replaced: both paths rewrite the size word.
    for (bytes, count) in [(b"secret".to_vec(), 2), ("é".repeat(4096).into_bytes(), 3)] {
        let len = bytes.len();
        let payload = LispString::from_emacs_bytes_with_declared_chars(
            bytes,
            GnuDeclaredCharacterCount::try_from(count).unwrap(),
        );
        assert_eq!(
            payload.character_count_kind(),
            StringCharacterCountKind::Supplied
        );
        let owner = heap.alloc_string(payload);

        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());

        let cleared = owner.as_lisp_string().unwrap();
        assert_eq!(
            cleared.character_count_kind(),
            StringCharacterCountKind::Computed
        );
        assert_eq!(cleared.size(), len);
        assert_eq!(cleared.schars(), len);
        assert_eq!(cleared.sbytes(), len);
        assert!(!cleared.is_multibyte());
        assert!(cleared.has_one_byte_characters());
        assert_eq!(cleared.as_bytes(), vec![0; len]);
        assert!(cleared.has_trailing_nul());
    }
}
