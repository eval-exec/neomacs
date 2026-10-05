struct TaggedHeap;
impl TaggedHeap {
    fn identity(&self) -> usize { 1 }
}
#[path = "../../src/tagged/gc/scan_contract.rs"]
mod scan_contract;
fn main() {
    let mut heap = TaggedHeap;
    let _world = scan_contract::SingleMutatorWorld::from_heap(&mut heap);
}
