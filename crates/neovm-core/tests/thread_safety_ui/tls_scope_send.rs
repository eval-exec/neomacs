#[path = "../../../neovm-value/src/tls_scope.rs"]
mod tls_scope;

use std::cell::Cell;

#[path = "support/thread_bounds.rs"]
mod thread_bounds;
use thread_bounds::ThreadTransfer;

thread_local! {
    static ACTIVE: Cell<u32> = const { Cell::new(0) };
}

fn main() {
    let scope = tls_scope::TlsScope::new(&ACTIVE, 1);
    ThreadTransfer::<tls_scope::TlsScope<u32, Cell<u32>>>::transfer(scope);
}
