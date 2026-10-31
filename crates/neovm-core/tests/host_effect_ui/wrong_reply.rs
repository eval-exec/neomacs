use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;
use contracts::{HostResult, OwnedNativeData};

fn main() {
    with_fixture(|fixture| {
        let (_, port) = fixture.message(OwnedNativeData::None).into_parts();
        let (font, _font_port) = fixture.font(OwnedNativeData::None).into_parts();
        let _response = port.complete(HostResult::Returned(fixture.font_reply_for(&font, OwnedNativeData::None)));
    });
}
