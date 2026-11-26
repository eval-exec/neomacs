//! The foreign-heap nest borrows the outer Context for the whole scope.
//!
//! A door on that Context inside the scope would be a second mutable borrow
//! (E0499). The scope's `Drop` is what keeps the first borrow alive, so the
//! Context stays unusable until the nest ends.

use neovm_core::Context;

fn outer_is_unusable_inside_the_nest(ctx: &mut Context) {
    let _scope = unsafe { ctx.nest_foreign_heap_for_test(std::ptr::NonNull::dangling()) };
    let _ = ctx.eval_str("1");
}

fn main() {}
