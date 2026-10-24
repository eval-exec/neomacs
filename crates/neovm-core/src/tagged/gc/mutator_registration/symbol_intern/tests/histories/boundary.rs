//! A scalar test bridge; reuse the qualified owner/root/retirement histories.
use super::*;

pub(crate) struct InternGrowthReceipt {
    pub(crate) before: InitialInternFacts,
    pub(crate) after: InitialInternFacts,
    pub(crate) winner: SymId,
    pub(crate) registry_before: (u64, usize),
    pub(crate) registry_after: (u64, usize),
}

impl ParallelHeapEra<'_> {
    /// Only existing input-holder identities enter. All owners and operand
    /// homes retire before any receipt or assertion outcome returns.
    pub(crate) fn c5_intern_growth_for_test(
        &self,
        holders: [SymId; 2],
    ) -> Result<InternGrowthReceipt, String> {
        self.c5_growth_receipt(holders, false)
    }

    /// First force the qualified clean reservation refusal to replace the
    /// pointer spine. Its reserved capacity covers the later winning ID.
    pub(crate) fn c5_intern_relocating_for_test(
        &self,
        holders: [SymId; 2],
    ) -> Result<InternGrowthReceipt, String> {
        self.c5_growth_receipt(holders, true)
    }

    fn c5_growth_receipt(
        &self,
        holders: [SymId; 2],
        relocate: bool,
    ) -> Result<InternGrowthReceipt, String> {
        let receipt = exercise_with_relocation(self, &holders, Scenario::SameName, relocate)?;
        // Return failures to the serial fixture rather than unwinding across
        // its finish capabilities. No formatter/Context enters this bridge.
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            assert_history(&receipt, Scenario::SameName, None, true);
        }))
        .map_err(|_| "C5 growth receipt assertion after owner retirement".to_owned())?;
        Ok(InternGrowthReceipt {
            before: receipt.before,
            after: receipt.after,
            winner: receipt.rows[0].id,
            registry_before: receipt.registry_before,
            registry_after: receipt.registry_after,
        })
    }
}

pub(super) fn force_spine_move(
    era: &ParallelHeapEra<'_>,
    holder: SymbolHandle,
    nil: &SharedRoot,
    id: SymId,
    before: &InitialInternFacts,
) -> Result<usize, String> {
    use crate::emacs_core::symbol::parallel::InitialInternFault;
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
                Ok(run.initial_fault_for_test(InitialInternFault::ReservationAfterSpineMove)?)
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
                    run.initial_facts_for_test(id)?,
                    run.owner
                        .local
                        .as_ref()
                        .ok_or(Failure::Fixture("relocation owner absent"))?
                        .hot
                        .bytes_since_gc,
                ))
            })?;
            if refused != Err(InitialInternError::Authority(RegistrationError::Capacity))
                || before.spine_address == after.spine_address
                || !after.spine_current
                || (
                    before.members,
                    before.epoch,
                    before.slots,
                    before.chunks,
                    before.sides,
                ) != (
                    after.members,
                    after.epoch,
                    after.slots,
                    after.chunks,
                    after.sides,
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
