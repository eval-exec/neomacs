#[path = "support/owner_step_owner.rs"]
mod owner;
use owner::owner_step::OwnerStepResult;

fn cannot_transport_raw_value(value: neovm_core::Value) {
    let _ = OwnerStepResult::Callback(value);
}
fn main() {}
