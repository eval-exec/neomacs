use neovm_value::tagged::gc::heap_transfer::HeapRootDesignator;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireSend::<HeapRootDesignator>::check();
}
