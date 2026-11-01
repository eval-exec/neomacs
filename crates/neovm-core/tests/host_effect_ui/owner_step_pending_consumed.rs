#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::{OwnerStepKey, PendingOwnerStep};

fn cannot_claim_twice(pending: PendingOwnerStep<'static, u64>, key: OwnerStepKey<u64>) {
    let _ = pending.claim(key);
    let _ = pending.claim(key);
}
fn main() {}
