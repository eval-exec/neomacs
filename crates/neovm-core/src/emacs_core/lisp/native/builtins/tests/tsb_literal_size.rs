//! GNU literal SIZE is ignored; the allocation request derives from DATA pairs.
//! Oracle fixtures are refreshed standalone with UPDATE_EXPECT=1 through the
//! campaign sandbox, then read here without launching nested GNU scopes.
use super::symbols::builtin_make_byte_code;
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use std::marker::PhantomData;
use std::rc::Rc;

/// Scratch roots are thread-local; restore them even when a red assertion unwinds.
#[must_use]
#[derive(Debug)]
struct TestRootScope {
    saved: usize,
    _thread_bound: PhantomData<Rc<()>>,
}

impl TestRootScope {
    fn new() -> Self {
        Self {
            saved: save_scratch_gc_roots(),
            _thread_bound: PhantomData,
        }
    }
}

impl Drop for TestRootScope {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.saved);
    }
}

static_assertions::assert_not_impl_any!(TestRootScope: Send, Sync);
use crate::emacs_core::value::{HashTableTest, Value};
use crate::heap_types::LispString;

fn converted_literal(context: &mut Context, size: Value, data: Value) -> Value {
    // GNU lread.c:3155–3200 constructs #s hash tables while reading. Ordinary
    // list constants stay data in Fmake_byte_code (alloc.c:3557–3571).
    let text = format!(
        "#s(hash-table size {} test eq data {})",
        crate::emacs_core::print::print_value(&size),
        crate::emacs_core::print::print_value(&data),
    );
    let constant =
        crate::emacs_core::reader::builtin_read_from_string(context, vec![Value::string(text)])
            .expect("GNU hash-table reader literal")
            .cons_car();
    push_scratch_gc_root(constant);
    let function = builtin_make_byte_code(vec![
        Value::fixnum(0),
        Value::heap_string(LispString::from_unibyte(vec![0xc0, 0x87])),
        Value::vector(vec![constant]),
        Value::fixnum(1),
    ])
    .expect("constant-return byte-code construction");
    push_scratch_gc_root(function);
    let stored = function.get_bytecode_data().unwrap().constants[0];
    assert_eq!(stored, constant, "constructor preserves the reader object");
    stored
}

fn summary(value: Value) -> String {
    assert!(
        value.is_hash_table(),
        "ignored SIZE must not abort conversion"
    );
    let table = value.as_hash_table().unwrap();
    assert!(matches!(table.test, HashTableTest::Eq));
    let get = |name| {
        let key = Value::symbol(name).to_hash_key(&table.test);
        table.data.get(&key).copied().unwrap_or(Value::NIL)
    };
    crate::emacs_core::print::print_value(&Value::list(vec![
        Value::T,
        Value::make_int(table.data.len() as i64),
        get("a"),
        get("b"),
        Value::symbol("eq"),
        Value::make_int(table.size),
    ]))
}

fn two_pairs() -> Value {
    Value::list(vec![
        Value::symbol("a"),
        Value::fixnum(1),
        Value::symbol("b"),
        Value::fixnum(2),
    ])
}

#[test]
fn compiled_hash_literal_ignores_size_metadata_like_gnu() {
    let _roots = TestRootScope::new();
    let mut context = Context::new();
    let expected = include_str!("tsb_literal_size/metadata.expect");
    let sizes = [
        Value::fixnum(3),
        Value::fixnum(-1),
        Value::make_integer_from_str_or_zero("1267650600228229401496703205376"),
        Value::make_float(1.5),
        Value::symbol("ignored"),
        Value::string("ignored"),
        Value::NIL,
    ];
    let expected = expected.lines().collect::<Vec<_>>();
    assert_eq!(
        sizes.len(),
        expected.len(),
        "GNU fixture covers each SIZE shape"
    );
    for (size, expected) in sizes.into_iter().zip(expected) {
        assert_eq!(
            summary(converted_literal(&mut context, size, two_pairs())),
            expected
        );
    }
}

#[test]
fn compiled_hash_literal_huge_size_does_not_control_allocation() {
    let _roots = TestRootScope::new();
    let mut context = Context::new();
    let expected = include_str!("tsb_literal_size/huge.expect").trim();
    let table = converted_literal(
        &mut context,
        Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
        two_pairs(),
    );
    assert_eq!(summary(table), expected);
}
