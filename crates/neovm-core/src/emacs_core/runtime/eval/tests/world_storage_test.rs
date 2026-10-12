use super::*;

#[test]
fn world_storage_views_keep_exact_allocations_and_canonical_identity() {
    let mut context = Context::new();
    let view = &context.obarray;
    assert_eq!(view.ptr.as_ptr(), view.lease.0.obarray.get());
    let expected = view
        .ptr
        .as_ptr()
        .cast::<u8>()
        .wrapping_add(OBARRAY_DISPATCH_CELLS_OFFSET)
        .cast::<CanonicalDispatchCells>();
    assert_eq!(view.dispatch.as_ptr(), expected);
    let old = context.dispatch_projection().function_epoch();
    context.obarray.invalidate_all_function_bindings(
        crate::emacs_core::symbol::FunctionEpochBump::CompilerOverrides,
    );
    assert!(context.dispatch_projection().function_epoch() > old);
    assert_eq!(
        context.dispatch_projection().function_epoch(),
        context.obarray().function_epoch()
    );
    assert!(context.obarray.admission_matches_canonical());
    assert_eq!(
        context.tagged_heap.ptr.as_ptr(),
        context.tagged_heap.lease.0.tagged_heap.get()
    );
}

#[test]
fn world_storage_shutdown_rejects_an_escaped_buffer_view() {
    let mut context = Box::new(Context::new());
    let name = context.buffers.current_buffer().map(|b| b.name.clone());
    let escaped = std::mem::replace(&mut context.buffers, BufferManager::new());
    match context.shutdown() {
        Err(ContextShutdownError::OutstandingViews(count)) => assert_eq!(count.0.get(), 1),
        other => panic!("expected an outstanding storage view: {other:?}"),
    }
    assert_eq!(escaped.current_buffer().map(|b| b.name.clone()), name);
    drop(escaped);
}

#[test]
fn world_storage_shutdown_succeeds_after_extracted_view_retirement() {
    let mut context = Box::new(Context::new());
    let escaped = std::mem::replace(&mut context.buffers, BufferManager::new());
    drop(escaped);
    context
        .shutdown()
        .expect("sole owner can explicitly finish");
}

#[test]
fn world_storage_cold_constructor_reads_its_own_canonical_cells() {
    // The minimal factory prepares its replacement manager BEFORE building the
    // World/view/header. An admitted owner has no mutable replacement API.
    let context = Context::new_minimal_vm_harness();
    let view = &context.obarray;
    assert_eq!(view.ptr.as_ptr(), view.lease.0.obarray.get());
    let canonical = view
        .ptr
        .as_ptr()
        .cast::<u8>()
        .wrapping_add(OBARRAY_DISPATCH_CELLS_OFFSET)
        .cast::<CanonicalDispatchCells>();
    assert_eq!(view.dispatch.as_ptr(), canonical);
    assert_eq!(
        context.dispatch_projection().function_epoch(),
        context.obarray().function_epoch()
    );
    assert!(view.admission_matches_canonical());
    assert!(
        context
            .obarray()
            .bool_forwarder(intern("debug-on-next-call"))
            .is_none()
    );
    assert!(
        !context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
}

#[test]
fn world_storage_factory_reinstalls_final_heap_before_lisp_allocates() {
    let mut context = Context::new();
    let identity = context.tagged_heap.identity();
    assert_eq!(
        crate::tagged::gc::current_tagged_heap_identity(),
        Some(identity)
    );
    assert!(crate::tagged::gc::tagged_heap_is_current(
        &context.tagged_heap
    ));
    let value = Value::string("final World allocation survives construction");
    context.eval_temp_roots.push(value);
    context.gc_collect_from_current_roots();
    assert_eq!(
        value.as_str_owned().as_deref(),
        Some("final World allocation survives construction")
    );
    drop(context);
    assert!(!crate::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn world_storage_dispatch_reads_live_forwarded_boolean_after_collection() {
    let mut context = Context::new();
    let id = intern("debug-on-next-call");
    context
        .obarray
        .bool_forwarder(id)
        .expect("startup descriptor")
        .set(true);
    assert!(
        context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    context.gc_collect_from_current_roots();
    assert!(
        context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    context
        .obarray
        .bool_forwarder(id)
        .expect("live canonical descriptor")
        .set(false);
    assert!(
        !context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
}
