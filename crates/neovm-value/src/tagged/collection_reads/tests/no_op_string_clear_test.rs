//! An interval-free string clear must not invent a collection mutation.
use super::*;
use crate::tagged::gc::TestHeap;
use crate::value::{
    StringTextPropertyRun, Value, clear_string_text_properties_for_value, eq_value,
    get_string_text_properties_table_for_value, string_has_text_property_interval_tree,
};

fn absent_intervals_clear_preserves_observed_string(text: &str) {
    let _heap = TestHeap::new();
    let string = Value::string(text);
    if text.is_empty() {
        assert!(
            eq_value(&string, &Value::string("")),
            "the empty case must exercise the actual canonical string"
        );
    }
    assert!(!string_has_text_property_interval_tree(string));
    let ((bytes, properties), reads) = capture(|| {
        (
            string.as_str_owned().expect("actual string bytes"),
            get_string_text_properties_table_for_value(string),
        )
    });
    let reads = reads.expect("actual bytes/property-read certificate");
    assert_eq!(bytes, text);
    assert!(properties.is_none());
    assert!(reads.contains_owner_for_test(string.bits()));
    assert!(
        reads.unchanged_and_observe(),
        "warm the original certificate"
    );

    for _ in 0..6 {
        clear_string_text_properties_for_value(string);
    }

    assert_eq!(string.as_str_owned().as_deref(), Some(text));
    assert!(!string_has_text_property_interval_tree(string));
    assert!(get_string_text_properties_table_for_value(string).is_none());
    assert!(
        reads.unchanged(),
        "clearing already-absent intervals changed no observed string state"
    );
    assert!(
        reads.unchanged_and_observe(),
        "the original string certificate must remain reusable in its World"
    );
}

#[test]
fn clearing_absent_intervals_preserves_canonical_empty_string_certificate() {
    absent_intervals_clear_preserves_observed_string("");
}

#[test]
fn clearing_absent_intervals_preserves_ordinary_string_certificate() {
    absent_intervals_clear_preserves_observed_string("ordinary interval-free text");
}

#[test]
fn clearing_present_intervals_removes_properties_and_invalidates_certificate() {
    let _heap = TestHeap::new();
    let string = Value::string_with_text_properties(
        "abc",
        vec![StringTextPropertyRun {
            start: 0,
            end: 3,
            plist: Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
        }],
    );
    assert!(string_has_text_property_interval_tree(string));
    let ((bytes, properties), reads) = capture(|| {
        (
            string.as_str_owned().expect("actual string bytes"),
            get_string_text_properties_table_for_value(string),
        )
    });
    assert_eq!(bytes, "abc");
    assert!(properties.is_some_and(|properties| !properties.is_empty()));
    let reads = reads.expect("real propertized-string certificate");
    assert!(reads.contains_owner_for_test(string.bits()));
    assert!(reads.unchanged_and_observe());

    clear_string_text_properties_for_value(string);

    assert_eq!(string.as_str_owned().as_deref(), Some("abc"));
    assert!(!string_has_text_property_interval_tree(string));
    assert!(get_string_text_properties_table_for_value(string).is_none());
    assert!(!reads.unchanged());
    assert!(!reads.unchanged_and_observe());
}

#[test]
fn editing_string_bytes_still_invalidates_interval_free_string_certificate() {
    let _heap = TestHeap::new();
    let string = Value::string("abc");
    let (_, reads) = capture(|| {
        assert_eq!(string.as_str_owned().as_deref(), Some("abc"));
        assert!(get_string_text_properties_table_for_value(string).is_none());
    });
    let reads = reads.expect("real interval-free byte-read certificate");
    assert!(reads.contains_owner_for_test(string.bits()));
    assert!(reads.unchanged_and_observe());

    assert!(string.set_string_byte_same_char_count(0, b'z'));

    assert_eq!(string.as_str_owned().as_deref(), Some("zbc"));
    assert!(!reads.unchanged());
    assert!(!reads.unchanged_and_observe());
}

#[test]
fn clearing_an_attached_empty_interval_table_still_invalidates_certificate() {
    let _heap = TestHeap::new();
    let string = Value::string("abc");
    crate::tagged::mutate::with_string_text_props_mut(string, |_| ())
        .expect("attach an actual empty interval table");
    assert!(string_has_text_property_interval_tree(string));
    let (_, reads) = capture(|| {
        assert!(string_has_text_property_interval_tree(string));
        assert!(get_string_text_properties_table_for_value(string).is_none());
    });
    let reads = reads.expect("attached interval-table certificate");
    assert!(reads.unchanged_and_observe());

    clear_string_text_properties_for_value(string);

    assert!(!string_has_text_property_interval_tree(string));
    assert!(get_string_text_properties_table_for_value(string).is_none());
    assert!(!reads.unchanged());
}
