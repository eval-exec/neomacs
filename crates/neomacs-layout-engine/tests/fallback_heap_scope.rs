//! Scope proof for the dedicated test-thread fallback heap (peer A-prime;
//! CI amendment 15:2xZ O1).
//!
//! The lazy per-thread `TaggedHeap` is compiled only under `cfg(test)` or
//! neovm-value's dedicated `test-thread-fallback-heap` feature, and the six
//! split crates' test trees arm it per process through the shared
//! `test_utils::init_test_tracing`.  This crate neither enables the feature
//! from its own dev-dependencies nor calls that init, so a Value construction
//! on a thread with no installed heap must still hit the production panic.
//! The property is deliberately independent of feature unification: a unified
//! `cargo nextest archive --workspace` build compiles the feature into every
//! dependent's graph, and the runtime arming flag is what keeps this tripwire
//! armed for a crate that never had the fallback pre-split.

#[test]
fn a_crate_without_the_fallback_feature_still_trips_the_no_heap_guard() {
    // A fresh thread has no installed heap.  Even in a unified workspace
    // build that compiles the fallback feature for this crate, this test
    // process never runs the split crates' test init, so the fallback stays
    // unarmed: `Value::vector` reaches `with_tagged_heap`'s panic arm.
    let handle = std::thread::spawn(|| {
        let _ = neovm_value::value::Value::vector(Vec::new());
    });
    let payload = handle
        .join()
        .expect_err("a fresh thread without an installed heap must panic");
    let message = payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("<non-string panic payload>");
    assert_eq!(
        message, "no TaggedHeap set for this thread",
        "the no-heap tripwire must keep its exact message in a crate that \
         never arms the test fallback"
    );
}
