use neovm_core::owner_slab_lease_contract::OwnerSlabHandle;

fn require_send<T: Send>() {}

fn main() {
    require_send::<OwnerSlabHandle>();
}
