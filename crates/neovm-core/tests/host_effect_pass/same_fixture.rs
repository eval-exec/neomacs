use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;
use contracts::{HostRequest, HostResponse, HostResult, NativeTruth, OwnedNativeData};

fn send<T: Send>(_: T) {}

fn main() {
    with_fixture(|mut fixture| {
        let _affinity = fixture.affinity();
        let submitted = fixture.message(OwnedNativeData::Text(String::from("same owner")));
        let request = HostRequest::Message(submitted);
        let HostRequest::Message(submitted) = request else { unreachable!() };
        let (plan, reply) = submitted.into_parts();
        assert!(matches!(plan.native(), OwnedNativeData::Text(_)));
        let result = fixture.message_reply_for(&plan, OwnedNativeData::Truth(NativeTruth::True));
        let terminal = reply.complete(HostResult::Returned(result)).expect("matching request");
        assert_eq!(terminal.session(), plan.record().session());
        assert_eq!(terminal.request(), plan.record().request());
        send(HostResponse::Message(terminal));
    });
}
