//! GNU charset-pruning interval and source ownership invariants.
use super::super::*;

#[test]
fn charset_policy_pruning_preserves_interval_boundaries() {
    let foo = Value::symbol("foo");
    let bar = Value::symbol("bar");
    let text = Value::string_with_text_properties(
        "abc",
        vec![
            StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![
                    Value::symbol("charset"),
                    Value::symbol("ascii"),
                    foo,
                    bar,
                ]),
            },
            StringTextPropertyRun {
                start: 1,
                end: 3,
                plist: Value::list(vec![
                    Value::symbol("charset"),
                    Value::symbol("unicode"),
                    foo,
                    bar,
                ]),
            },
        ],
    );
    let original = get_string_text_properties_for_value(text).expect("input intervals");
    let pruned =
        printed_string_property_runs(text, CharsetTextProperty::Never).expect("retained foo");
    assert_eq!(pruned.len(), 2);
    assert_eq!((pruned[0].start, pruned[0].end), (0, 1));
    assert_eq!((pruned[1].start, pruned[1].end), (1, 3));
    assert_eq!(list_to_vec(&pruned[0].plist), Some(vec![foo, bar]));
    let after = get_string_text_properties_for_value(text).expect("input unchanged");
    assert_eq!(after.len(), original.len());
    for (before, after) in original.iter().zip(after) {
        assert!(before.plist.eq(&after.plist));
    }
}

#[test]
fn charset_policy_pruning_preserves_distinct_equal_values() {
    crate::test_utils::init_test_tracing();
    let foo = Value::symbol("foo");
    let left = Value::string("x");
    let right = Value::string("x");
    let text = Value::string_with_text_properties(
        "ab",
        vec![
            StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(vec![
                    Value::symbol("charset"),
                    Value::symbol("ascii"),
                    foo,
                    left,
                ]),
            },
            StringTextPropertyRun {
                start: 1,
                end: 2,
                plist: Value::list(vec![
                    Value::symbol("charset"),
                    Value::symbol("unicode"),
                    foo,
                    right,
                ]),
            },
        ],
    );
    let runs =
        printed_string_property_runs(text, CharsetTextProperty::Never).expect("foo retained");
    assert_eq!(runs.len(), 2);
    assert_eq!(list_to_vec(&runs[0].plist), Some(vec![foo, left]));
    assert_eq!(list_to_vec(&runs[1].plist), Some(vec![foo, right]));
}
