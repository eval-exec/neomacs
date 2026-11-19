#[path = "support/activation_owner.rs"]
mod owner;

use owner::activation::OwnedLocalMain;

fn main() {
    let _forged = OwnedLocalMain::new();
}
