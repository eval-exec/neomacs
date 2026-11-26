use neovm_core::owner_slab_lease_contract::OwnerSlabHandle;
use neovm_value::tagged::gc::Mutator;

#[allow(dead_code)]
fn sequential_leases_compile(handle: &mut OwnerSlabHandle, mutator: &mut Mutator<'_>) {
    let mut run = mutator.run_access().unwrap();
    {
        let first = handle.lease(&mut run).unwrap();
        let _ = first.heap_identity();
    }
    {
        let second = handle.lease(&mut run).unwrap();
        let _ = second.heap_identity();
    }
}

fn main() {}
