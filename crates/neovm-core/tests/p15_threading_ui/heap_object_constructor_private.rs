use neovm_core::tagged::header::{CondVarObj, MutexObj, ThreadObj};

// Non-diverging value producer so the compiler reaches every constructor call
// (a diverging argument would skip the rest of the keyed checks).
fn make<T>() -> T {
    loop {}
}

fn constructors() {
    // The type names are legitimately public: the thread manager casts heap
    // cells to them from another crate. The construction boundary is what this
    // contract pins, and every constructor stays crate-private.
    let _thread = ThreadObj::new(make());
    let _mutex = MutexObj::new(make());
    let _condition = CondVarObj::new(make());
}

fn main() {}
