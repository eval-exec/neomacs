use neovm_value::tagged::collection_reads::ObservationPlaceholderGuard;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireClone::<ObservationPlaceholderGuard>::check();
}
