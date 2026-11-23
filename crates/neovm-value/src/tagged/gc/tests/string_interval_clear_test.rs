//! `clear_string_intervals` matches GNU `set_text_properties` for a whole string.
//!
//! An interval-free string returns false and does not journal. A string that
//! has an interval tree returns true, records that write, and drops the tree
//! (`src/textprop.c`, emacs-31.1).
use super::*;
use crate::heap_types::LispString;
use crate::tagged::collection_reads::journal_since_for_test;
use crate::tagged::mutate::{
    LispCollectionRevision, clear_string_intervals, with_string_text_props_mut,
};

fn has_intervals(value: TaggedValue) -> bool {
    let ptr = value.as_string_ptr().expect("string");
    // SAFETY: the fixture string is live on this heap and uniquely owned.
    unsafe { (*ptr).data.has_intervals() }
}

#[test]
fn interval_free_clear_returns_false_and_does_not_journal() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let value = heap.alloc_string(LispString::from_unibyte(b"abc".to_vec()));
    assert!(!has_intervals(value));
    let start = LispCollectionRevision::current();

    assert!(
        !clear_string_intervals(value),
        "an interval-free string clear is GNU nil"
    );

    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        Vec::<usize>::new(),
        "a no-op clear must not advance the collection revision"
    );
    assert!(!has_intervals(value));
}

#[test]
fn clearing_an_interval_tree_journals_and_drops_it() {
    let mut heap = TaggedHeap::new();
    set_tagged_heap(&mut heap);
    let value = heap.alloc_string(LispString::from_unibyte(b"prop".to_vec()));
    with_string_text_props_mut(value, |_| {}).expect("install an interval tree");
    assert!(has_intervals(value));
    let start = LispCollectionRevision::current();

    assert!(clear_string_intervals(value));

    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        vec![value.bits()],
        "dropping an interval tree records one collection write"
    );
    assert!(!has_intervals(value));
}
