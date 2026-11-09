//! Buffer storage ownership and the explicit detached-clone root contract.

use super::*;

#[test]
fn standalone_manager_publishes_one_coherent_selection() {
    crate::test_utils::init_test_tracing();
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
    crate::test_utils::init_test_tracing();
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
    crate::test_utils::init_test_tracing();
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
            .get(second)
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
    crate::test_utils::init_test_tracing();
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
    crate::test_utils::init_test_tracing();
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
fn infallible_world_parts_factory_preserves_standalone_selection() {
    crate::test_utils::init_test_tracing();
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
