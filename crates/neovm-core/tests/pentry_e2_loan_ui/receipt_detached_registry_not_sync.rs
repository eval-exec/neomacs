use neovm_value::heap_registry::detachment::DetachedRegistry;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireSync::<DetachedRegistry<String>>::check();
}
