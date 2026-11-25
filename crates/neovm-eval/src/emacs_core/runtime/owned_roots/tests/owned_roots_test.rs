use super::*;

/// Count actual weak-lease inspections, not timing: registrations must remain
/// linear with both a growing live set and long-lived plus transient owners.
#[test]
fn root_registration_pruning_work_is_amortized_linear() {
    let mut context = Context::new(crate::test_utils::unit_runtime());
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
    let capacity = context.owned_roots.leases.values.borrow().capacity();
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
    let mut context = Context::new(crate::test_utils::unit_runtime());
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
        value.cons_cdr().as_vector_data().unwrap(),
        &[Value::fixnum(23)]
    );
    drop(pending);
    context.gc_collect_exact();
    assert_eq!(context.tagged_heap.allocated_count(), baseline);
    assert!(context.owned_roots.leases.values.borrow().is_empty());
}

#[test]
fn weak_retainer_registers_live_graphs_without_extending_context_lifetime() {
    let mut context = Context::new(crate::test_utils::unit_runtime());
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
    let context = Context::new(crate::test_utils::unit_runtime());
    let value = Value::cons(Value::fixnum(1), Value::NIL);
    for _ in 0..1000 {
        let lease = context.retain_gc_roots([value]);
        assert_eq!(context.owned_roots.leases.values.borrow().len(), 1);
        drop(lease);
    }
    let immediate = context.retain_gc_roots([Value::NIL, Value::fixnum(2)]);
    assert!(immediate.values.is_empty());
}

#[test]
fn held_text_property_registration_skips_unchanged_64k_table_walk() {
    use neovm_edit::buffer::{CharPos0, CharRange, EmacsByteRange};
    use neovm_value::gc_trace::GcTrace;
    use neovm_value::heap_types::LispString;
    use neovm_value::value::text_props::{
        PropertyInterval, TextPropertyTable, reset_text_property_root_node_visits_for_test,
        text_property_root_node_visits_for_test,
    };

    let mut context = Context::new(crate::test_utils::unit_runtime());
    let id = context
        .buffer_manager_mut()
        .create_buffer("held-property-root-budget");
    let face = Value::symbol("face");
    let count = 65_536;
    {
        let buffer = context.buffer_manager_mut().get_mut(id).unwrap();
        buffer.set_undo_list(Value::T);
        let text = "x".repeat(count);
        buffer.insert(&text);
        // Existing restore API constructs a balanced table once, avoiding
        // 64K mutation transactions in the fixture setup.
        let table = TextPropertyTable::from_dump(
            (0..count)
                .map(|pos| {
                    PropertyInterval::from_parts(
                        CharRange::new(CharPos0::new(pos), CharPos0::new(pos + 1)),
                        std::collections::HashMap::from([(face, Value::fixnum(pos as i64 + 1))]),
                        vec![face],
                    )
                })
                .collect(),
        );
        buffer.replace_lisp_string_with_text_props(&LispString::from_utf8(&text), table);
    }
    // The only pending view comes from an indirect source which has already
    // died; its live base has no lease. GC must still recognize the table as
    // covered by ordinary tracing, independently of capture/source identity.
    let indirect = context
        .buffer_manager_mut()
        .create_indirect_buffer(id, "held-property-root-indirect", false)
        .unwrap();
    reset_text_property_root_node_visits_for_test();
    let snapshot = context
        .buffer_manager()
        .get(indirect)
        .unwrap()
        .text_snapshot();
    assert_eq!(
        text_property_root_node_visits_for_test(),
        0,
        "text capture must not enumerate the table"
    );
    assert!(context.buffer_manager_mut().kill_buffer(indirect));
    let held_identity = snapshot.text_property_roots().identity();
    assert!(
        context
            .buffer_manager()
            .live_text_property_table_identities()
            .any(|identity| identity == held_identity),
        "the unregistered baseline is safe only while a live base owns this exact table"
    );
    reset_text_property_root_node_visits_for_test();
    let mut ordinary_roots = Vec::new();
    context.buffer_manager().trace_roots(&mut ordinary_roots);
    let ordinary = text_property_root_node_visits_for_test();
    assert_eq!(
        ordinary, count,
        "the fixture must really walk 64K logical nodes"
    );
    reset_text_property_root_node_visits_for_test();
    context.gc_collect_exact();
    let before_gc = text_property_root_node_visits_for_test();
    assert!(
        before_gc >= count,
        "the baseline must enumerate the actual native table during GC"
    );
    reset_text_property_root_node_visits_for_test();
    let retainer = context.owned_root_retainer();
    let lease = retainer
        .retain_text_properties(snapshot.text_property_roots())
        .unwrap();
    let second_lease = retainer
        .retain_text_properties(snapshot.text_property_roots())
        .unwrap();
    assert_eq!(
        text_property_root_node_visits_for_test(),
        0,
        "capture registration must not enumerate a table"
    );
    reset_text_property_root_node_visits_for_test();
    context.gc_collect_exact();
    let held_current_gc = text_property_root_node_visits_for_test();
    assert_eq!(
        held_current_gc, before_gc,
        "per-GC native table trace volume must remain unchanged"
    );
    reset_text_property_root_node_visits_for_test();
    ordinary_roots.clear();
    context.buffer_manager().trace_roots(&mut ordinary_roots);
    context
        .owned_roots
        .trace_roots_with_buffers(context.buffer_manager(), &mut |value| {
            ordinary_roots.push(value)
        });
    let held_current = text_property_root_node_visits_for_test();
    assert_eq!(
        held_current, ordinary,
        "holding an unchanged table must add zero native node visits"
    );

    context
        .buffer_manager_mut()
        .get_mut(id)
        .unwrap()
        .delete_emacs_byte_range(EmacsByteRange::from_usize(0, count));
    reset_text_property_root_node_visits_for_test();
    context
        .owned_roots
        .trace_roots_with_buffers(context.buffer_manager(), &mut |_| {});
    let detached = text_property_root_node_visits_for_test();
    assert_eq!(
        detached, count,
        "two live leases on one old table must enumerate it only once"
    );
    reset_text_property_root_node_visits_for_test();
    context.gc_collect_exact();
    let detached_gc = text_property_root_node_visits_for_test();
    assert!(
        detached_gc >= count,
        "actual GC must enumerate the detached held table"
    );
    drop(lease);
    drop(second_lease);
    reset_text_property_root_node_visits_for_test();
    context
        .owned_roots
        .trace_roots_with_buffers(context.buffer_manager(), &mut |_| {});
    assert_eq!(
        text_property_root_node_visits_for_test(),
        0,
        "last lease retirement must remove the extra trace"
    );
    assert!(
        context
            .owned_roots
            .leases
            .text_properties
            .borrow()
            .is_empty()
    );
    reset_text_property_root_node_visits_for_test();
    context.gc_collect_exact();
    let retired_gc = text_property_root_node_visits_for_test();
    assert_eq!(
        retired_gc, 0,
        "retired old tables must add no actual GC node visits"
    );
    eprintln!(
        "native property-root node visits: per-GC-before={before_gc}, per-GC-held-current={held_current_gc}, per-GC-detached={detached_gc}, per-GC-retired={retired_gc}, ordinary={ordinary}, held-current={held_current}, detached={detached}"
    );
    drop(snapshot);
}

