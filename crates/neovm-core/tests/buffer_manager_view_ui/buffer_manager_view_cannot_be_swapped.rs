//! rt A3 2c (d) Fixture 1: `BufferManagerMut` and `EditRuntimeMut` cannot be
//! swapped via `std::mem::swap` (neither with another view nor with a freshly
//! constructed `BufferManager::new()`), and expose no `into_manager()` /
//! `into_runtime()` unwrap.
//!
//! Bindings are declared `mut` (`#![allow(unused_mut)]` keeps `.stderr` free of
//! noise) so the SOLE reason compilation fails is the missing `DerefMut` /
//! unwrap method — never an unrelated missing-`mut` E0596.
#![allow(unused_mut)]

use neovm_core::buffer::{BufferManager, BufferManagerMut, EditRuntimeMut};

fn swap_two_buffer_views(mut first: BufferManagerMut<'_>, mut second: BufferManagerMut<'_>) {
    std::mem::swap(&mut *first, &mut *second);
}

fn swap_buffer_view_with_fresh(mut view: BufferManagerMut<'_>) {
    let mut fresh = BufferManager::new();
    std::mem::swap(&mut *view, &mut fresh);
}

fn swap_two_edit_views(mut first: EditRuntimeMut<'_>, mut second: EditRuntimeMut<'_>) {
    std::mem::swap(&mut *first, &mut *second);
}

fn unwrap_manager_does_not_exist(view: BufferManagerMut<'_>) {
    let _ = view.into_manager();
}

fn unwrap_runtime_does_not_exist(edit: EditRuntimeMut<'_>) {
    let _ = edit.into_runtime();
}

fn main() {}
