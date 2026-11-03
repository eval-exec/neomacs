use super::{
    GnuDeclaredCharacterCount, LispString, SIZE_BYTE_UNIBYTE_IMMOVABLE, StringCharacterCountKind,
};
use crate::emacs_core::buffer::builtin_clear_string;
use crate::tagged::collection_reads::capture;
use crate::tagged::gc::{TaggedHeap, mark_collection_observed, set_tagged_heap};
use crate::tagged::header::StringObj;

#[test]
fn clear_string_preserves_owner_and_invalidates_observed_reads() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    for payload in [
        LispString::from_unibyte(Vec::new()),
        LispString::from_unibyte(b"secret".to_vec()),
        LispString::from_unibyte(vec![0xe9; 1000]),
        LispString::from_emacs_bytes(Vec::new()),
        LispString::from_emacs_bytes(b"secret".to_vec()),
        LispString::from_emacs_bytes("aé界".as_bytes().to_vec()),
        LispString::from_emacs_bytes("é".repeat(100_000).into_bytes()),
    ] {
        let len = payload.sbytes();
        let owner = heap.alloc_string(payload);
        let alias = owner;
        assert!(mark_collection_observed(owner.bits()));
        let (_, reads) = capture(|| owner.as_str_owned());
        let reads = reads.expect("observed string dependency");

        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());

        assert_eq!(alias.bits(), owner.bits());
        assert_eq!(alias.as_lisp_string().unwrap().as_bytes(), vec![0; len]);
        // SAFETY: this heap owns the live string; no safepoint occurs while
        // inspecting its object header.
        let object = unsafe { &*owner.as_string_ptr().unwrap().cast::<StringObj>() };
        assert_eq!(object.data.schars(), len);
        assert_eq!(object.data.sbytes(), len);
        assert!(!object.data.is_multibyte());
        assert!(object.data.as_bytes().iter().all(|byte| *byte == 0));
        assert!(object.data.has_trailing_nul());
        assert!(object.header.collection_observed());
        assert!(!reads.unchanged());
    }
}

#[test]
fn clear_string_retains_small_owned_payload_storage() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    for payload in [
        LispString::from_unibyte(b"secret".to_vec()),
        LispString::from_unibyte(vec![0xe9; 1000]),
        LispString::from_emacs_bytes("aé界".as_bytes().to_vec()),
        LispString::from_emacs_bytes("界".repeat(1000).into_bytes()),
    ] {
        let len = payload.sbytes();
        let data = payload.data;
        let capacity = payload.owned_capacity();
        let owner = heap.alloc_string(payload);
        for _ in 0..2 {
            assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());
            let cleared = owner.as_lisp_string().unwrap();
            assert_eq!(cleared.data, data);
            assert_eq!(cleared.owned_capacity(), capacity);
            assert_eq!(cleared.schars(), len);
            assert_eq!(cleared.sbytes(), len);
            assert!(!cleared.is_multibyte());
            assert_eq!(cleared.as_bytes(), vec![0; len]);
            assert!(cleared.has_trailing_nul());
        }
    }
}

#[test]
fn clear_string_materializes_borrowed_bytes_without_changing_the_source() {
    static UNIBYTE: &[u8] = b"secret\0";
    static MULTIBYTE: &[u8] = "aé界\0".as_bytes();
    static EMPTY: &[u8] = b"\0";
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    // SAFETY: static storage remains mapped, has six payload bytes and a NUL,
    // and contains three GNU multibyte characters.
    let multibyte = unsafe { LispString::from_borrowed_bytes(MULTIBYTE.as_ptr(), 6, 3, 6, false) };
    for payload in [
        LispString::from_rodata_unibyte(UNIBYTE),
        multibyte,
        // SAFETY: the static bytes are live and NUL-terminated. A dump can
        // hydrate the immovable marker over borrowed storage; it is not writable.
        unsafe {
            LispString::from_borrowed_bytes(
                UNIBYTE.as_ptr(),
                6,
                6,
                SIZE_BYTE_UNIBYTE_IMMOVABLE,
                false,
            )
        },
        LispString::from_rodata_unibyte(EMPTY),
    ] {
        let len = payload.sbytes();
        let data = payload.data;
        let alias = payload.borrowed_alias().unwrap();
        let owner = heap.alloc_string(payload);

        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());

        let cleared = owner.as_lisp_string().unwrap();
        assert_eq!(cleared.has_owned_storage(), len != 0);
        assert_eq!(cleared.as_bytes(), vec![0; len]);
        assert_eq!(cleared.schars(), len);
        if len == 0 {
            assert_eq!(cleared.data, data);
            assert!(cleared.is_rodata());
        }
        assert!(!cleared.is_multibyte());
        assert!(cleared.has_trailing_nul());
        assert_eq!(alias.sbytes(), len);
        if len != 0 {
            assert!(alias.as_bytes().iter().any(|byte| *byte != 0));
        }
    }
    assert_eq!(UNIBYTE, b"secret\0");
    assert_eq!(MULTIBYTE, "aé界\0".as_bytes());
}

#[test]
fn clear_string_keeps_immovable_owned_storage_and_demotes_shape() {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    for bytes in [Vec::new(), b"secret".to_vec()] {
        let bytecode = crate::tagged::header::LispByteVec::copy_from_slice(&bytes);
        let original = bytes.clone();
        let len = bytes.len();
        let mut payload = LispString::from_unibyte(bytes);
        payload.pin_immovable();
        let before_data = payload.data;
        let before_capacity = payload.owned_capacity();
        let owner = heap.alloc_string(payload);
        assert!(owner.as_lisp_string().unwrap().is_immovable());
        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());
        // SAFETY: this heap owns the live string, with no intervening safepoint.
        let object = unsafe { &*owner.as_string_ptr().unwrap().cast::<StringObj>() };
        assert_eq!(object.data.data, before_data);
        assert_eq!(object.data.owned_capacity(), before_capacity);
        assert_eq!(object.data.is_immovable(), len == 0);
        assert_eq!(object.data.as_bytes(), vec![0; len]);
        assert!(object.data.has_trailing_nul());
        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());
        assert_eq!(owner.as_lisp_string().unwrap().as_bytes(), vec![0; len]);
        assert_eq!(bytecode.as_slice(), original);
    }
}

#[test]
fn clear_string_erases_owned_payload_before_release() {
    for mut payload in [
        LispString::from_unibyte(b"secret".to_vec()),
        LispString::from_emacs_bytes("aé界".as_bytes().to_vec()),
        LispString::from_emacs_bytes("é".repeat(100_000).into_bytes()),
    ] {
        let data = payload.data;
        let capacity = payload.owned_capacity();
        let shape = (payload.schars(), payload.size_byte());
        payload.erase_owned_payload();
        // The payload remains owned and live while its erased preimage is
        // checked. Never inspect an allocation after the string releases it.
        assert_eq!(payload.data, data);
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
        assert_eq!(cleared.size, len);
        assert_eq!(cleared.schars(), len);
        assert_eq!(cleared.sbytes(), len);
        assert!(!cleared.is_multibyte());
        assert!(cleared.has_one_byte_characters());
        assert_eq!(cleared.as_bytes(), vec![0; len]);
        assert!(cleared.has_trailing_nul());
    }
}
