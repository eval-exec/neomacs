//! Recursive custom-type descriptors are runtime roots, independently of the
//! variable's default value. These tests use explicit roots and no Lisp code.

use super::{CustomType, CustomVariable, GcTrace, LispString, ModeRegistry, Value};
use crate::emacs_core::value::eq_value;
use crate::tagged::gc::TaggedHeap;

fn choice(first: Value, second: Value) -> CustomType {
    CustomType::Choice(vec![
        ("first".to_owned(), first),
        ("second".to_owned(), second),
    ])
}

#[test]
fn nested_custom_type_choices_survive_full_gc() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();
    // Run this same regression with NEOVM_GC_GENERATIONAL=0 and =1. The knob
    // is read at construction; the test never mutates process environment.
    assert_eq!(
        heap.generational_enabled(),
        std::env::var("NEOVM_GC_GENERATIONAL").as_deref() == Ok("1")
    );

    let mut stored = Vec::new();
    let mut strings = Vec::new();
    for index in 0..6 {
        let text = heap.alloc_string(LispString::from_utf8(&format!("choice-{index}")));
        strings.push(text);
        stored.push(heap.alloc_cons(text, Value::fixnum(index)));
    }
    let dead_string = heap.alloc_string(LispString::from_utf8("unrooted-control"));
    let dead_cons = heap.alloc_cons(dead_string, Value::NIL);

    let mut registry = ModeRegistry::new();
    registry.register_custom_variable(
        "recursive-choice-root",
        CustomVariable {
            default_value: Value::NIL,
            doc: None,
            type_: CustomType::List(Box::new(CustomType::Alist(
                Box::new(choice(stored[0], stored[1])),
                Box::new(CustomType::Plist(
                    Box::new(choice(stored[2], stored[3])),
                    Box::new(CustomType::List(Box::new(choice(stored[4], stored[5])))),
                )),
            ))),
            group: None,
            set_function: None,
            get_function: None,
            tag: None,
        },
    );

    for _ in 0..3 {
        let mut roots = Vec::new();
        registry.trace_roots(&mut roots);
        heap.collect_exact(roots.iter().copied());

        // Test locals are not explicit roots. Check allocated ownership
        // before any payload read, so this fails safely on the old trace.
        assert!(!heap.owns_heap_value_for_test(dead_cons));
        assert!(!heap.owns_heap_value_for_test(dead_string));
        for (&value, &text) in stored.iter().zip(&strings) {
            assert!(heap.owns_heap_value_for_test(value));
            assert!(heap.owns_heap_value_for_test(text));
        }
        // Existing mode roots and the default value keep their positions;
        // choice payloads follow depth-first field and choice-vector order.
        assert_eq!(roots.len(), 3 + stored.len());
        assert_eq!(&roots[3..], stored.as_slice());

        let type_ = &registry
            .get_custom_variable("recursive-choice-root")
            .unwrap()
            .type_;
        let CustomType::List(alist) = type_ else {
            panic!("expected the stored list descriptor");
        };
        let CustomType::Alist(alist_key, plist) = alist.as_ref() else {
            panic!("expected the stored alist descriptor");
        };
        let CustomType::Plist(plist_key, list) = plist.as_ref() else {
            panic!("expected the stored plist descriptor");
        };
        let CustomType::List(list_element) = list.as_ref() else {
            panic!("expected the stored plist value descriptor");
        };
        for (pair, descriptor) in [
            alist_key.as_ref(),
            plist_key.as_ref(),
            list_element.as_ref(),
        ]
        .into_iter()
        .enumerate()
        {
            let CustomType::Choice(choices) = descriptor else {
                panic!("expected the stored choice descriptor");
            };
            assert_eq!(choices.len(), 2);
            for (offset, (_, value)) in choices.iter().enumerate() {
                let index = pair * 2 + offset;
                assert!(eq_value(value, &stored[index]));
                assert!(eq_value(&value.cons_car(), &strings[index]));
                assert_eq!(value.cons_cdr(), Value::fixnum(index as i64));
                assert_eq!(
                    value.cons_car().as_str_owned(),
                    Some(format!("choice-{index}"))
                );
            }
        }
    }
}
