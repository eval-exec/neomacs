struct TaggedHeap;
impl TaggedHeap {
    fn identity(&self) -> usize { 1 }
}
#[path = "../../src/tagged/gc/scan_contract.rs"]
mod scan_contract;
fn main() {
    let mut heap = TaggedHeap;
    // SAFETY: the fixture isolates the lifetime of exclusive capture admission.
    let world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&mut heap) };
    let _identity = heap.identity();
    let _captured_identity = world.heap_identity();
}
