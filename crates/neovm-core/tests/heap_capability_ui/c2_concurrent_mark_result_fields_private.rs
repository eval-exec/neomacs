//! value-gc C2 regression guard (G1): `ConcurrentMarkResult` fields are private
//! so sibling modules in `tagged::gc` cannot construct a struct literal safely.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HeapIdentity(u64);
#[derive(Clone, Copy, Debug)]
struct MarkWord(usize);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SymId(u32);

#[path = "../../../neovm-value/src/tagged/gc/concurrent_mark_result.rs"]
mod concurrent_mark_result;

fn main() {
    let _forged = concurrent_mark_result::ConcurrentMarkResult {
        heap: HeapIdentity(1),
        deferred: Vec::new(),
        promo: Vec::new(),
        symbols: Vec::new(),
    };
}
