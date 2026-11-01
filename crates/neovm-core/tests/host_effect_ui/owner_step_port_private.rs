#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::{CallbackStep, OwnerResumePort, OwnerStepKey};
use std::marker::PhantomData;

fn cannot_mint_from_numeric_key(key: OwnerStepKey<u64>) {
    let _ = OwnerResumePort::<CallbackStep, u64> {
        key,
        _kind: PhantomData,
        _requester: PhantomData,
    };
}
fn main() {}
