//! A scalar test bridge; reuse the qualified owner/root/retirement histories.
use super::*;
use neovm_eval::test_support::authority::WorldProofEra as ParallelHeapEra;

pub(super) fn force_spine_move(
    era: &ParallelHeapEra<'_>,
    holder: SymbolHandle,
    nil: &SharedRoot,
    id: SymId,
    before: &InitialInternFacts,
) -> Result<usize, String> {
    use crate::symbol::parallel::test_facts::InitialInternFault;
    guarded(
        activate(
            era.register(9100, 3)
                .map_err(|e| format!("C5 relocation permit: {e:?}"))?,
            era.heap_identity(),
            9100,
        )?,
        |owner| {
            let mut homes = seed(owner, &holder, nil)?;
            attempt(owner, "arm spine relocation", |run| {
                Ok(run.arm_initial_fault_for_test(InitialInternFault::ReservationAfterSpineMove)?)
            })?;
            let refused = attempt(owner, "clean relocating refusal", |run| {
                match run.initial_intern_into(&homes.name, &mut homes.dst) {
                    Err(InitialInternError::Authority(RegistrationError::Busy)) => {
                        Err(RegistrationError::Busy.into())
                    }
                    outcome => Ok(outcome),
                }
            })?;
            let (after, charge) = attempt(owner, "relocation facts", |run| {
                Ok((
                    run.initial_facts_copy_for_test(id)?,
                    run.hot_bytes_since_gc_for_test()
                        .ok_or(Failure::Fixture("relocation owner absent"))?,
                ))
            })?;
            if refused != Err(InitialInternError::Authority(RegistrationError::Capacity))
                || before.spine_address() == after.spine_address()
                || !after.spine_current()
                || (
                    before.members(),
                    before.epoch(),
                    before.slots(),
                    before.chunks(),
                    before.sides(),
                ) != (
                    after.members(),
                    after.epoch(),
                    after.slots(),
                    after.chunks(),
                    after.sides(),
                )
            {
                return Err(format!(
                    "C5 clean relocation facts mismatch: {refused:?}; {before:?} -> {after:?}"
                ));
            }
            eprintln!("P3SYM_C5_SERIAL_RELOCATION_REFUSAL capacity=1 moved=1 logical_change=0");
            Ok(charge)
        },
    )
}
