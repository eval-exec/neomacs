use super::*;

/// Count actual weak-lease inspections, not timing: registrations must remain
/// linear with both a growing live set and long-lived plus transient owners.
#[test]
fn root_registration_pruning_work_is_amortized_linear() {
    let mut context = Context::new();
    context.gc_collect_exact();
    let baseline = context.tagged_heap.allocated_count();
    let value = Value::string("registration workload");
    let retainer = context.owned_root_retainer();
    let count = 1024;
    LEASE_PRUNING_INSPECTIONS.with(|work| work.set(0));
    let mut live: std::collections::VecDeque<_> = (0..count)
        .map(|_| retainer.retain([value]).unwrap())
        .collect();
    let growing = LEASE_PRUNING_INSPECTIONS.with(|work| work.get());
    assert!(
        growing <= 8 * count,
        "growing live leases: {growing} inspections for {count} registrations"
    );
    LEASE_PRUNING_INSPECTIONS.with(|work| work.set(0));
    for _ in 0..4 * count {
        drop(retainer.retain([value]).unwrap());
    }
    let transient = LEASE_PRUNING_INSPECTIONS.with(|work| work.get());
    assert!(
        transient <= 8 * 4 * count,
        "live plus transient leases: {transient} inspections"
    );
    // Fill the registry, then expire its oldest (non-tail) lease each time.
    // A capacity sweep freeing only one slot must create more headroom rather
    // than immediately rescanning the whole live set on the next insertion.
    let capacity = context.owned_roots.leases.borrow().capacity();
    while live.len() < capacity {
        live.push_back(retainer.retain([value]).unwrap());
    }
    LEASE_PRUNING_INSPECTIONS.with(|work| work.set(0));
    for _ in 0..4 * capacity {
        drop(live.pop_front());
        live.push_back(retainer.retain([value]).unwrap());
    }
    let rotating = LEASE_PRUNING_INSPECTIONS.with(|work| work.get());
    assert!(
        rotating <= 8 * 4 * capacity,
        "rotating interior leases: {rotating} inspections"
    );
    eprintln!(
        "root-registration inspections: growing={growing}, transient={transient}, rotating={rotating}"
    );
    drop(live);
    context.gc_collect_exact();
    assert_eq!(
        context.tagged_heap.allocated_count(),
        baseline,
        "dead weak entries do not keep Lisp graphs alive"
    );
}

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
