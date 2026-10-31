use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;
use contracts::{HostClosure, HostResult, OwnedNativeData};

fn main() {
    with_fixture(|fixture| {
        let (_, port) = fixture.message(OwnedNativeData::None).into_parts();
        let _first = port.complete(HostResult::Closed(HostClosure::Closed));
        let _second = port.complete(HostResult::Closed(HostClosure::Closed));
    });
}