#[test]
fn held_text_property_lease_clones_trace_old_table_until_last_drop() {
    use neovm_edit::buffer::{EmacsBytePos, EmacsByteRange};
    let mut context = Context::new(crate::test_utils::unit_runtime());
    let id = context
        .buffer_manager_mut()
        .create_buffer("held-property-root-lifetime");
    let face = Value::symbol("face");
    let old = Value::string("snapshot-only property operand");
    let buffer = context.buffer_manager_mut().get_mut(id).unwrap();
    buffer.set_undo_list(Value::T);
    buffer.insert("xy");
    buffer.text_props_put_property_in_emacs_byte_range(EmacsByteRange::from_usize(0, 2), face, old);
    let snapshot = buffer.text_snapshot();
    let lease = context
        .owned_root_retainer()
        .retain_text_properties(snapshot.text_property_roots())
        .unwrap();
    let pending = (snapshot.clone(), lease.clone());
    drop(snapshot);
    drop(lease);
    context
        .buffer_manager_mut()
        .get_mut(id)
        .unwrap()
        .delete_emacs_byte_range(EmacsByteRange::from_usize(0, 2));
    context.gc_collect_exact();
    assert!(
        context.tagged_heap.owns_heap_value_for_test(old),
        "old captured operand must pass allocation guard before dereference"
    );
    assert_eq!(old.as_utf8_str(), Some("snapshot-only property operand"));
    assert_eq!(
        pending
            .0
            .text_prop_at_emacs_byte_pos(EmacsBytePos::new(0), face),
        Some(old)
    );
    drop(pending);
    context.gc_collect_exact();
    assert!(
        !context.tagged_heap.owns_heap_value_for_test(old),
        "last snapshot/lease drop must release the old graph"
    );
    assert!(
        context
            .owned_roots
            .leases
            .text_properties
            .borrow()
            .is_empty()
    );
}
