use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;

fn require_send<T: Send>(_: T) {}

fn main() {
    with_fixture(|mut fixture| require_send(fixture.affinity()));
}
