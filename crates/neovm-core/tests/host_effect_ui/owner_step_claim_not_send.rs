#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::ClaimedOwnerStep;

fn require_send<T: Send>() {}
fn main() {
    require_send::<ClaimedOwnerStep<'static, u64>>();
}
