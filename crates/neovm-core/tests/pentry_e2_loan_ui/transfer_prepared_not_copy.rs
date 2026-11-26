use neovm_value::tagged::gc::heap_transfer::PreparedHeapTransfer;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireCopy::<PreparedHeapTransfer>::check();
}
