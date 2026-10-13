//! Local type and ownership invariants behind the GNU settings fixtures.
use super::super::*;
use crate::emacs_core::intern::{intern_uninterned, intern_uninterned_lisp_string};
static_assertions::assert_not_impl_any!(PrintEvaluationGuard<'static>: Send, Sync);
static_assertions::assert_not_impl_any!(PrintState<'static>: Send, Sync);

#[test]
fn charset_pruning_preserves_runs_without_mutating_input() {
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
fn symbol_reader_whitespace_distinguishes_nbsp_from_byte8() {
    let multibyte = Value::symbol(intern_uninterned("a\u{a0}b"));
    assert_eq!(print_value_bytes(&multibyte), b"a\\\xc2\xa0b");
    let raw = crate::heap_types::LispString::from_unibyte(vec![b'a', 0xa0, b'b']);
    let unibyte = Value::symbol(intern_uninterned_lisp_string(&raw));
    let mut expected = vec![b'a'];
    let mut encoded = [0; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let width = crate::emacs_core::emacs_char::char_string(
        crate::emacs_core::emacs_char::unibyte_to_char(0xa0),
        &mut encoded,
    );
    expected.extend_from_slice(&encoded[..width]);
    expected.push(b'b');
    assert_eq!(print_value_bytes(&unibyte), expected);
}
#[test]
fn charset_pruning_keeps_distinct_equal_property_values() {
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
