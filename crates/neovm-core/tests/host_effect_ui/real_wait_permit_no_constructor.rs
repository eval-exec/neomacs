#[path = "support/wake_owner.rs"]
mod owner;

use owner::wake::native_wait::RealWaitPermit;

fn main() {
    let _forged = RealWaitPermit::new();
}
