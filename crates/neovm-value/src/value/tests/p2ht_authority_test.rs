//! Proof 2 layout pins for the hash-table object.
//!
//! The authority word and the user-test depth live on `HashTableObj`, beside
//! the payload. The JIT reads `switch_epoch` through
//! [`super::switch_plan::SWITCH_EPOCH_OFFSET`], which is `offset_of` the real
//! field. The numbers below are pinned so a reorder fails here.

use super::*;

/// The authority word's reserved bits, as in `table_authority.rs`.
const WRITER: u64 = 1 << 63;

#[test]
fn p2ht_layout_pins_the_jit_read_offset() {
    use std::mem::{offset_of, size_of};

    // Measured after the word and the depth moved onto the object:
    // storage lost its trailing 8-byte word (200 → 192) and the shell
    // followed (272 → 264). `test_depth` had been sitting in alignment
    // padding, so removing it did not move `switch_epoch`. The object is
    // header + shell + authority + depth, rounded to the 8-byte alignment
    // (304). 304 does not fit the usable payload of the glibc 304-byte
    // chunk (304 - 8 = 296), so this shell takes the next size class.
    assert_eq!(size_of::<HashTableObj>(), 304);
    assert_eq!(size_of::<LispHashTable>(), 264);
    assert_eq!(size_of::<HashTableStorage>(), 192);

    let epoch_offset = offset_of!(HashTableObj, table.data.switch_epoch);
    assert_eq!(epoch_offset, 240);
    assert_eq!(
        switch_plan::SWITCH_EPOCH_OFFSET,
        epoch_offset,
        "the JIT const must name the real field"
    );
    assert_eq!(offset_of!(HashTableObj, authority), 288);
    assert_eq!(offset_of!(HashTableObj, test_depth), 296);
}

#[test]
fn p2ht_payload_replace_keeps_writer_and_depth_and_a_new_object_starts_clear() {
    let mut object = HashTableObj::new(LispHashTable::new_with_options(
        HashTableTest::Eq,
        0,
        None,
        1.5,
        0.8125,
    ));
    object
        .authority
        .store(WRITER | 7, std::sync::atomic::Ordering::Relaxed);
    object
        .test_depth
        .store(2, std::sync::atomic::Ordering::Relaxed);
    object.table = LispHashTable::new(HashTableTest::Eql);
    assert_eq!(
        object.authority.load(std::sync::atomic::Ordering::Relaxed),
        WRITER | 7,
        "replacing the payload keeps the writer word"
    );
    assert_eq!(
        object.test_depth.load(std::sync::atomic::Ordering::Relaxed),
        2,
        "replacing the payload keeps the user-test depth"
    );
    assert_eq!(object.table.test, HashTableTest::Eql);

    // A cloned payload wrapped in a new object starts quiescent and at
    // depth zero. The copy does not inherit this object's in-flight state.
    let fresh = HashTableObj::new(object.table.clone());
    assert_eq!(
        fresh.authority.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        fresh.test_depth.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
}

#[test]
fn p2ht_fresh_constructions_are_quiescent() {
    // Every constructor the engine reaches a new table through starts the
    // object word and the depth at zero.
    let fresh = HashTableObj::new(LispHashTable::new_with_options(
        HashTableTest::Eq,
        4,
        None,
        1.5,
        0.8125,
    ));
    assert_eq!(
        fresh.authority.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
    assert_eq!(
        fresh.test_depth.load(std::sync::atomic::Ordering::Relaxed),
        0
    );
}

/// B5r: `sealed_storage_bytes` must count the growth sources that
/// `known_storage_bytes`'s Vec-capacity accounting misses — the user-hash
/// memo maps (slots, and the boxed content of their keys) and the boxed
/// `HashKey` materialization behind index entries. These are the bytes a
/// sealed lease charges its owner (§4 Rev R3); `eq`-test keys must stay at
/// zero so the common tables charge exactly their Vec growth.
#[test]
fn p2ht_sealed_storage_bytes_counts_memo_maps_and_boxed_keys() {
    let mut storage = HashTableStorage::default();
    let base = storage.sealed_storage_bytes();
    assert_eq!(base, storage.known_storage_bytes());

    // An eq-test key never materializes heap bytes.
    assert_eq!(HashKey::Int(7).heap_bytes(), 0);
    assert_eq!(HashKey::Ptr(0x1234).heap_bytes(), 0);

    // A boxed key in the index: 33 bignum limbs are 264 heap bytes the
    // index's capacity accounting cannot see.
    let bignum = HashKey::Bignum(vec![1u64; 33].into());
    assert_eq!(bignum.heap_bytes(), 33 * 8);
    storage.index.insert_new(7, bignum, 0);
    let with_index = storage.sealed_storage_bytes();
    assert!(with_index >= base + 33 * 8);

    // The user-hash memo maps: their slots and the boxed content of their
    // keys are both outside `known_storage_bytes` (a user-test table's
    // growth source). One remembered Text key under one bucket.
    let text = HashKey::Text("abcdefgh".into());
    assert_eq!(text.heap_bytes(), 8);
    storage.user_hashes.insert(text.clone(), 7);
    storage.user_buckets.insert(7, smallvec::smallvec![text]);
    let with_memos = storage.sealed_storage_bytes();
    assert!(with_memos > with_index);
    // The boxed text is present somewhere in the delta (memo key content).
    assert!(with_memos >= with_index + 8);
}
