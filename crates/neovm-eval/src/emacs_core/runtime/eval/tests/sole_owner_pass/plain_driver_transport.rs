#[path = "../../world/sole_owner.rs"]
mod sole_owner;

use sole_owner::{SoleContextConstruction, SoleOwnerDriver};

fn returned_driver() -> SoleOwnerDriver {
    let construction = SoleContextConstruction::new();
    let vm = construction.registration().into_vm();
    vm.driver()
}

fn combine(_first: SoleOwnerDriver, _second: SoleOwnerDriver) {}

fn main() {
    // Plain policy tokens can leave their issuance function and combine with
    // another activation's token. Neither has manager access or owner identity.
    combine(returned_driver(), returned_driver());
}
