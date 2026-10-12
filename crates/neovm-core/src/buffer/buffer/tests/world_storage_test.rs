//! Buffer storage ownership and the explicit detached-clone root contract.

use super::*;
use crate::emacs_core::eval::Context;

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

#[test]
fn standalone_manager_publishes_one_coherent_selection() {
    let mut manager = BufferManager::new();
    assert!(matches!(manager.backing, BufferBacking::Standalone(_)));
    assert_selection_matches_storage(&manager);
    let next = manager.create_buffer("*standalone-selection*");
    assert!(manager.switch_current(next));
    assert_selection_matches_storage(&manager);
    assert_eq!(manager.current_buffer_id(), Some(next));
}

#[test]
fn from_dump_rebuilds_the_selection_for_the_restored_slab() {
    let original = BufferManager::new();
    let current = original.current_buffer_id();
    let buffers = original
        .dump_buffers()
        .map(|(id, buffer)| (id, buffer.clone()))
        .collect();
    let restored = BufferManager::from_dump(
        buffers,
        current,
        original.dump_next_id(),
        original.dump_next_marker_id(),
        Some(original.dump_buffer_order()),
        Some(&original.buffer_defaults),
        original.implemented_default_text_backend_kind(),
        SavedPointBeforeCommand::new_editor_global(),
    );
    assert_eq!(restored.current_buffer_id(), current);
    assert_selection_matches_storage(&restored);
    assert_ne!(original.current_ptr, restored.current_ptr);
}

#[test]
fn deep_clone_has_independent_buffers_and_current_selection() {
    let mut original = BufferManager::new();
    let first = original.current_buffer_id().unwrap();
    let second = original.create_buffer("*clone-selection*");
    let mut clone = original.clone();
    assert_ne!(clone.storage_ptr, original.storage_ptr);
    assert_ne!(clone.current_ptr, original.current_ptr);
    assert!(clone.switch_current(second));
    clone
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("p0-clone-only", Value::fixnum(41));
    assert_eq!(original.current_buffer_id(), Some(first));
    assert_eq!(
        original
            .buffers
            .get(&second)
            .unwrap()
            .buffer_local_value("p0-clone-only"),
        None
    );
    assert_eq!(
        clone
            .current_buffer()
            .unwrap()
            .buffer_local_value("p0-clone-only"),
        Some(Value::fixnum(41))
    );
    assert_selection_matches_storage(&original);
    assert_selection_matches_storage(&clone);
}

#[test]
fn selection_retires_before_kill_and_survives_slab_growth() {
    let mut manager = BufferManager::new();
    let current = manager.current_buffer_id().unwrap();
    let pointer = manager.current_ptr;
    for index in 0..64 {
        manager.create_buffer(&format!("*storage-growth-{index}*"));
    }
    assert_eq!(manager.current_ptr, pointer);
    assert_selection_matches_storage(&manager);
    assert!(manager.kill_buffer(current));
    assert_eq!(manager.current_buffer_id(), None);
    assert_eq!(manager.current_ptr, None);
    assert_selection_matches_storage(&manager);
}

#[test]
fn selection_is_rederived_after_swap_replaces_a_live_box() {
    let mut manager = BufferManager::new();
    let first = manager.current_buffer_id().unwrap();
    let second = manager.create_buffer("*swap-selection*");
    manager.swap_buffer_text(first, second).unwrap();
    assert_eq!(manager.current_buffer_id(), Some(first));
    assert_selection_matches_storage(&manager);
    assert!(manager.switch_current(second));
    manager.swap_buffer_text(first, second).unwrap();
    assert_eq!(manager.current_buffer_id(), Some(second));
    assert_selection_matches_storage(&manager);
}

#[test]
fn world_installation_reuses_the_existing_storage_allocation() {
    let context = Context::new();
    assert!(matches!(context.buffers.backing, BufferBacking::World(_)));
    assert_selection_matches_storage(&context.buffers);
    let clone = context.buffers.clone();
    assert!(matches!(clone.backing, BufferBacking::DetachedClone { .. }));
    assert_selection_matches_storage(&clone);
    assert_ne!(clone.storage_ptr, context.buffers.storage_ptr);
}

#[test]
fn installed_view_cannot_surrender_its_world_storage() {
    let mut context = Context::new();
    let manager = std::mem::replace(&mut context.buffers, BufferManager::new());
    let manager = manager
        .into_world_parts()
        .expect_err("World storage cannot become standalone");
    assert_selection_matches_storage(&manager);
    context.buffers = manager;
}

#[test]
fn escaped_runtime_manager_retains_its_world_after_context_drop() {
    let mut context = Context::new();
    let manager = std::mem::replace(&mut context.buffers, BufferManager::new());
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
        .buffers
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("p0-detached-root", payload);
    let clone = context.buffers.clone();
    context
        .buffers
        .current_buffer_mut()
        .unwrap()
        .kill_buffer_local("p0-detached-root");
    assert_eq!(
        context
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
    let mut clone = context.buffers.clone();
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
fn infallible_world_parts_factory_preserves_standalone_selection() {
    let parts = BufferManager::new_world_parts();
    let manager = BufferManager::from(parts);
    assert!(matches!(manager.backing, BufferBacking::Standalone(_)));
    assert_eq!(
        manager
            .current_buffer()
            .unwrap()
            .name_runtime_string_owned(),
        "*scratch*"
    );
    assert_selection_matches_storage(&manager);
}

#[test]
fn owned_parts_adapter_rejects_both_runtime_and_detached_world_views() {
    let mut context = Context::new();
    let detached = context.buffers.clone();
    assert_eq!(
        OwnedBufferStorageParts::try_from(detached).unwrap_err(),
        BufferStorageOwnershipError::DetachedFromWorld,
    );
    let runtime = std::mem::replace(&mut context.buffers, BufferManager::new());
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
    assert!(matches!(context.buffers.backing, BufferBacking::World(_)));
    assert_selection_matches_storage(&context.buffers);
    assert_eq!(context.buffers.buffer_list().len(), 1);
}

#[test]
fn cold_reset_retires_selection_and_preserves_the_storage_address() {
    let mut context = Context::new();
    let pointer = context.buffers.storage_ptr;
    let temporary = context.buffers.create_buffer("*before-minimal-reset*");
    assert!(context.buffers.switch_current(temporary));
    context.buffers.reset_for_minimal_runtime();
    assert!(matches!(context.buffers.backing, BufferBacking::World(_)));
    assert_eq!(context.buffers.storage_ptr, pointer);
    assert_eq!(context.buffers.buffer_list().len(), 1);
    assert_eq!(
        context
            .buffers
            .current_buffer()
            .unwrap()
            .name_runtime_string_owned(),
        "*scratch*"
    );
    assert_selection_matches_storage(&context.buffers);
}
