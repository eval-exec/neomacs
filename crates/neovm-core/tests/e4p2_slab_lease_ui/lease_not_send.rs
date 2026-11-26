use neovm_core::owner_slab_lease_contract::OwnerSlabLease;

fn require_send<T: Send>() {}

fn main() {
    require_send::<OwnerSlabLease<'static, 'static, 'static>>();
}
