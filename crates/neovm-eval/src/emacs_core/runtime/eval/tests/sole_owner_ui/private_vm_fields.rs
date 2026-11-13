#[path = "../../world/sole_owner.rs"]
mod sole_owner;

fn main() {
    let _forged = sole_owner::SoleOwnerVm {
        _confined: std::marker::PhantomData,
    };
}
