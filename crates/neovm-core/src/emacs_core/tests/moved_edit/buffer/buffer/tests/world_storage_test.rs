//! Buffer storage ownership and the explicit detached-clone root contract.

use super::*;

use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::cold_retained::ColdWorldLease;
use neovm_value::gc_trace::GcTrace;
use neovm_value::intern::{SymId, WellKnown, intern};
use neovm_value::symbol::SetInternalAlist;
use rustc_hash::FxHashMap;
use std::cell::{RefCell, UnsafeCell};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::marker::PhantomData;
use std::mem;
use std::ops::Deref;
use std::ptr::NonNull;
use std::sync::OnceLock;

use crate::emacs_core::eval::Context;
use neovm_value::value::Value;

use neovm_edit::buffer::buffer::*;

#[test]
fn world_installation_reuses_the_existing_storage_allocation() {
    let context = crate::test_utils::new_context();
    assert_eq!(
        context.edit.buffers.backing_kind_for_test(),
        BufferBackingKindForTest::LeasedAttached
    );
    assert_selection_matches_storage(&context.edit.buffers);
    let clone = context.edit.buffers.clone();
    assert_eq!(
        clone.backing_kind_for_test(),
        BufferBackingKindForTest::LeasedDetached
    );
    assert_selection_matches_storage(&clone);
    assert_ne!(
        clone.storage_address_for_test(),
        context.edit.buffers.storage_address_for_test()
    );
}

#[test]
fn installed_view_cannot_surrender_its_world_storage() {
    let mut context = crate::test_utils::new_context();
    let manager = std::mem::replace(&mut context.edit.buffers, BufferManager::new());
    let manager = manager
        .into_world_parts()
        .expect_err("World storage cannot become standalone");
    assert_selection_matches_storage(&manager);
    context.edit.buffers = manager;
}

#[test]
fn escaped_runtime_manager_retains_its_world_after_context_drop() {
    let mut context = crate::test_utils::new_context();
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
    let mut context = crate::test_utils::new_context();
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
    let mut context = crate::test_utils::new_context();
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
    let mut context = crate::test_utils::new_context();
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
    let pointer = standalone.storage_address_for_test();
    let owned = OwnedBufferStorageParts::try_from(standalone).unwrap();
    let restored = BufferManager::from(owned);
    assert_eq!(pointer, restored.storage_address_for_test());
    assert_selection_matches_storage(&restored);
}

#[test]
fn cold_reset_retires_selection_and_preserves_the_storage_address() {
    let mut context = crate::test_utils::new_context();
    let pointer = context.edit.buffers.storage_address_for_test();
    let temporary = context.edit.buffers.create_buffer("*before-minimal-reset*");
    assert!(context.edit.buffers.switch_current(temporary));
    context.edit.buffers.reset_for_minimal_runtime();
    assert_eq!(
        context.edit.buffers.backing_kind_for_test(),
        BufferBackingKindForTest::LeasedAttached
    );
    assert_eq!(context.edit.buffers.storage_address_for_test(), pointer);
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
        manager.current_raw_for_test(),
        manager.current_buffer_id().map_or(0, |id| id.0)
    );
}
