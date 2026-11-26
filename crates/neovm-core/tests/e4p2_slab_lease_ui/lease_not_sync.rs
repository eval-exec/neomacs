use neovm_core::owner_slab_lease_contract::OwnerSlabLease;

fn require_sync<T: Sync>() {}

fn main() {
    require_sync::<OwnerSlabLease<'static, 'static, 'static>>();
}
