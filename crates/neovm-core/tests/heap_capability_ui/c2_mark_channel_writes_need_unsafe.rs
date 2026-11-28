//! value-gc C2 regression guard (G1): `MarkExitSlot::{install_for_heap,
//! clear_for_test, clear_sentinel_for_test}`, `ConcurrentMarkFlag::set`,
//! `complete_for_heap`, and `retire_heap` require `unsafe` so safe code in
//! `tagged::gc` cannot install foreign completion channels, toggle the
//! concurrent-mark flag, or clear the active-mark side-table.
#![allow(unexpected_cfgs)] // mark_channel.rs gates its test helpers on cfg(trybuild) (#859)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct HeapIdentity(u64);
#[derive(Clone, Copy, Debug)]
struct MarkWord(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SymId(u32);
#[derive(Debug)]
enum MarkFinishError {
    MissingCompletion,
    Disconnected(std::sync::mpsc::RecvError),
    PoisonedCollectorState,
}
#[derive(Debug)]
struct ConcurrentMarkResult;
impl ConcurrentMarkResult {
    fn into_parts_for_heap(
        self,
        _expected: HeapIdentity,
    ) -> Result<(Vec<MarkWord>, Vec<usize>, Vec<SymId>), HeapIdentity> {
        Ok((Vec::new(), Vec::new(), Vec::new()))
    }
}

#[path = "../../../neovm-value/src/tagged/gc/mark_channel.rs"]
mod mark_channel;

fn main() {
    let (_tx, rx) = std::sync::mpsc::channel::<ConcurrentMarkResult>();
    let mut slot = mark_channel::MarkExitSlot::empty();
    slot.install_for_heap(HeapIdentity(1), rx);
    slot.clear_for_test();
    slot.clear_sentinel_for_test(HeapIdentity(1));
    let mut flag = mark_channel::ConcurrentMarkFlag::new();
    flag.set(true);
    mark_channel::complete_for_heap(HeapIdentity(1));
    mark_channel::retire_heap(HeapIdentity(1));
}
