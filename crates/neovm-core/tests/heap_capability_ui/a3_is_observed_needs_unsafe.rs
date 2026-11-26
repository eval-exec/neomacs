// value-gc A3: `collection_reads::is_observed(bits)` dereferences arbitrary
// tagged pointer bits as `*const GcHeader` for non-cons tags, so calling it
// without `unsafe` must be rejected at compile time.

fn main() {
    let _ = neovm_value::tagged::collection_reads::is_observed(0x1000 | 0b101);
}
