use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;
use contracts::OwnedNativeData;

fn main() {
    with_fixture(|fixture| {
        let native = String::from("borrowed host data");
        let _request = fixture.message(OwnedNativeData::Text(native.as_str()));
    });
}
