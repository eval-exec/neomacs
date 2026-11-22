use neovm_value::error::SourceShellInFlightReceipt;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireCopy::<SourceShellInFlightReceipt>::check();
}
