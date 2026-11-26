use neovm_core::owner_slab_lease_contract::OwnerSlabHandle;
use neovm_value::tagged::gc::Mutator;

#[allow(dead_code)]
fn lease_dropped_before_mutator_poll(handle: &mut OwnerSlabHandle, mutator: &mut Mutator<'_>) {
    {
        let mut run = mutator.run_access().unwrap();
        let lease = handle.lease(&mut run).unwrap();
        let _ = lease.heap_identity();
    }
    let _ = mutator.poll(|| Ok(Vec::new()));
}

fn main() {}
