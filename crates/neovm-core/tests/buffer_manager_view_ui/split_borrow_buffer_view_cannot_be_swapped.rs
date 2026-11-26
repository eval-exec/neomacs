//! rt A3 2c (d) Fixture 2: the buffer-manager half of a split borrow
//! (`Context::frame_manager_and_buffer_view_mut`) cannot be `std::mem::swap`ped.
//!
//! Bindings are declared `mut` (`#![allow(unused_mut)]` keeps `.stderr` clean)
//! so the SOLE reason compilation fails is the missing `DerefMut`.
#![allow(unused_mut)]

use neovm_core::emacs_core::eval::Context;

fn swap_split_borrow_buffers(first: &mut Context, second: &mut Context) {
    let (_frames_a, mut bm_a) = first.frame_manager_and_buffer_view_mut();
    let (_frames_b, mut bm_b) = second.frame_manager_and_buffer_view_mut();
    std::mem::swap(&mut *bm_a, &mut *bm_b);
}

fn main() {}
