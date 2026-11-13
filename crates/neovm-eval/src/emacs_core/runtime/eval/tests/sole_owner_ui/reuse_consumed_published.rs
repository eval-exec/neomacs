#[path = "../../world/sole_owner.rs"]
mod sole_owner;

use sole_owner::SoleContextConstruction;

fn main() {
    let construction = SoleContextConstruction::new();
    let vm = construction.registration().into_vm();
    let published = vm.driver().into_published();
    let _running = published.into_running();
    let _stale = published.into_running();
}
