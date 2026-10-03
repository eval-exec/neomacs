//! Native generational stores retain the same collection-read dependencies
//! as the mutation helpers used by the interpreter.

use super::*;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

#[test]
fn generational_inline_cons_stores_invalidate_collection_read_certificates() {
    let mut context = context(true);
    let roots = context.save_specpdl_roots();
    let owner = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    context.push_specpdl_root(owner);
    context.push_specpdl_root(unrelated);
    for op in [Op::Setcar, Op::Setcdr] {
        let car = matches!(op, Op::Setcar);
        let leaf = compile_bytecode_function(&store_function(op)).expect("store compiles");
        let (_, reads) = capture(|| {
            if car {
                owner.cons_car()
            } else {
                owner.cons_cdr()
            }
        });
        let reads = reads.expect("coherent read");
        let before = cons_shims();
        native(&mut context, &leaf, &[unrelated, Value::make_int(7)]);
        assert!(
            reads.unchanged(),
            "an unrelated native store preserves reuse"
        );
        let revision = LispCollectionRevision::current();
        native(&mut context, &leaf, &[owner, Value::make_int(8)]);
        assert_ne!(LispCollectionRevision::current(), revision);
        assert!(!reads.unchanged(), "the observed native owner changed");
        assert_eq!(
            cons_shims(),
            before,
            "the journal does not outline the store"
        );

        let (_, reads) = capture(|| {
            if car {
                owner.cons_car()
            } else {
                owner.cons_cdr()
            }
        });
        if car {
            owner.set_car(Value::NIL);
        } else {
            owner.set_cdr(Value::NIL);
        }
        assert!(!reads.expect("interpreter read").unchanged());

        // Like an interpreter setter, the store's projected owner becomes a
        // dependency even when this capture had not previously read it.
        let (_, writes) = capture(|| native(&mut context, &leaf, &[owner, Value::make_int(9)]));
        let writes = writes.expect("the setter projection observes its new revision");
        owner.set_car(Value::T);
        assert!(
            !writes.unchanged(),
            "the native setter's projection is retained"
        );
    }
    context.restore_specpdl_roots(roots);
}

#[test]
fn generational_constant_fixnum_store_keeps_collection_history() {
    let mut context = context(true);
    let roots = context.save_specpdl_roots();
    let owner = Value::cons(Value::NIL, Value::NIL);
    context.push_specpdl_root(owner);
    let leaf = lower_leaf(
        &[Op::StackRef(0), Op::Constant(0), Op::Setcar, Op::Return],
        &[Value::make_int(12)],
        1,
    )
    .expect("constant store compiles");
    let (_, reads) = capture(|| owner.cons_car());
    let before = cons_shims();
    assert_eq!(native(&mut context, &leaf, &[owner]), Value::make_int(12));
    assert_eq!(cons_shims(), before);
    assert!(!reads.expect("coherent read").unchanged());
    context.restore_specpdl_roots(roots);
}

#[test]
fn generational_inline_aset_invalidates_vector_and_record_certificates() {
    let mut context = context(true);
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
    .expect("aset compiles");
    for source in ["(vector nil)", "(record 'journal nil)"] {
        let owner = context.eval_str(source).expect("array owner");
        let roots = context.save_specpdl_roots();
        context.push_specpdl_root(owner);
        let index = Value::make_int(if owner.as_vector_data().is_some() {
            0
        } else {
            1
        });
        // Arm aset's function epoch through its initial outlined call.
        native(&mut context, &leaf, &[owner, index, Value::make_int(1)]);
        let (_, reads) = capture(|| owner.veclike_type());
        let before = super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get());
        let revision = LispCollectionRevision::current();
        native(&mut context, &leaf, &[owner, index, Value::make_int(2)]);
        assert_eq!(
            super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get()),
            before
        );
        assert_ne!(LispCollectionRevision::current(), revision);
        assert!(!reads.expect("coherent array read").unchanged());

        let (_, writes) =
            capture(|| native(&mut context, &leaf, &[owner, index, Value::make_int(3)]));
        let writes = writes.expect("native aset observes its owner after the write");
        if index == Value::make_int(0) {
            assert!(owner.set_vector_slot(0, Value::NIL));
        } else {
            assert!(owner.set_record_slot(1, Value::NIL));
        }
        assert!(!writes.unchanged());
        context.restore_specpdl_roots(roots);
    }
}

#[test]
fn generational_inline_blv_stores_invalidate_collection_read_certificates() {
    use super::super::shims::VARSET_SHIM_CALLS;
    for local in [false, true] {
        let mut context = blv_fixture(local);
        let owner = blv_cell(&context, local);
        let leaf = blv_set_leaf(&context);
        let (_, reads) = capture(|| owner.cons_cdr());
        let before = VARSET_SHIM_CALLS.with(|count| count.get());
        assert_eq!(
            native(&mut context, &leaf, &[Value::make_int(27)]),
            Value::make_int(27)
        );
        assert_eq!(VARSET_SHIM_CALLS.with(|count| count.get()), before);
        assert!(!reads.expect("coherent BLV cell read").unchanged());
    }
}
