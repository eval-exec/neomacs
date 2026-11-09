//! Core-owned half of the historical `heap_types::clear_string_in_place_tests`
//! module (Prep D C split, R7a).
//!
//! The five tests that call the upper `builtin_clear_string` boundary moved
//! here with unchanged assertions. The lower-only
//! `clear_string_erases_owned_payload_before_release` test stays beside the
//! `LispString` implementation in `neovm-value`. Private value state is read
//! through its already-signed routes: the `data`/`size` field splits, the
//! A-prime `owned_capacity()`, `has_trailing_nul()` and `borrowed_alias()`
//! methods, the production `character_count_kind()` and
//! `collection_observed()` reads, and the test-support `has_owned_storage`
//! predicate (its only route). The `mark_collection_observed` driver and the
//! `from_borrowed_bytes` mint cross through the signed authority fence.
use crate::emacs_core::buffer::builtin_clear_string;
use crate::tagged::collection_reads::capture;
use crate::tagged::gc::{TaggedHeap, set_tagged_heap};
use crate::tagged::header::StringObj;
use neovm_value::heap_types::{LispString, SIZE_BYTE_UNIBYTE_IMMOVABLE};
use neovm_value::test_support::authority::{
    heap_types__LispString__from_borrowed_bytes, mark_collection_observed,
};
use neovm_value::test_support::heap_types__LispString__has_owned_storage;

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
        let data = payload.data();
        let capacity = payload.owned_capacity();
        let owner = heap.alloc_string(payload);
        for _ in 0..2 {
            assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());
            let cleared = owner.as_lisp_string().unwrap();
            assert_eq!(cleared.data(), data);
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
    let multibyte =
        unsafe { heap_types__LispString__from_borrowed_bytes(MULTIBYTE.as_ptr(), 6, 3, 6, false) };
    for payload in [
        LispString::from_rodata_unibyte(UNIBYTE),
        multibyte,
        // SAFETY: the static bytes are live and NUL-terminated. A dump can
        // hydrate the immovable marker over borrowed storage; it is not writable.
        unsafe {
            heap_types__LispString__from_borrowed_bytes(
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
        let data = payload.data();
        let alias = payload.borrowed_alias().unwrap();
        let owner = heap.alloc_string(payload);

        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());

        let cleared = owner.as_lisp_string().unwrap();
        assert_eq!(heap_types__LispString__has_owned_storage(cleared), len != 0);
        assert_eq!(cleared.as_bytes(), vec![0; len]);
        assert_eq!(cleared.schars(), len);
        if len == 0 {
            assert_eq!(cleared.data(), data);
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
        let before_data = payload.data();
        let before_capacity = payload.owned_capacity();
        let owner = heap.alloc_string(payload);
        assert!(owner.as_lisp_string().unwrap().is_immovable());
        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());
        // SAFETY: this heap owns the live string, with no intervening safepoint.
        let object = unsafe { &*owner.as_string_ptr().unwrap().cast::<StringObj>() };
        assert_eq!(object.data.data(), before_data);
        assert_eq!(object.data.owned_capacity(), before_capacity);
        assert_eq!(object.data.is_immovable(), len == 0);
        assert_eq!(object.data.as_bytes(), vec![0; len]);
        assert!(object.data.has_trailing_nul());
        assert!(builtin_clear_string(vec![owner]).unwrap().is_nil());
        assert_eq!(owner.as_lisp_string().unwrap().as_bytes(), vec![0; len]);
        assert_eq!(bytecode.as_slice(), original);
    }
}
