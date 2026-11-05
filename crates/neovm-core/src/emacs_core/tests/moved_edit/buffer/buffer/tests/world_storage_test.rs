//! Buffer storage ownership and the explicit detached-clone root contract.

use crate::emacs_core::eval::Context;

use neovm_edit::buffer::buffer::*;

#[test]
fn world_installation_reuses_the_existing_storage_allocation() {
    let context = Context::new();
    assert!(matches!(
        context.edit.buffers.backing,
        BufferBacking::Leased { detached: None, .. }
    ));
    assert_selection_matches_storage(&context.edit.buffers);
    let clone = context.edit.buffers.clone();
    assert!(matches!(
        clone.backing,
        BufferBacking::Leased {
            detached: Some(_),
            ..
        }
    ));
    assert_selection_matches_storage(&clone);
    assert_ne!(clone.storage_ptr, context.edit.buffers.storage_ptr);
}

#[test]
fn installed_view_cannot_surrender_its_world_storage() {
    let mut context = Context::new();
    let manager = std::mem::replace(&mut context.edit.buffers, BufferManager::new());
    let manager = manager
        .into_world_parts()
        .expect_err("World storage cannot become standalone");
    assert_selection_matches_storage(&manager);
    context.edit.buffers = manager;
}

#[test]
fn escaped_runtime_manager_retains_its_world_after_context_drop() {
    let mut context = Context::new();
    let manager = std::mem::replace(&mut context.edit.buffers, BufferManager::new());
    let expected_name = manager
        .current_buffer()
        .unwrap()
        .name_runtime_string_owned();
    drop(context);
    assert_eq!(
        manager
            .current_buffer()
            .unwrap()
            .name_runtime_string_owned(),
        expected_name
    );
    assert_selection_matches_storage(&manager);
}

#[test]
fn detached_clone_roots_keep_source_removed_values_alive() {
    let mut context = Context::new();
    let payload = Value::vector(vec![Value::cons(Value::fixnum(73), Value::NIL)]);
    context
        .edit
        .buffers
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("p0-detached-root", payload);
    let clone = context.edit.buffers.clone();
    context
        .edit
        .buffers
        .current_buffer_mut()
        .unwrap()
        .kill_buffer_local("p0-detached-root");
    assert_eq!(
        context
            .edit
            .buffers
            .current_buffer()
            .unwrap()
            .buffer_local_value("p0-detached-root"),
        None
    );
    let mut roots = Vec::new();
    clone.trace_roots(&mut roots);
    let _published = context.retain_gc_roots(roots);
    context.gc_collect_exact();
    let retained = clone
        .current_buffer()
        .unwrap()
        .buffer_local_value("p0-detached-root")
        .unwrap();
    assert_eq!(
        retained.as_vector_data().unwrap()[0].cons_car(),
        Value::fixnum(73)
    );
}

#[test]
fn detached_clone_mutation_refreshes_its_explicit_root_publication() {
    let mut context = Context::new();
    let mut clone = context.edit.buffers.clone();
    let old_payload = Value::cons(Value::fixnum(81), Value::NIL);
    clone
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("p0-refreshed-root", old_payload);
    let mut roots = Vec::new();
    clone.trace_roots(&mut roots);
    let old_publication = context.retain_gc_roots(roots);
    context.gc_collect_exact();
    let new_payload = Value::vector(vec![Value::cons(Value::fixnum(91), Value::NIL)]);
    clone
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("p0-refreshed-root", new_payload);
    let mut roots = Vec::new();
    clone.trace_roots(&mut roots);
    let _new_publication = context.retain_gc_roots(roots);
    drop(old_publication);
    context.gc_collect_exact();
    let retained = clone
        .current_buffer()
        .unwrap()
        .buffer_local_value("p0-refreshed-root")
        .unwrap();
    assert_eq!(
        retained.as_vector_data().unwrap()[0].cons_car(),
        Value::fixnum(91)
    );
}

#[test]
fn owned_parts_adapter_rejects_both_runtime_and_detached_world_views() {
    let mut context = Context::new();
    let detached = context.edit.buffers.clone();
    assert_eq!(
        OwnedBufferStorageParts::try_from(detached).unwrap_err(),
        BufferStorageOwnershipError::DetachedFromWorld,
    );
    let runtime = std::mem::replace(&mut context.edit.buffers, BufferManager::new());
    assert_eq!(
        OwnedBufferStorageParts::try_from(runtime).unwrap_err(),
        BufferStorageOwnershipError::WorldBacked,
    );
    let standalone = BufferManager::new();
    let pointer = standalone.storage_ptr;
    let owned = OwnedBufferStorageParts::try_from(standalone).unwrap();
    let restored = BufferManager::from(owned);
    assert_eq!(pointer, restored.storage_ptr);
    assert_selection_matches_storage(&restored);
}

#[test]
fn minimal_runtime_keeps_buffers_in_its_world_allocation() {
    let context = Context::new_minimal_vm_harness();
    assert!(matches!(
        context.edit.buffers.backing,
        BufferBacking::Leased { detached: None, .. }
    ));
    assert_selection_matches_storage(&context.edit.buffers);
    assert_eq!(context.edit.buffers.buffer_list().len(), 1);
}

#[test]
fn cold_reset_retires_selection_and_preserves_the_storage_address() {
    let mut context = Context::new();
    let pointer = context.edit.buffers.storage_ptr;
    let temporary = context.edit.buffers.create_buffer("*before-minimal-reset*");
    assert!(context.edit.buffers.switch_current(temporary));
    context.edit.buffers.reset_for_minimal_runtime();
    assert!(matches!(
        context.edit.buffers.backing,
        BufferBacking::Leased { detached: None, .. }
    ));
    assert_eq!(context.edit.buffers.storage_ptr, pointer);
    assert_eq!(context.edit.buffers.buffer_list().len(), 1);
    assert_eq!(
        context
            .edit
            .buffers
            .current_buffer()
            .unwrap()
            .name_runtime_string_owned(),
        "*scratch*"
    );
    assert_selection_matches_storage(&context.edit.buffers);
}

fn assert_selection_matches_storage(manager: &BufferManager) {
    let expected = manager.current_buffer().map(std::ptr::from_ref);
    assert_eq!(jit_layout::current_buffer_ptr_via_walk(manager), expected);
    assert_eq!(
        jit_layout::current_buffer_ptr_via_projection(manager),
        expected
    );
    assert_eq!(
        manager.current_raw,
        manager.current_buffer_id().map_or(0, |id| id.0)
    );
}
