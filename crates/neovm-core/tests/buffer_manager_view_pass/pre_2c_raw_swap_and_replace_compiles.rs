//! rt A3 2c (d) Pre-2c red-evidence pass control (Commit A):
//!
//! Proves that BEFORE Commit B migrates callers off the raw `&mut BufferManager`
//! and `&mut EditRuntime` accessors, safe code CAN `std::mem::swap` two
//! contexts' `BufferManager`s or `EditRuntime`s (both directly and through a
//! split borrow) AND `std::mem::replace` a `Buffer` element with a standalone
//! buffer. This control demonstrates the exact soundness hole that
//! `BufferManagerMut` and `EditRuntimeMut` close.

use neovm_core::buffer::{Buffer, BufferId};
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::value::Value;

#[allow(dead_code)]
fn pre_2c_raw_manager_swap_compiles(first: &mut Context, second: &mut Context) {
    std::mem::swap(first.buffer_manager_mut(), second.buffer_manager_mut());
}

#[allow(dead_code)]
fn pre_2c_raw_edit_runtime_swap_compiles(first: &mut Context, second: &mut Context) {
    std::mem::swap(first.edit_mut(), second.edit_mut());
}

#[allow(dead_code)]
fn pre_2c_raw_split_borrow_swap_compiles(first: &mut Context, second: &mut Context) {
    let (_frames_a, bm_a) = first.frame_manager_and_buffers_mut();
    let (_frames_b, bm_b) = second.frame_manager_and_buffers_mut();
    std::mem::swap(bm_a, bm_b);
}

#[allow(dead_code)]
fn pre_2c_raw_buffer_element_replace_compiles(ctx: &mut Context, id: BufferId) {
    if let Some(slot) = ctx.buffer_manager_mut().get_mut(id) {
        let _ = std::mem::replace(slot, Buffer::new_standalone(id, Value::NIL));
    }
}

fn main() {}
