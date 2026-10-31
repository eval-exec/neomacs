use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;

fn main() {
    with_fixture(|fixture| {
        let _request = fixture.message(neovm_core::Value::NIL);
    });
}
