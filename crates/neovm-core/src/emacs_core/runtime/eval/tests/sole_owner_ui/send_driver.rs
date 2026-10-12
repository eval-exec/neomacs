#[path = "../../world/sole_owner.rs"]
mod sole_owner;

use sole_owner::SoleContextConstruction;

fn main() {
    let construction = SoleContextConstruction::new();
    let vm = construction.registration().into_vm();
    let driver = vm.driver();
    std::thread::spawn(move || drop(driver));
}
