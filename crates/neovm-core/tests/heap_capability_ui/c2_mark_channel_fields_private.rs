//! value-gc C2 regression guard (G1): `MarkExitSlot` and `ConcurrentMarkFlag`
//! seal `TaggedHeap::{gc_exited, concurrent_mark_running}` with private inner
//! fields so sibling modules in `tagged::gc` cannot directly assign them.
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
    slot.0 = Some(rx);
    let mut flag = mark_channel::ConcurrentMarkFlag::new();
    flag.0 = true;
}
