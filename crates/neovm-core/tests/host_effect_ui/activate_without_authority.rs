#[path = "support/activation_owner.rs"]
mod owner;

fn main() {
    let mut arm = owner::activation::SelectionActivation::dormant();
    let _flipped = arm.activate_local();
}
