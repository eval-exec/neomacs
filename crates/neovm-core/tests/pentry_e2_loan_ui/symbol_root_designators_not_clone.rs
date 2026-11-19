use neovm_value::tagged::gc::SymbolRootDesignators;

#[path = "support/loan_bounds.rs"]
mod loan_bounds;

fn main() {
    loan_bounds::RequireClone::<SymbolRootDesignators>::check();
}
