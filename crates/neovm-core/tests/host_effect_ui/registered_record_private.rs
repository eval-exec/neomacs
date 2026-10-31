use neovm_core::host_effect::contracts;

use contracts::contract_fixture::with_fixture;

fn main() {
    with_fixture(|fixture| {
        let (plan, _) = fixture.message(contracts::OwnedNativeData::None).into_parts();
        let _forged = contracts::RegisteredRootRef::<contracts::MessageArguments> {
            binding: plan.record().binding,
            record: plan.record().record(),
            _kind: std::marker::PhantomData,
            _session: std::marker::PhantomData,
        };
    });
}
