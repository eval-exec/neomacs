#[path = "../../world/sole_owner.rs"]
mod sole_owner;

use sole_owner::SoleContextConstruction;

fn main() {
    // This closed fixture factory includes the production pointer-free module;
    // it does not implement a second phase algorithm or a manager accessor.
    let construction = SoleContextConstruction::new();
    let vm = construction.registration().into_vm();
    let driver = vm.driver();
    let published = driver.into_published();
    let rejoined = published.into_running();
    drop(rejoined);
}
