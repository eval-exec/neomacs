//! Observation belongs to the atomic owner header, never to payload capacity.
use super::LispString;
use crate::tagged::gc::{
    TaggedHeap, collection_observed, mark_collection_observed, set_tagged_heap,
};
use crate::tagged::header::StringObj;
use crate::tagged::mutate::{set_string_byte_same_char_count, with_lisp_string_mut};
use crate::tagged::value::TaggedValue;
use std::sync::atomic::Ordering;

fn string_owner(payload: LispString) -> (Box<TaggedHeap>, TaggedValue) {
    crate::test_utils::init_test_tracing();
    let mut heap = Box::new(TaggedHeap::new());
    set_tagged_heap(&mut heap);
    let owner = heap.alloc_string(payload);
    assert!(!collection_observed(owner.bits()));
    assert!(mark_collection_observed(owner.bits()));
    (heap, owner)
}

fn assert_owned_string(owner: TaggedValue) {
    // SAFETY: the retained heap owns this object; no allocation or safepoint intervenes.
    let object = unsafe { &*owner.as_string_ptr().unwrap().cast::<StringObj>() };
    assert!(object.header.collection_observed());
    assert!(object.data.has_owned_storage());
    assert_eq!(object.data.storage_capacity, object.data.owned_capacity());
    assert!(object.data.owned_capacity() >= object.data.sbytes() + 1);
    assert!(object.data.has_trailing_nul());
}

#[test]
fn string_owner_observation_preserves_plain_capacity_and_layout() {
    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(std::mem::offset_of!(LispString, storage_capacity), 32);
        assert_eq!(std::mem::size_of::<LispString>(), 40);
    }
    let payload = LispString::from_unibyte(b"abc".to_vec());
    let capacity = payload.owned_capacity();
    let (_heap, owner) = string_owner(payload);
    // SAFETY: the retained heap owns this live string, and no safepoint intervenes.
    let object = unsafe { &*owner.as_string_ptr().unwrap().cast::<StringObj>() };
    assert_eq!(object.data.storage_capacity, capacity);
    assert!(!mark_collection_observed(owner.bits()));
    assert_owned_string(owner);
}

#[test]
fn string_owner_observation_survives_growth_shrink_and_replacement() {
    let (_heap, owner) = string_owner(LispString::from_unibyte(b"abc".to_vec()));
    let (_, reads) = crate::tagged::collection_reads::capture(|| owner.as_str_owned());
    let reads = reads.expect("initial string dependency");
    with_lisp_string_mut(owner, |string| {
        let capacity = string.owned_capacity();
        string.mutate_bytes(|bytes| bytes.resize(capacity + 1024, b'x'));
        assert!(string.owned_capacity() > capacity);
    })
    .unwrap();
    assert_owned_string(owner);
    assert!(!reads.unchanged());
    with_lisp_string_mut(owner, |string| {
        string.mutate_bytes(|bytes| {
            bytes.truncate(2);
            bytes.shrink_to_fit();
        });
        assert_eq!(string.as_bytes(), b"ab");
        string.set_from_str("replacement");
    })
    .unwrap();
    assert_eq!(owner.as_str_owned().as_deref(), Some("replacement"));
    assert_owned_string(owner);
}

#[test]
fn string_owner_observation_survives_storage_guard_unwind() {
    let (_heap, owner) = string_owner(LispString::from_unibyte(b"abc".to_vec()));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_lisp_string_mut(owner, |string| {
            let capacity = string.owned_capacity();
            let mut storage = string.owned_data_guard();
            storage.reserve_exact(capacity + 128);
            panic!("unwind owned storage guard");
        });
    }));
    assert!(result.is_err());
    assert_eq!(owner.as_str_owned().as_deref(), Some("abc"));
    assert_owned_string(owner);
}

#[test]
fn borrowed_string_owner_observation_survives_materialization() {
    let (mut heap, owner) = string_owner(LispString::from_rodata_unibyte(b"abc\0"));
    let alias = {
        // SAFETY: the heap owns this live string; borrowing does not run a safepoint.
        let object = unsafe { &*owner.as_string_ptr().unwrap().cast::<StringObj>() };
        assert_eq!(object.data.storage_capacity, 0);
        object
            .data
            .borrowed_alias()
            .expect("borrowed bytes may alias")
    };
    let alias = heap.alloc_string(alias);
    assert!(
        !collection_observed(alias.bits()),
        "new owner has no observation"
    );
    let (_, reads) = crate::tagged::collection_reads::capture(|| owner.as_str_owned());
    let reads = reads.expect("borrowed owner dependency");
    assert!(set_string_byte_same_char_count(owner, 0, b'x'));
    assert_eq!(owner.as_str_owned().as_deref(), Some("xbc"));
    assert_eq!(alias.as_str_owned().as_deref(), Some("abc"));
    assert!(!reads.unchanged());
    assert_owned_string(owner);
}

#[test]
fn string_clone_and_reclamation_keep_observation_on_owner_only() {
    let (mut heap, owner) = string_owner(LispString::from_unibyte(b"abc".to_vec()));
    let copy = {
        // SAFETY: the heap retains this owner while its payload is cloned.
        let object = unsafe { &*owner.as_string_ptr().unwrap().cast::<StringObj>() };
        object.data.clone()
    };
    let copy = heap.alloc_string(copy);
    assert!(!collection_observed(copy.bits()));
    heap.collect_exact(std::iter::once(copy));
    // Metadata lookup never dereferences this reclaimed owner's address.
    assert!(!crate::tagged::gc::collection_observed_metadata(
        owner.bits()
    ));
    assert_eq!(copy.as_str_owned().as_deref(), Some("abc"));
    assert!(!collection_observed(copy.bits()));
}

#[test]
fn string_shared_observers_publish_header_without_touching_capacity() {
    crate::test_utils::init_test_tracing();
    let owner = Box::new(StringObj {
        header: crate::tagged::header::GcHeader::new(crate::tagged::header::HeapObjectKind::String),
        data: LispString::from_unibyte(b"abc".to_vec()),
    });
    let capacity = owner.data.storage_capacity;
    // Derive the published byte from the header's own state transition.
    // The real owner/metadata publisher is exercised by the mutator-local
    // tests above; scoped workers share only this header's atomic byte.
    assert!(owner.header.mark_collection_observed());
    let observed_mark = owner.header.collection_observed.load(Ordering::Acquire);
    owner.header.clear_collection_observed();
    let observation = &owner.header.collection_observed;
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(move || {
                // AtomicU8 is shared; neither Lisp values nor payload pointers
                // cross threads, and the parent retains the header until join.
                for _ in 0..1024 {
                    observation.fetch_or(observed_mark, Ordering::Release);
                }
            });
        }
    });
    assert!(owner.header.collection_observed());
    assert_eq!(owner.data.storage_capacity, capacity);
    assert_eq!(owner.data.as_bytes(), b"abc");
    owner.header.clear_collection_observed();
}
