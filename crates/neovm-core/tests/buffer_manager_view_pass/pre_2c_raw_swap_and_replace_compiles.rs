//! rt A3 2c (d) Pre-2c red-evidence pass control:
//!
//! Demonstrates that while raw `&mut EditRuntime` (and `pub Context.edit` /
//! `pub EditRuntime.buffers`) still exist, safe code holding `&mut EditRuntime`
//! can `std::mem::swap` it. Both `Context::buffer_manager_mut()` (#844 Slice 2a)
//! and all 7 raw window/frame split-borrow accessors (#844 Slice 2b) are now
//! retired from production builds.

use neovm_core::buffer::EditRuntime;

#[allow(dead_code)]
fn pre_2c_raw_edit_runtime_swap_compiles(first: &mut EditRuntime, second: &mut EditRuntime) {
    std::mem::swap(first, second);
}

fn main() {}
