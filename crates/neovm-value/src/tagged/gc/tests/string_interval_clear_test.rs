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

fn fresh_string(bytes: &[u8]) -> TaggedValue {
    // The string is addressed by the thread-local heap. Leak the fixture so
    // it outlives this helper; nextest runs each test in its own process.
    let heap: &'static mut TaggedHeap = Box::leak(Box::new(TaggedHeap::new()));
    set_tagged_heap(heap);
    heap.alloc_string(LispString::from_unibyte(bytes.to_vec()))
}

fn char_len(value: TaggedValue) -> usize {
    value.as_lisp_string().expect("string").schars()
}

fn whole_range(value: TaggedValue) -> crate::value::positions::CharRange {
    crate::value::positions::CharRange::from_usize(0, char_len(value))
}

fn put_face(value: TaggedValue, face: TaggedValue) {
    let len = char_len(value);
    crate::value::mutate_string_text_properties(value, |table| {
        table.put_property_for_object_char_len(
            whole_range(value),
            crate::value::positions::CharLen::new(len),
            TaggedValue::symbol("face"),
            face,
        );
    })
    .expect("string");
}

/// GNU `add_text_properties_1` returns nil without modifying when the interval
/// already has the property (`src/textprop.c`, emacs-31.1).
#[test]
fn noop_put_of_an_equal_value_does_not_journal() {
    let value = fresh_string(b"abcd");
    let bold = TaggedValue::symbol("bold");
    put_face(value, bold);
    let start = LispCollectionRevision::current();

    put_face(value, bold);

    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        Vec::<usize>::new(),
        "putting the property the string already has must not journal"
    );
}

/// GNU `Fremove_text_properties` returns nil without modifying when no interval
/// in the range holds the name.
#[test]
fn remove_of_an_absent_property_does_not_journal() {
    let value = fresh_string(b"abcd");
    put_face(value, TaggedValue::symbol("bold"));
    let start = LispCollectionRevision::current();
    let names = [TaggedValue::symbol("absent-prop")];

    let removed = crate::value::mutate_string_text_properties(value, |table| {
        table.remove_properties_in_char_range(whole_range(value), &names)
    })
    .expect("string");

    assert!(!removed, "the name is not on the string");
    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        Vec::<usize>::new(),
        "a remove that removes nothing must not journal"
    );
}

/// Replacing the interval tree with an empty one is one collection write.
/// `mutate_string_text_properties` used to journal inside the closure and
/// again when it nulled the empty table.
#[test]
fn emptying_a_string_interval_tree_journals_once() {
    let value = fresh_string(b"abcd");
    put_face(value, TaggedValue::symbol("bold"));
    assert!(has_intervals(value));
    let start = LispCollectionRevision::current();

    crate::value::mutate_string_text_properties(value, |table| {
        *table = crate::value::text_props::TextPropertyTable::new();
    })
    .expect("string");

    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        vec![value.bits()],
        "emptying the tree records one collection write"
    );
    assert!(!has_intervals(value));
}

/// `set_string_text_properties_table_for_value` eq-identical write-back is
/// not a mutation. The clone shares the live plist objects.
#[test]
fn writing_an_equal_text_property_table_does_not_journal() {
    let value = fresh_string(b"abcd");
    put_face(value, TaggedValue::symbol("bold"));
    let table = crate::value::get_string_text_properties_table_for_value(value)
        .expect("the face property is installed");
    let start = LispCollectionRevision::current();

    crate::value::set_string_text_properties_table_for_value(value, table);

    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        Vec::<usize>::new(),
        "writing an equal interval table must not journal"
    );
    assert!(has_intervals(value));
}

