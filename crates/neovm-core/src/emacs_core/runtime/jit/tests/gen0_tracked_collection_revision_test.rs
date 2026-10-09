//! Explicit write tracking retains interpreter journaling for unobserved GEN0
//! owners. Certificates remain mutator-local; roots retain their storage.

use super::*;
use crate::tagged::collection_reads::{
    CompiledJournalMode, capture, force_compiled_journal_for_test, is_observed,
};
use crate::tagged::gc::{HeapWriteKind, WriteTrackingMode};
use crate::tagged::mutate::LispCollectionRevision;

struct ObservedMode;

impl ObservedMode {
    fn enter() -> Self {
        force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
        Self
    }
}

impl Drop for ObservedMode {
    fn drop(&mut self) {
        force_compiled_journal_for_test(None);
    }
}

fn context() -> Context {
    struct RestoreGeneration(Option<std::ffi::OsString>);
    impl Drop for RestoreGeneration {
        fn drop(&mut self) {
            unsafe {
                match self.0.take() {
                    Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
                    None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
                }
            }
        }
    }
    let _restore = RestoreGeneration(std::env::var_os("NEOVM_GC_GENERATIONAL"));
    // Each nextest case runs in its own process. The heap's generation mode
    // is immutable once constructed, and the environment is restored here.
    unsafe { std::env::remove_var("NEOVM_GC_GENERATIONAL") };
    let mut context = Context::new();
    context.gc_stress = false;
    context.tagged_heap.set_gc_threshold(usize::MAX);
    assert!(!context.tagged_heap.generational_enabled());
    // Even WRITE_LAZY keeps history after a capture. This reads no owner.
    assert!(capture(|| ()).1.is_some());
    context
}

#[derive(Clone, Copy, Debug)]
enum Store {
    Car,
    Cdr,
    Vector,
    Record,
}

impl Store {
    fn allocate(self, context: &mut Context) -> Value {
        match self {
            Self::Car | Self::Cdr => context.tagged_heap.alloc_cons(Value::NIL, Value::NIL),
            Self::Vector => context.tagged_heap.alloc_vector(vec![Value::NIL]),
            Self::Record => context
                .tagged_heap
                .alloc_record(vec![Value::symbol("tracked-journal"), Value::NIL]),
        }
    }

    fn compile(self) -> CompiledLeaf {
        match self {
            Self::Car | Self::Cdr => lower_leaf(
                &[
                    Op::StackRef(1),
                    Op::StackRef(1),
                    if matches!(self, Self::Car) {
                        Op::Setcar
                    } else {
                        Op::Setcdr
                    },
                    Op::Return,
                ],
                &[],
                2,
            ),
            Self::Vector | Self::Record => lower_leaf(
                &[
                    Op::StackRef(2),
                    Op::StackRef(2),
                    Op::StackRef(2),
                    Op::Aset,
                    Op::Return,
                ],
                &[],
                3,
            ),
        }
        .expect("setter compiles")
    }

    fn slot(self) -> usize {
        usize::from(matches!(self, Self::Cdr | Self::Record))
    }

    fn kind(self) -> HeapWriteKind {
        match self {
            Self::Car => HeapWriteKind::ConsCar,
            Self::Cdr => HeapWriteKind::ConsCdr,
            Self::Vector => HeapWriteKind::VectorSlot,
            Self::Record => HeapWriteKind::RecordSlot,
        }
    }

    fn read(self, owner: Value) -> Value {
        match self {
            Self::Car => owner.cons_car(),
            Self::Cdr => owner.cons_cdr(),
            Self::Vector => owner.as_vector_data().expect("vector")[0],
            Self::Record => owner.as_record_data().expect("record")[1],
        }
    }

