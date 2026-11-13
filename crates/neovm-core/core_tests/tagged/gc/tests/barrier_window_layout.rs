use super::*;

/// MAIN390's old byte-tail offsets remain an explicit regression contract.
/// This is test evidence, not hardcoded generated-code offsets: native code
/// still obtains its two offsets through the owning layout constants above.
#[test]
fn compiled_exclusion_bytes_preserve_main390_heap_tail_layout() {
    crate::test_utils::init_test_tracing();
    // Copied offsets of the same private fields (VIS row
    // COMPILED_EXCLUSION_FIELD_OFFSETS_FOR_TEST); no field is widened.
    let old_offsets = TaggedHeap::COMPILED_EXCLUSION_FIELD_OFFSETS_FOR_TEST;
    let test_only_bytes = size_of::<usize>();
    let expected_main_offsets = [
        4512, 4513, 4514, 4515, 4516, 4517, 4518, 4519, 4520, 4521, 4522, 4523, 4524,
    ];
    assert_eq!(
        old_offsets.map(|offset| offset - test_only_bytes),
        expected_main_offsets,
        "account only for the existing test-only mapped_veclike_traces usize"
    );
    assert_eq!(HEAP_JIT_EXCLUSION_LO - test_only_bytes, 4525);
    assert_eq!(HEAP_JIT_EXCLUSION_LEN - test_only_bytes, 4533);
}
