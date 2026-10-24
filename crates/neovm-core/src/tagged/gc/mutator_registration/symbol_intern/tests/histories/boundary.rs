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
        let receipt = exercise(self, &holders, Scenario::SameName)?;
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
