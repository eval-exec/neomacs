//! Integration test demonstrating how code that uses the old Value enum
//! translates to the new TaggedValue system.
//!
//! Each test shows the OLD pattern (commented) and the NEW pattern.

use super::gc::TaggedHeap;
use super::header::VecLikeType;
use super::value::{TaggedValue, ValueKind};
use crate::emacs_core::intern::SymId;

/// Helper: create a proper list (a b c ... nil) from a slice of values.
fn make_list(heap: &mut TaggedHeap, items: &[TaggedValue]) -> TaggedValue {
    let mut result = TaggedValue::NIL;
    for item in items.iter().rev() {
        result = heap.alloc_cons(*item, result);
    }
    result
}

/// Helper: collect a proper list into a Vec (like old list_to_vec).
fn list_to_vec(value: TaggedValue) -> Option<Vec<TaggedValue>> {
    let mut result = Vec::new();
    let mut cursor = value;
    loop {
        if cursor.is_nil() {
            return Some(result);
        }
        if !cursor.is_cons() {
            return None; // improper list
        }
        result.push(cursor.cons_car());
        cursor = cursor.cons_cdr();
    }
}

/// Helper: list length
fn list_length(value: TaggedValue) -> Option<usize> {
    list_to_vec(value).map(|v| v.len())
}

// -----------------------------------------------------------------------
// Test: basic cons list operations
// -----------------------------------------------------------------------

#[test]
fn test_list_operations() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    // OLD: Value::list(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
    // NEW:
    let list = make_list(
        &mut heap,
        &[
            TaggedValue::fixnum(1),
            TaggedValue::fixnum(2),
            TaggedValue::fixnum(3),
        ],
    );

    // OLD: list_to_vec(&list)
    // NEW: same logic, direct pointer access
    let items = list_to_vec(list).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].as_fixnum(), Some(1));
    assert_eq!(items[1].as_fixnum(), Some(2));
    assert_eq!(items[2].as_fixnum(), Some(3));

    // OLD: list_length(&list)
    // NEW:
    assert_eq!(list_length(list), Some(3));
}

// -----------------------------------------------------------------------
// Test: type dispatch (replacing match on Value enum)
// -----------------------------------------------------------------------

#[test]
fn test_type_dispatch() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    let values = vec![
        TaggedValue::NIL,
        TaggedValue::T,
        TaggedValue::fixnum(42),
        TaggedValue::char('A'),
        heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL),
        heap.alloc_float(3.125),
        heap.alloc_vector(vec![TaggedValue::fixnum(10)]),
    ];

    // OLD:
    // match value {
    //     Value::NIL => "nil",
    //     Value::T => "t",
    //     Value::Int(n) => format!("int:{}", n),
    //     Value::Char(c) => format!("char:{}", c),
    //     Value::Cons(_) => "cons",
    //     Value::Float(f, _) => format!("float:{}", f),
    //     Value::Vector(_) => "vector",
    //     _ => "other",
    // }
    //
    // NEW:
    let mut results = Vec::new();
    for value in &values {
        let desc = match value.kind() {
            ValueKind::Nil => "nil".to_string(),
            ValueKind::T => "t".to_string(),
            ValueKind::Fixnum(n) => format!("int:{}", n),
            ValueKind::Cons => "cons".to_string(),
            ValueKind::Float => format!("float:{}", value.xfloat()),
            ValueKind::Veclike(VecLikeType::Vector) => "vector".to_string(),
            ValueKind::String => "string".to_string(),
            ValueKind::Symbol(_) => "symbol".to_string(),
            ValueKind::Subr(_) => "subr".to_string(),
            ValueKind::Veclike(VecLikeType::Subr) => "subr".to_string(),
            ValueKind::Veclike(_) => "veclike".to_string(),
            ValueKind::Unbound => "unbound".to_string(),
            ValueKind::Unknown => "unknown".to_string(),
        };
        results.push(desc);
    }

    assert_eq!(results[0], "nil");
    assert_eq!(results[1], "t");
    assert_eq!(results[2], "int:42");
    assert_eq!(results[3], "int:65");
    assert_eq!(results[4], "cons");
    assert_eq!(results[5], "float:3.125");
    assert_eq!(results[6], "vector");
}

// -----------------------------------------------------------------------
// Test: eq_value equivalent (pointer/bit equality)
// -----------------------------------------------------------------------

#[test]
fn test_eq_semantics() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    // Fixnum eq: same value = eq
    let a = TaggedValue::fixnum(42);
    let b = TaggedValue::fixnum(42);
    assert_eq!(a, b); // eq for fixnums

    // Symbol eq: same SymId = eq
    let s1 = TaggedValue::from_sym_id(SymId(5));
    let s2 = TaggedValue::from_sym_id(SymId(5));
    assert_eq!(s1, s2);

    // Cons eq: same pointer = eq, different pointer = not eq
    let c1 = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    let c2 = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    assert_ne!(c1.bits(), c2.bits()); // Different allocations → different pointers → not eq

    // But same value copied = eq (same pointer)
    let c3 = c1;
    assert_eq!(c1.bits(), c3.bits());

    // nil eq nil
    assert_eq!(TaggedValue::NIL.bits(), TaggedValue::NIL.bits());

    // Float eq: different allocations = not eq (pointer identity)
    let f1 = heap.alloc_float(3.125);
    let f2 = heap.alloc_float(3.125);
    assert_ne!(f1.bits(), f2.bits()); // Different allocations
}

// -----------------------------------------------------------------------
// Test: is_* predicate patterns
// -----------------------------------------------------------------------

