use super::*;

#[test]
fn cloned_lease_survives_exact_gc_and_retires_with_last_owner() {
    let mut context = Context::new();
    context.gc_collect_exact();
    let baseline = context.tagged_heap.allocated_count();
    let value = Value::cons(Value::fixnum(17), Value::vector(vec![Value::fixnum(23)]));
    let lease = context.retain_gc_roots([value]);
    let pending = lease.clone();
    drop(lease);
    context.gc_collect_exact();
    assert_eq!(
        context.tagged_heap.allocated_count(),
        baseline + 2,
        "a pending native owner must keep the whole captured graph alive"
    );
    assert_eq!(value.cons_car(), Value::fixnum(17));
    assert_eq!(
        value.cons_cdr().as_vector_data().unwrap().as_slice(),
        &[Value::fixnum(23)]
    );
    drop(pending);
    context.gc_collect_exact();
    assert_eq!(context.tagged_heap.allocated_count(), baseline);
    assert!(context.owned_roots.leases.borrow().is_empty());
}

#[test]
fn weak_retainer_registers_live_graphs_without_extending_context_lifetime() {
    let mut context = Context::new();
    context.gc_collect_exact();
    let baseline = context.tagged_heap.allocated_count();
    let factory = context.owned_root_retainer();
    let value = Value::cons(Value::fixnum(7), Value::vector(vec![Value::fixnum(9)]));
    let lease = factory.retain([value]).unwrap();
    context.gc_collect_exact();
    assert_eq!(context.tagged_heap.allocated_count(), baseline + 2);
    assert_eq!(value.cons_car(), Value::fixnum(7));
    drop(lease);
    context.gc_collect_exact();
    assert_eq!(context.tagged_heap.allocated_count(), baseline);
    drop(context);
    assert!(factory.retain([Value::NIL]).is_none());
}

#[test]
fn retired_leases_do_not_accumulate_between_collections() {
    let context = Context::new();
    let value = Value::cons(Value::fixnum(1), Value::NIL);
    for _ in 0..1000 {
        let lease = context.retain_gc_roots([value]);
        assert_eq!(context.owned_roots.leases.borrow().len(), 1);
        drop(lease);
    }
    let immediate = context.retain_gc_roots([Value::NIL, Value::fixnum(2)]);
    assert!(immediate.values.is_empty());
}