    fn native(self, context: &mut Context, leaf: &CompiledLeaf, owner: Value, value: Value) {
        let args = match self {
            Self::Car | Self::Cdr => vec![owner, value],
            Self::Vector | Self::Record => {
                vec![owner, Value::make_int(self.slot() as i64), value]
            }
        };
        match leaf.call(context as *mut Context as *mut u8, &args) {
            NativeRun::Ok(bits) => assert_eq!(bits, value.bits()),
            other => panic!("tracked setter must complete natively: {other:?}"),
        }
    }

    fn interpreted(self, owner: Value, value: Value) -> bool {
        match self {
            Self::Car => crate::tagged::mutate::set_cons_car(owner, value),
            Self::Cdr => crate::tagged::mutate::set_cons_cdr(owner, value),
            Self::Vector => crate::tagged::mutate::set_vector_slot(owner, 0, value),
            Self::Record => crate::tagged::mutate::set_record_slot(owner, 1, value),
        }
    }

    fn assert_write(self, context: &Context, owner: Value, value: Value) {
        assert!(context.tagged_heap.is_dirty_owner(owner));
        let writes = context.tagged_heap.dirty_writes();
        assert_eq!(writes.len(), 1, "one selected-field barrier: {self:?}");
        assert_eq!(writes[0].owner.bits(), owner.bits());
        assert_eq!(writes[0].kind, self.kind());
        assert_eq!(writes[0].slot, Some(self.slot()));
        assert_eq!(writes[0].value.map(Value::bits), Some(value.bits()));
    }
}

fn check_tracking(store: Store) {
    let mut context = context();
    context
        .tagged_heap
        .set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
    let leaf = store.compile();
    let owner = store.allocate(&mut context);
    context.push_specpdl_root(owner);
    assert!(!is_observed(owner.bits()), "fresh tracked owner: {store:?}");
    context.tagged_heap.clear_dirty_owners();
    context.tagged_heap.clear_dirty_writes();
    let first = Value::make_int(51);
    let before = LispCollectionRevision::current();
    store.native(&mut context, &leaf, owner, first);
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        1,
        "explicit tracking journals an unobserved native owner once: {store:?}"
    );
    store.assert_write(&context, owner, first);
    assert!(
        !is_observed(owner.bits()),
        "an inactive setter is not a read"
    );
    let (value, reads) = capture(|| store.read(owner));
    assert_eq!(value.bits(), first.bits());
    let reads = reads.expect("read after the tracked store is coherent");
    assert!(reads.unchanged());
    context.tagged_heap.clear_dirty_writes();
    let second = Value::make_int(52);
    let before = LispCollectionRevision::current();
    store.native(&mut context, &leaf, owner, second);
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        1
    );
    store.assert_write(&context, owner, second);
    assert!(!reads.unchanged(), "the next store invalidates the read");

    // The interpreter journals before observing the setter's pointer. A
    // setter-only scope must therefore retain its dependency at the new
    // revision, even if this new owner had no preceding collection read.
    let projected_owner = store.allocate(&mut context);
    context.push_specpdl_root(projected_owner);
    assert!(!is_observed(projected_owner.bits()));
    context.tagged_heap.clear_dirty_writes();
    let before = LispCollectionRevision::current();
    let (_, projected) = capture(|| store.native(&mut context, &leaf, projected_owner, first));
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(before),
        1
    );
    store.assert_write(&context, projected_owner, first);
    assert!(is_observed(projected_owner.bits()));
    let projected = projected.expect("setter-only projected read is coherent");
    assert!(projected.unchanged());
    assert!(store.interpreted(projected_owner, second));
    assert!(
        !projected.unchanged(),
        "setter projection retains its owner"
    );
}

#[test]
fn gen0_observed_explicit_tracking_journals_unobserved_cons_setters() {
    let _mode = ObservedMode::enter();
    check_tracking(Store::Car);
    check_tracking(Store::Cdr);
}

#[test]
fn gen0_observed_explicit_tracking_journals_unobserved_owned_arrays() {
    let _mode = ObservedMode::enter();
    check_tracking(Store::Vector);
    check_tracking(Store::Record);
}
