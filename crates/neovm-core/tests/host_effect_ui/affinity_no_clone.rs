use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;

fn main() {
    with_fixture(|mut fixture| {
        let _copy = fixture.affinity().clone();
    });
}
