//! The folded string store gate follows the live owner's storage transitions.
//! Certificates and journals remain local to this mutator; all owners are rooted.

use super::*;
use crate::heap_types::LispString;
use crate::tagged::collection_reads::{
    CompiledJournalMode, capture, force_compiled_journal_for_test,
};
use crate::tagged::mutate::{LispCollectionRevision, with_lisp_string_mut};

struct ObservedMode;
impl ObservedMode {
    fn new() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}
impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn fixture(storage: LispString) -> (Context, Value, CompiledLeaf) {
    let mut context = context(false);
    let leaf = lower_leaf(
        &[
            Op::StackRef(2),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Aset,
            Op::Return,
        ],
        &[],
        3,
    )
    .expect("string setter compiles");
    let warm = context
        .tagged_heap
        .alloc_string(LispString::from_unibyte(b"abc".to_vec()));
    context.push_specpdl_root(warm);
    for _ in 0..2 {
        native(
            &mut context,
            &leaf,
            &[warm, Value::make_int(0), Value::make_int(64)],
        );
    }
    let owner = context.tagged_heap.alloc_string(storage);
    context.push_specpdl_root(owner);
    (context, owner, leaf)
}

fn certify_then_store(context: &mut Context, owner: Value, leaf: &CompiledLeaf) {
    let (_, reads) = capture(|| owner.as_str_owned().expect("live string"));
    let reads = reads.expect("string read certifies");
    let before = LispCollectionRevision::current();
    let shims = super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get());
    assert_eq!(
        native(
            context,
            leaf,
            &[owner, Value::make_int(0), Value::make_int(67)]
        ),
        Value::make_int(67)
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        1
    );
    assert_eq!(
        super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get()),
        shims + 1
    );
    assert!(
        !reads.unchanged(),
        "the folded gate must journal this observed owner"
    );
    assert_eq!(owner.as_str_owned().expect("string").as_bytes()[0], b'C');
}

#[test]
fn observed_string_borrowed_byte_materialization_keeps_the_native_gate() {
    let _mode = ObservedMode::new();
    let (mut context, owner, leaf) = fixture(LispString::from_rodata_unibyte(b"abc\0"));
    let (_, reads) = capture(|| owner.as_str_owned().expect("borrowed string"));
    crate::emacs_core::builtins::builtin_aset_args(&[
        owner,
        Value::make_int(0),
        Value::make_int(66),
    ])
    .expect("interpreted materialization");
    assert!(!reads.expect("borrowed read").unchanged());
    certify_then_store(&mut context, owner, &leaf);
}

#[test]
fn observed_string_borrowed_closure_materialization_keeps_the_native_gate() {
    let _mode = ObservedMode::new();
    let (mut context, owner, leaf) = fixture(LispString::from_rodata_unibyte(b"abc\0"));
    let (_, reads) = capture(|| owner.as_str_owned().expect("borrowed string"));
    with_lisp_string_mut(owner, |storage| {
        storage.mutate_bytes(|bytes| bytes.push(b'd'))
    })
    .expect("live owner");
    assert!(!reads.expect("borrowed read").unchanged());
    certify_then_store(&mut context, owner, &leaf);
}

#[test]
fn observed_string_whole_payload_replacement_keeps_the_native_gate() {
    let _mode = ObservedMode::new();
    let (mut context, owner, leaf) = fixture(LispString::from_unibyte(b"abc".to_vec()));
    let (_, reads) = capture(|| owner.as_str_owned().expect("string"));
    with_lisp_string_mut(owner, |storage| {
        *storage = LispString::from_unibyte(b"pq".to_vec())
    })
    .expect("live owner");
    assert!(!reads.expect("old payload read").unchanged());
    certify_then_store(&mut context, owner, &leaf);
}

#[test]
fn observed_string_unwinding_replacement_keeps_the_native_gate() {
    let _mode = ObservedMode::new();
    let (mut context, owner, leaf) = fixture(LispString::from_unibyte(b"abc".to_vec()));
    let (_, reads) = capture(|| owner.as_str_owned().expect("string"));
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_lisp_string_mut(owner, |storage| {
            *storage = LispString::from_unibyte(b"pq".to_vec());
            panic!("unwind after installing a replacement");
        })
        .expect("live owner");
    }));
    assert!(unwind.is_err());
    assert!(!reads.expect("old payload read").unchanged());
    certify_then_store(&mut context, owner, &leaf);
}

#[test]
fn detached_observed_string_storage_is_unobserved_at_a_fresh_identity() {
    let _mode = ObservedMode::new();
    let (mut context, owner, leaf) = fixture(LispString::from_unibyte(b"abc".to_vec()));
    let (_, reads) = capture(|| owner.as_str_owned().expect("string"));
    let detached = with_lisp_string_mut(owner, |storage| {
        std::mem::replace(storage, LispString::from_unibyte(b"pq".to_vec()))
    })
    .expect("detach payload");
    assert!(!reads.expect("old read").unchanged());
    let fresh = context.tagged_heap.alloc_string(detached);
    context.push_specpdl_root(fresh);
    assert!(!crate::tagged::collection_reads::is_observed(fresh.bits()));
    let before = LispCollectionRevision::current();
    let shims = super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get());
    assert_eq!(
        native(
            &mut context,
            &leaf,
            &[fresh, Value::make_int(0), Value::make_int(68)]
        ),
        Value::make_int(68)
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        0
    );
    assert_eq!(
        super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get()),
        shims
    );
    certify_then_store(&mut context, owner, &leaf);
}
