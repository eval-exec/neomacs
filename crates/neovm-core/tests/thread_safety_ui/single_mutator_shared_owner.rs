struct TaggedHeap;
impl TaggedHeap {
    fn identity(&self) -> usize { 1 }
}
#[path = "../../src/tagged/gc/scan_contract.rs"]
mod scan_contract;
fn main() {
    let heap = TaggedHeap;
    // SAFETY: the fixture isolates the constructor's exclusive-borrow contract.
    let _world = unsafe { scan_contract::SingleMutatorWorld::from_heap(&heap) };
}
