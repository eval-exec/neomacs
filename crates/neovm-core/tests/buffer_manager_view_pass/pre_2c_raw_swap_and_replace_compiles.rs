//! rt A3 2c (d) Pre-2c red-evidence pass control:
//!
//! Demonstrates that while raw `&mut EditRuntime` and the remaining window/frame
//! split-borrow raw accessors still exist (see #844 Slices 2b/2c), safe code
//! can `std::mem::swap` them. `Context::buffer_manager_mut()` itself is now
//! gated behind `#[cfg(any(test, feature = "test-support"))]` (#844 Slice 2a),
//! making it unavailable in non-test production builds.

use neovm_core::buffer::EditRuntime;
use neovm_core::emacs_core::eval::Context;

#[allow(dead_code)]
fn pre_2c_raw_edit_runtime_swap_compiles(first: &mut EditRuntime, second: &mut EditRuntime) {
    std::mem::swap(first, second);
}

#[allow(dead_code)]
fn pre_2c_raw_split_borrow_swap_compiles(first: &mut Context, second: &mut Context) {
    let (_frames_a, bm_a) = first.frame_manager_and_buffers_mut();
    let (_frames_b, bm_b) = second.frame_manager_and_buffers_mut();
    std::mem::swap(bm_a, bm_b);
}

fn main() {}
