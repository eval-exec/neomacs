struct TaggedHeap;
impl TaggedHeap {
    fn identity(&self) -> usize { 1 }
}
#[path = "../../src/tagged/gc/scan_contract.rs"]
mod scan_contract;
fn require_send<T: Send>() {}
fn main() {
    require_send::<scan_contract::SingleMutatorWorld<'static>>();
}
