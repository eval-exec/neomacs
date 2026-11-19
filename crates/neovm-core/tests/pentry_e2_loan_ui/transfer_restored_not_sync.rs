use neovm_value::tagged::gc::heap_transfer::RestoredHeapBacking;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireSync::<RestoredHeapBacking>::check();
}
