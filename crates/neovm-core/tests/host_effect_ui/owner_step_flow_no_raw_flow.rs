#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::OwnerStepResult;

fn cannot_transport_raw_flow(flow: neovm_core::Flow) {
    let _ = OwnerStepResult::Flow(flow);
}
fn main() {}
