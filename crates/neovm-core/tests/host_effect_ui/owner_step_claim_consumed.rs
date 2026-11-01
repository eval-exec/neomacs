#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::{ClaimedOwnerStep, OwnerStepResult};

fn cannot_complete_twice(
    claim: ClaimedOwnerStep<'static, u64>,
    first: OwnerStepResult<'static>,
    second: OwnerStepResult<'static>,
) {
    let _ = claim.complete(first);
    let _ = claim.complete(second);
}
fn main() {}
