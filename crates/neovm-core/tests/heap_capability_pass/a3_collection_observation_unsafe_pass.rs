// Positive control for value-gc A3: calling `is_observed`, `collection_observed`,
// and `mark_collection_observed` inside `unsafe` with non-dereferencing zero
// bits compiles cleanly.

fn main() {
    // SAFETY: address 0 does not dereference any heap header.
    let _ = unsafe { neovm_value::tagged::collection_reads::is_observed(0) };
    // SAFETY: address 0 does not dereference any heap header.
    let _ = unsafe { neovm_value::tagged::gc::collection_observed(0) };
    // SAFETY: address 0 does not dereference any heap header.
    let _ = unsafe { neovm_value::test_support::authority::mark_collection_observed(0) };
}
