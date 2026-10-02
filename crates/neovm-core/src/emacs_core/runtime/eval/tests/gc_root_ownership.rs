use super::*;

fn scratch_roots(ctx: &Context) -> Vec<Value> {
    let mut roots = Vec::new();
    collect_thread_local_gc_roots(
        &mut roots,
        ctx.tagged_heap.identity(),
        ctx.tagged_heap.gc_collections(),
        crate::tagged::gc::CacheRootScan::Snapshot {
            collection_in_progress: ctx.tagged_heap.mark_in_progress()
                || ctx.tagged_heap.sweep_in_progress(),
        },
        &mut Vec::new(),
    );
    roots
        .into_iter()
        .filter_map(|(value, origin)| (origin == "scratch-thread-local").then_some(value))
        .collect()
}

#[test]
fn gc_scratch_roots_exclude_a_dropped_heap() {
    let saved = save_scratch_gc_roots();
    let old = {
        let mut ctx = Context::new();
        let value = ctx.eval_str("(make-hash-table)").unwrap();
        push_scratch_gc_root(value);
        assert!(scratch_roots(&ctx).contains(&value));
        value
    };
    let mut ctx = Context::new();
    // Compare only tagged words. The old Box was freed when its Context died.
    assert!(
        !scratch_roots(&ctx).contains(&old),
        "scratch roots outlived their heap"
    );
    ctx.gc_collect_exact();
    restore_scratch_gc_roots(saved);
}

#[test]
fn gc_scratch_root_slots_preserve_each_live_heaps_roots() {
    let saved = save_scratch_gc_roots();
    let mut first = Context::new();
    let a = first.eval_str("(vector 'first)").unwrap();
    let slot = push_scratch_gc_root_slot(a);
    push_scratch_gc_roots(&[a]);
    let mut second = Context::new();
    let b = second.eval_str("(vector 'second)").unwrap();
    push_scratch_gc_roots(&[b]);
    assert_eq!(scratch_roots(&second), vec![b]);
    first.setup_thread_locals();
    let replacement = first.eval_str("(vector 'replacement)").unwrap();
    set_scratch_gc_root(slot, replacement);
    assert_eq!(scratch_roots(&first), vec![replacement, a]);
    first.gc_collect_exact();
    second.setup_thread_locals();
    second.gc_collect_exact();
    assert_eq!(a.as_vector_data().unwrap()[0], Value::symbol("first"));
    assert_eq!(b.as_vector_data().unwrap()[0], Value::symbol("second"));
    assert_eq!(
        replacement.as_vector_data().unwrap()[0],
        Value::symbol("replacement")
    );
    restore_scratch_gc_roots(saved);
}

#[test]
fn gc_scratch_roots_without_an_active_heap_share_only_immediates() {
    let saved = save_scratch_gc_roots();
    let old = {
        let mut ctx = Context::new();
        ctx.eval_str("(make-hash-table)").unwrap()
    };
    assert!(crate::tagged::gc::current_tagged_heap_identity().is_none());
    push_scratch_gc_roots(&[Value::fixnum(7), old]);
    let mut ctx = Context::new();
    let roots = scratch_roots(&ctx);
    assert!(roots.len() == 1 && roots[0] == Value::fixnum(7));
    ctx.gc_collect_exact();
    restore_scratch_gc_roots(saved);
}
