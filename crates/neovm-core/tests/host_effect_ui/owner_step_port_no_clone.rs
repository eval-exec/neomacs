#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::{CallbackStep, OwnerResumePort};

fn require_clone<T: Clone>() {}
fn main() {
    require_clone::<OwnerResumePort<CallbackStep, u64>>();
}
