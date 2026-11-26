use neovm_core::owner_slab_lease_contract::{OwnerSlabHandle, OwnerSlabLease};

fn accept_local<T>() {}

fn main() {
    accept_local::<OwnerSlabHandle>();
    accept_local::<OwnerSlabLease<'static, 'static, 'static>>();
}
