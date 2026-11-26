use neovm_core::owner_slab_lease_contract::OwnerSlabHandle;

fn require_sync<T: Sync>() {}

fn main() {
    require_sync::<OwnerSlabHandle>();
}
