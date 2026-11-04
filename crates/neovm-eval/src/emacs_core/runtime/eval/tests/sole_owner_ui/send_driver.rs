#[path = "../../world/sole_owner.rs"]
mod sole_owner;

use sole_owner::{SoleContextConstruction, SoleOwnerDriver};

#[path = "../../../../../../tests/thread_safety_ui/support/thread_bounds.rs"]
mod thread_bounds;
use thread_bounds::ThreadTransfer;

fn main() {
    let construction = SoleContextConstruction::new();
    let vm = construction.registration().into_vm();
    let driver = vm.driver();
    ThreadTransfer::<SoleOwnerDriver>::transfer(driver);
}
