//! value-gc C2 regression guard (G1): `ConcurrentMarkResult::from_parts`
//! requires `unsafe` so safe code in `tagged::gc` cannot mint a worker result
//! carrying deferred `MarkWord`s or promotion addresses.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HeapIdentity(u64);
#[derive(Clone, Copy, Debug)]
struct MarkWord(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SymId(u32);

#[path = "../../../neovm-value/src/tagged/gc/concurrent_mark_result.rs"]
mod concurrent_mark_result;

fn main() {
    let _ = concurrent_mark_result::ConcurrentMarkResult::from_parts(
        HeapIdentity(1),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
}
