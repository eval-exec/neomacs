//! Thin compiler contracts for the dormant owned host-effect boundary.

#[test]
#[cfg(feature = "contract-fixtures")]
fn host_effect_compile_contracts() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/host_effect_pass/*.rs");
    cases.compile_fail("tests/host_effect_ui/*.rs");
}

#[test]
#[cfg(feature = "contract-fixtures")]
fn same_family_cross_reply_preserves_authority_and_payload() {
    use neovm_core::host_effect::contracts::contract_fixture::with_fixture;
    use neovm_core::host_effect::contracts::{HostResult, NativeTruth, OwnedNativeData};

    with_fixture(|fixture| {
        let (first, first_port) = fixture
            .message(OwnedNativeData::Text("first".into()))
            .into_parts();
        let (second, second_port) = fixture
            .message(OwnedNativeData::Text("second".into()))
            .into_parts();
        assert_ne!(first.record().request(), second.record().request());
        let second_reply =
            fixture.message_reply_for(&second, OwnedNativeData::Truth(NativeTruth::True));
        let rejected = match first_port.complete(HostResult::Returned(second_reply)) {
            Err(rejected) => rejected,
            Ok(_) => panic!("a same-family reply must not answer another request"),
        };
        let (first_port, second_result) = rejected.into_parts();
        let second_terminal = second_port
            .complete(second_result)
            .expect("preserved second result");
        assert_eq!(second_terminal.session(), second.record().session());
        assert_eq!(second_terminal.request(), second.record().request());
        let HostResult::Returned(second_reply) = second_terminal.into_result() else {
            panic!("rejection must preserve the reply kind");
        };
        assert_eq!(
            second_reply.native(),
            &OwnedNativeData::Truth(NativeTruth::True)
        );
        let first_terminal = first_port
            .complete(HostResult::Returned(
                fixture.message_reply_for(&first, OwnedNativeData::None),
            ))
            .expect("preserved first port");
        assert_eq!(first_terminal.request(), first.record().request());
    });
}

#[test]
#[cfg(feature = "contract-fixtures")]
fn rooted_error_and_signal_require_the_original_request() {
    use neovm_core::host_effect::contracts::contract_fixture::with_fixture;
    use neovm_core::host_effect::contracts::{HostClosure, HostResult, OwnedNativeData};

    with_fixture(|fixture| {
        let (first, first_port) = fixture.message(OwnedNativeData::None).into_parts();
        let (second, second_port) = fixture.message(OwnedNativeData::None).into_parts();
        let rejected =
            match first_port.complete(HostResult::LispError(fixture.message_error_for(&second))) {
                Err(rejected) => rejected,
                Ok(_) => panic!("foreign request error must be rejected"),
            };
        let (first_port, second_error) = rejected.into_parts();
        let terminal = second_port
            .complete(second_error)
            .expect("original error port");
        let HostResult::LispError(error) = terminal.into_result() else {
            panic!("error preserved")
        };
        assert_eq!(error.record().request(), second.record().request());

        let (third, third_port) = fixture.message(OwnedNativeData::None).into_parts();
        let rejected = match first_port
            .complete(HostResult::Interrupted(fixture.message_signal_for(&third)))
        {
            Err(rejected) => rejected,
            Ok(_) => panic!("foreign request signal must be rejected"),
        };
        let (first_port, third_signal) = rejected.into_parts();
        let terminal = third_port
            .complete(third_signal)
            .expect("original signal port");
        let HostResult::Interrupted(signal) = terminal.into_result() else {
            panic!("signal preserved")
        };
        assert_eq!(signal.record().request(), third.record().request());

        let terminal = first_port
            .complete(HostResult::Closed(HostClosure::Closed))
            .expect("session closure settles any port");
        assert_eq!(terminal.request(), first.record().request());
    });
}
