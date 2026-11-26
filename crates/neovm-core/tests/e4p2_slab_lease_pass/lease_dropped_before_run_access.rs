use neovm_core::owner_slab_lease_contract::OwnerSlabHandle;
use neovm_value::tagged::gc::Mutator;

#[allow(dead_code)]
fn lease_dropped_before_run_access(handle: &mut OwnerSlabHandle, mutator: &mut Mutator<'_>) {
    let mut run = mutator.run_access().unwrap();
    let lease = handle.lease(&mut run).unwrap();
    let _ = lease.heap_identity();
    drop(lease);
    drop(run);
}

fn main() {}