/// A copied plist spine is `equal` to the live one and not `eq`. Writing it
/// back must install the new conses. `compose-string` does not reach this
/// skip: its second put `set_car`s the shared value cell before write-back.
#[test]
fn write_back_of_an_equal_plist_spine_installs_the_new_conses() {
    let value = fresh_string(b"abcd");
    put_face(value, TaggedValue::symbol("bold"));
    let before = crate::value::get_string_text_properties_table_for_value(value)
        .expect("table")
        .interval_plist_at_char_pos(crate::value::positions::CharPos0::ZERO)
        .expect("plist");
    let copied = crate::value::get_string_text_properties_table_for_value(value)
        .expect("table")
        .copy_interval_plist_spines();

    crate::value::set_string_text_properties_table_for_value(value, copied);

    let after = crate::value::get_string_text_properties_table_for_value(value)
        .expect("table")
        .interval_plist_at_char_pos(crate::value::positions::CharPos0::ZERO)
        .expect("plist");
    assert!(
        !crate::value::eq_value(&before, &after),
        "an equal plist spine must replace the previous cons"
    );
}

/// A put that changes the property value journals the string once.
///
/// Replacing the value uses `set_car` on the existing plist value cell, so
/// that cons records its own write first. The string must still appear once.
#[test]
fn changed_put_of_a_different_value_journals_exactly_once() {
    let value = fresh_string(b"abcd");
    put_face(value, TaggedValue::symbol("bold"));
    let value_cell = crate::value::get_string_text_properties_table_for_value(value)
        .expect("table")
        .interval_plist_at_char_pos(crate::value::positions::CharPos0::ZERO)
        .expect("plist")
        .cons_cdr();
    let start = LispCollectionRevision::current();

    put_face(value, TaggedValue::symbol("italic"));

    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        vec![value_cell.bits(), value.bits()],
        "changing a property value journals the plist cell and the string once each"
    );
}

/// A remove that strips a present name records one collection write.
#[test]
fn changed_remove_of_a_present_property_journals_exactly_once() {
    let value = fresh_string(b"abcd");
    put_face(value, TaggedValue::symbol("bold"));
    let start = LispCollectionRevision::current();
    let names = [TaggedValue::symbol("face")];

    let removed = crate::value::mutate_string_text_properties(value, |table| {
        table.remove_properties_in_char_range(whole_range(value), &names)
    })
    .expect("string");

    assert!(removed, "face was on the string");
    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        vec![value.bits()],
        "removing a present property records one collection write"
    );
}

/// `set-text-properties` of a non-nil plist installs new conses. It does not
/// `set_car` an existing cell, so the journal is the string alone.
#[test]
fn set_of_a_non_nil_plist_journals_the_string_once() {
    let value = fresh_string(b"abcd");
    let start = LispCollectionRevision::current();
    let len = char_len(value);

    crate::value::mutate_string_text_properties(value, |table| {
        table.set_properties_for_object_char_len(
            whole_range(value),
            crate::value::positions::CharLen::new(len),
            vec![(TaggedValue::symbol("face"), TaggedValue::symbol("bold"))],
        );
    })
    .expect("string");

    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        vec![value.bits()],
        "set-text-properties of a non-nil plist journals the string once"
    );
}

/// `add-text-properties` of a name the interval does not hold prepends a cons.
/// That path does not `set_car`, so the journal is the string alone.
#[test]
fn add_of_a_new_property_name_journals_the_string_once() {
    let value = fresh_string(b"abcd");
    put_face(value, TaggedValue::symbol("bold"));
    let start = LispCollectionRevision::current();
    let len = char_len(value);
    let properties = [(TaggedValue::symbol("display"), TaggedValue::symbol("raise"))];

    let changed = crate::value::mutate_string_text_properties(value, |table| {
        table.apply_property_plist_for_object_char_len(
            whole_range(value),
            crate::value::positions::CharLen::new(len),
            &properties,
            crate::value::text_props::PropertyPlistApplication::AddProperties,
        )
    })
    .expect("string");

    assert!(changed, "a new name is added");
    assert_eq!(
        journal_since_for_test(start).expect("journal window"),
        vec![value.bits()],
        "adding a new property name journals the string once"
    );
}
