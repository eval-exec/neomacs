use neovm_core::owner_slab_lease_contract::OwnerSlabHandle;
use neovm_value::tagged::gc::Mutator;

fn second_lease_cannot_overlap_first(handle: &mut OwnerSlabHandle, mutator: &mut Mutator<'_>) {
    let mut run = mutator.run_access().unwrap();
    let first = handle.lease(&mut run).unwrap();
    let second = handle.lease(&mut run).unwrap();
    let _ = (first.heap_identity(), second.heap_identity());
}

fn main() {}
