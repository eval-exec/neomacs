use neovm_value::tagged::gc::WorldLoan;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireSync::<WorldLoan>::check();
}
