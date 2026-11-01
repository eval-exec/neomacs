#[path = "../host_effect_ui/support/owner_step_owner.rs"]
mod owner;
use owner::contracts::{CallbackStepResult, RegisteredRootRef};
use owner::owner_step::{
    ClaimedOwnerStep, OwnerStepCompletion, OwnerStepKey, OwnerStepResult, PendingOwnerStep,
    RejectedOwnerStep,
};

// Check the real consuming API without inventing registered roots or ports.
// Runtime tests separately supply the registry's exact owner/parent authority.
fn claim_registered_callback<'s>(
    pending: PendingOwnerStep<'s, u64>,
    expected: OwnerStepKey<u64>,
) -> Result<ClaimedOwnerStep<'s, u64>, RejectedOwnerStep<PendingOwnerStep<'s, u64>>> {
    pending.claim(expected)
}

fn complete_registered_callback<'s>(
    claim: ClaimedOwnerStep<'s, u64>,
    result: RegisteredRootRef<'s, CallbackStepResult>,
) -> Result<
    OwnerStepCompletion<'s, u64>,
    RejectedOwnerStep<(ClaimedOwnerStep<'s, u64>, OwnerStepResult<'s>)>,
> {
    claim.complete(OwnerStepResult::Callback(result))
}
fn main() {}
