use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;

fn main() {
    let _escaped = with_fixture(|fixture| fixture.message(contracts::OwnedNativeData::None));
}
