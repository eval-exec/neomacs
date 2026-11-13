#[path = "../../world/sole_owner.rs"]
mod sole_owner;

use sole_owner::SoleContextConstruction;

fn main() {
    let construction = SoleContextConstruction::new();
    let registration = construction.registration();
    let _vm = registration.into_vm();
    let _duplicate = registration.into_vm();
}
