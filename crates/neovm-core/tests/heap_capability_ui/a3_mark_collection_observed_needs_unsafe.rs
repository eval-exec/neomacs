// value-gc A3: `mark_collection_observed(bits)` dereferences arbitrary tagged
// pointer bits as `*const GcHeader`/`*const StringObj` for non-cons tags, so
// calling the test-support wrapper without `unsafe` must be rejected.

fn main() {
    let _ = neovm_value::test_support::authority::mark_collection_observed(0x1000 | 0b101);
}
