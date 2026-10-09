#[path = "../../src/tls_scope.rs"]
mod tls_scope;

use std::cell::Cell;

thread_local! {
    static ACTIVE: Cell<u32> = const { Cell::new(0) };
}

fn main() {
    let scope = tls_scope::TlsScope::new(&ACTIVE, 1);
    std::thread::spawn(move || drop(scope));
}
