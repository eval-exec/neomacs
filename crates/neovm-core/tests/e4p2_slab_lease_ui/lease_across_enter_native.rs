use neovm_core::owner_slab_lease_contract::OwnerSlabHandle;
use neovm_value::tagged::gc::Mutator;

fn lease_cannot_cross_enter_native(handle: &mut OwnerSlabHandle, mutator: &mut Mutator<'_>) {
    let mut run = mutator.run_access().unwrap();
    let lease = handle.lease(&mut run).unwrap();
    let _ = mutator.enter_native(&[]);
    let _ = lease.heap_identity();
}

fn main() {}
