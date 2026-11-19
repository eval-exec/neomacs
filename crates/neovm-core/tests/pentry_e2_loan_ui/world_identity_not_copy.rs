use neovm_value::tagged::gc::WorldIdentity;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireCopy::<WorldIdentity>::check();
}
