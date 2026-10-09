struct TaggedHeap;
impl TaggedHeap {
    fn identity(&self) -> usize { 1 }
}
#[path = "../../src/tagged/gc/scan_contract.rs"]
mod scan_contract;
fn require_sync<T: Sync>() {}
fn main() {
    require_sync::<scan_contract::ScanStorageLease>();
}
