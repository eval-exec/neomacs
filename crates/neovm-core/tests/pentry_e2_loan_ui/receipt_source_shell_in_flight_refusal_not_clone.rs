use neovm_value::error::SourceShellInFlightRefusal;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireClone::<SourceShellInFlightRefusal>::check();
}