#[test]
fn test_predicate_patterns() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    let val = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);

    // OLD: matches!(val, Value::Cons(_))
    // NEW:
    assert!(val.is_cons());

    // OLD: if let Value::Int(n) = val { ... }
    // NEW:
    let n_val = TaggedValue::fixnum(99);
    if let Some(n) = n_val.as_fixnum() {
        assert_eq!(n, 99);
    } else {
        panic!("expected fixnum");
    }

    // OLD: !val.is_nil()
    // NEW: same!
    assert!(val.is_truthy());
    assert!(!TaggedValue::NIL.is_truthy());
}

// -----------------------------------------------------------------------
// Test: cons cell mutation
// -----------------------------------------------------------------------

#[test]
fn test_cons_mutation() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    // OLD: let cons = Value::cons(Value::Int(1), Value::Int(2));
    // NEW:
    let cons = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::fixnum(2));

    // Cons mutation now goes through the tagged mutation helpers directly.
    // NEW: direct mutation
    cons.set_car(TaggedValue::fixnum(10));
    cons.set_cdr(TaggedValue::fixnum(20));

    assert_eq!(cons.cons_car().as_fixnum(), Some(10));
    assert_eq!(cons.cons_cdr().as_fixnum(), Some(20));
}

// -----------------------------------------------------------------------
// Test: alist lookup (common Elisp pattern)
// -----------------------------------------------------------------------

#[test]
fn test_alist_lookup() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    // Build alist: ((a . 1) (b . 2) (c . 3))
    let pair_a = heap.alloc_cons(TaggedValue::from_sym_id(SymId(10)), TaggedValue::fixnum(1));
    let pair_b = heap.alloc_cons(TaggedValue::from_sym_id(SymId(11)), TaggedValue::fixnum(2));
    let pair_c = heap.alloc_cons(TaggedValue::from_sym_id(SymId(12)), TaggedValue::fixnum(3));
    let alist = make_list(&mut heap, &[pair_a, pair_b, pair_c]);

    // assq-like lookup
    fn assq(alist: TaggedValue, key: TaggedValue) -> Option<TaggedValue> {
        let mut cursor = alist;
        while cursor.is_cons() {
            let entry = cursor.cons_car();
            if entry.is_cons() && entry.cons_car() == key {
                return Some(entry);
            }
            cursor = cursor.cons_cdr();
        }
        None
    }

    let target = TaggedValue::from_sym_id(SymId(11));
    let found = assq(alist, target).unwrap();
    assert_eq!(found.cons_cdr().as_fixnum(), Some(2));

    // Not found
    let missing = assq(alist, TaggedValue::from_sym_id(SymId(99)));
    assert!(missing.is_none());
}

// -----------------------------------------------------------------------
// Test: GC with mixed types
// -----------------------------------------------------------------------

#[test]
fn test_gc_mixed_types() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    // Allocate various types
    let root_cons = heap.alloc_cons(TaggedValue::fixnum(1), TaggedValue::NIL);
    let _garbage_cons = heap.alloc_cons(TaggedValue::fixnum(999), TaggedValue::NIL);
    let root_float = heap.alloc_float(2.625);
    let _garbage_float = heap.alloc_float(0.0);

    assert_eq!(heap.allocated_count, 4);

    // Collect with only root_cons and root_float as roots
    heap.collect(vec![root_cons, root_float].into_iter());

    assert_eq!(heap.allocated_count, 2);
    assert_eq!(root_cons.cons_car().as_fixnum(), Some(1));
    assert!((root_float.xfloat() - 2.625).abs() < f64::EPSILON);
}

// -----------------------------------------------------------------------
// Test: value_to_expr equivalent (for display/debugging)
// -----------------------------------------------------------------------

#[test]
fn test_value_description() {
    crate::test_utils::init_test_tracing();
    let mut heap = TaggedHeap::new();

    // Show that we can describe any value without the old match-on-enum pattern
    fn describe(val: TaggedValue) -> String {
        match val.kind() {
            ValueKind::Nil => "nil".into(),
            ValueKind::T => "t".into(),
            ValueKind::Fixnum(n) => n.to_string(),
            ValueKind::Float => format!("{}", val.xfloat()),
            ValueKind::Symbol(id) => format!("sym#{}", id.0),
            ValueKind::Subr(id) => format!("#<subr#{}>", id.0),
            ValueKind::Veclike(VecLikeType::Subr) => {
                format!("#<subr#{}>", val.as_subr_id().unwrap().0)
            }
            ValueKind::Cons => {
                let car = describe(val.cons_car());
                let cdr = val.cons_cdr();
                if cdr.is_nil() {
                    format!("({})", car)
                } else {
                    format!("({} . ...)", car)
                }
            }
            ValueKind::String => {
                if let Some(s) = val.as_utf8_str() {
                    format!("\"{}\"", s)
                } else {
                    "\"...\"".into()
                }
            }
            ValueKind::Veclike(ty) => format!("#<{:?}>", ty),
            ValueKind::Unbound => "#<unbound>".into(),
            ValueKind::Unknown => "#<unknown>".into(),
        }
    }

    assert_eq!(describe(TaggedValue::NIL), "nil");
    assert_eq!(describe(TaggedValue::fixnum(42)), "42");

    let cdr = heap.alloc_cons(TaggedValue::fixnum(2), TaggedValue::NIL);
    let list = heap.alloc_cons(TaggedValue::fixnum(1), cdr);
    assert_eq!(describe(list), "(1 . ...)");
}
