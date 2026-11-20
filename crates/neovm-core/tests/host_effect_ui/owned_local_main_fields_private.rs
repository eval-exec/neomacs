#[path = "support/activation_owner.rs"]
mod owner;

use owner::activation::{ActivationCapability, OwnedLocalMain};

fn main() {
    let _forged = OwnedLocalMain {
        _capability: ActivationCapability {
            _confined: std::marker::PhantomData,
        },
    };
}
