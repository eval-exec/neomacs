use super::LispString;
use std::sync::atomic::{AtomicUsize, Ordering};

fn encoded_capacity(string: &LispString) -> usize {
    string.storage_capacity.load(Ordering::Acquire)
}

fn assert_owned_mark(string: &LispString, observed: bool) {
    let encoded = encoded_capacity(string);
    assert!(string.has_owned_storage());
    assert_eq!(
        encoded & LispString::OWNED_STORAGE_COLLECTION_OBSERVED_MASK != 0,
        observed
    );
    assert_eq!(
        encoded & !LispString::OWNED_STORAGE_COLLECTION_OBSERVED_MASK,
        string.owned_capacity()
    );
    assert_eq!((encoded as isize) > 0, !observed);
    assert!(string.owned_capacity() >= string.sbytes() + 1);
    assert!(string.has_trailing_nul());
}

#[test]
fn owned_string_observation_capacity_preserves_layout_and_accounting() {
    assert_eq!(
        std::mem::size_of::<AtomicUsize>(),
        std::mem::size_of::<usize>()
    );
    assert_eq!(
        std::mem::align_of::<AtomicUsize>(),
        std::mem::align_of::<usize>()
    );
    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(std::mem::offset_of!(LispString, storage_capacity), 32);
        assert_eq!(std::mem::size_of::<LispString>(), 40);
    }
    let string = LispString::from_unibyte(b"abc".to_vec());
    let capacity = string.owned_capacity();
    assert_owned_mark(&string, false);
    string.mark_owned_storage_collection_observed();
    assert_owned_mark(&string, true);
    assert_eq!(string.owned_capacity(), capacity);
    assert_eq!(string.as_bytes(), b"abc");
    string.mark_owned_storage_collection_observed();
    assert_eq!(
        encoded_capacity(&string),
        capacity | LispString::OWNED_STORAGE_COLLECTION_OBSERVED_MASK
    );
}

#[test]
fn observed_owned_string_capacity_survives_growth_shrink_and_replacement() {
    let mut string = LispString::from_unibyte(b"abc".to_vec());
    string.mark_owned_storage_collection_observed();
    let initial_capacity = string.owned_capacity();
    string.mutate_bytes(|bytes| bytes.resize(initial_capacity + 1024, b'x'));
    assert_eq!(&string.as_bytes()[..3], b"abc");
    assert!(string.owned_capacity() > initial_capacity);
    assert_owned_mark(&string, true);
    let grown_capacity = string.owned_capacity();
    string.mutate_bytes(|bytes| {
        bytes.truncate(2);
        bytes.shrink_to_fit();
    });
    assert_eq!(string.as_bytes(), b"ab");
    assert!(string.owned_capacity() < grown_capacity);
    assert_owned_mark(&string, true);
    string.set_from_str("replacement");
    assert_eq!(string.as_bytes(), b"replacement");
    assert_owned_mark(&string, true);
}

#[test]
fn owned_string_guard_preserves_observation_published_during_guard() {
    let mut string = LispString::from_unibyte(b"abc".to_vec());
    let initial_capacity = string.owned_capacity();
    {
        let mut data = string.owned_data_guard();
        data.string.mark_owned_storage_collection_observed();
        data.reserve_exact(initial_capacity + 128);
    }
    assert_eq!(string.as_bytes(), b"abc");
    assert!(string.owned_capacity() > initial_capacity);
    assert_owned_mark(&string, true);
}

#[test]
fn owned_string_guard_unwind_preserves_observation_and_allocation() {
    let mut string = LispString::from_unibyte(b"abc".to_vec());
    let initial_capacity = string.owned_capacity();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut data = string.owned_data_guard();
        data.string.mark_owned_storage_collection_observed();
        data.reserve_exact(initial_capacity + 128);
        panic!("unwind the owned storage guard");
    }));
    assert!(result.is_err());
    assert_eq!(string.as_bytes(), b"abc");
    assert!(string.owned_capacity() > initial_capacity);
    assert_owned_mark(&string, true);
}

#[test]
fn observed_owned_string_clone_transfer_and_release_reset_the_mirror() {
    let mut string = LispString::from_unibyte(b"abc".to_vec());
    string.mark_owned_storage_collection_observed();
    let cloned = string.clone();
    assert_eq!(cloned.as_bytes(), b"abc");
    assert_owned_mark(&cloned, false);
    assert_owned_mark(&string, true);
    string.clear_owned_storage_collection_observed();
    assert_owned_mark(&string, false);
    string.mark_owned_storage_collection_observed();
    // Explicit release exercises masked allocator reconstruction; Drop then
    // sees zero ownership and must not free the allocation a second time.
    string.release_owned_storage();
    assert_eq!(encoded_capacity(&string), 0);
    assert_eq!(string.owned_capacity(), 0);
    assert!(!string.has_owned_storage());
    assert!(string.data.is_null());
    drop(string);
    drop(cloned);
}

#[test]
fn borrowed_string_observation_keeps_raw_zero_and_alias_unmarked() {
    let mut string = LispString::from_rodata_unibyte(b"abc\0");
    string.mark_owned_storage_collection_observed();
    assert_eq!(encoded_capacity(&string), 0);
    assert_eq!(string.owned_capacity(), 0);
    assert!(!string.has_owned_storage());
    let alias = string.borrowed_alias().expect("unmarked borrowed alias");
    assert_eq!(encoded_capacity(&alias), 0);
    assert_eq!(alias.as_bytes(), b"abc");
    string.set_byte_same_char_count(0, b'x');
    assert_eq!(string.as_bytes(), b"xbc");
    assert_owned_mark(&string, false);
    assert_eq!(alias.as_bytes(), b"abc");
}

#[test]
fn borrowed_string_owned_observer_runs_once_before_the_byte_write() {
    let mut string = LispString::from_rodata_unibyte(b"abc\0");
    let callbacks = std::cell::Cell::new(0);
    string.set_byte_same_char_count_with_owned_observer(0, b'x', |owned| {
        callbacks.set(callbacks.get() + 1);
        assert_eq!(
            owned.as_bytes(),
            b"abc",
            "conversion precedes the byte write"
        );
        assert_owned_mark(owned, false);
        owned.mark_owned_storage_collection_observed();
    });
    assert_eq!(callbacks.get(), 1);
    assert_eq!(string.as_bytes(), b"xbc");
    assert_owned_mark(&string, true);
    string.set_byte_same_char_count_with_owned_observer(1, b'y', |_| {
        panic!("owned byte store must not call the conversion observer");
    });
    assert_eq!(callbacks.get(), 1);
    assert_eq!(string.as_bytes(), b"xyc");
    assert_owned_mark(&string, true);
}

#[test]
fn owned_string_shared_observers_publish_only_the_atomic_mirror() {
    let string = std::sync::Arc::new(LispString::from_unibyte(b"abc".to_vec()));
    let capacity = string.owned_capacity();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(4));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let string = std::sync::Arc::clone(&string);
            let barrier = std::sync::Arc::clone(&barrier);
            scope.spawn(move || {
                barrier.wait();
                string.mark_owned_storage_collection_observed();
            });
        }
    });
    assert_eq!(string.owned_capacity(), capacity);
    assert_eq!(string.as_bytes(), b"abc");
    assert_owned_mark(&string, true);
}
