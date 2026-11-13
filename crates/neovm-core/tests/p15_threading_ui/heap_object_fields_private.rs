use neovm_core::tagged::header::{CondVarObj, MutexObj, ThreadObj};

// Non-diverging value producer so every field expression is type-checked and
// the compiler reports the private fields instead of stopping at the first
// diverging initializer.
fn make<T>() -> T {
    loop {}
}

fn fields() {
    // The fields are crate-private too, so a struct literal cannot build the
    // object either. This is the sibling half of the construction contract;
    // the constructor half lives in heap_object_constructor_private.rs.
    let _thread = ThreadObj {
        header: make(),
        payload: make(),
    };
    let _mutex = MutexObj {
        header: make(),
        payload: make(),
    };
    let _condition = CondVarObj {
        header: make(),
        payload: make(),
    };
}

fn main() {}
