#[path = "support/activation_owner.rs"]
mod owner;

use owner::activation::ActivationAuthority;

fn main() {
    let _forged = ActivationAuthority::new();
}
