use neovm_value::heap_registry::text_retirement::PreparedValueCacheRetirement;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireClone::<PreparedValueCacheRetirement<'static>>::check();
}
