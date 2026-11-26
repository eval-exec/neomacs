use neovm_value::tagged::collection_reads::SourceObservationsDetached;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireClone::<SourceObservationsDetached>::check();
}
