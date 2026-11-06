//! The stop-the-world first partition cycle traces each image veclike once.
//! Its flat seed traces every one of them, since the roots need not reach
//! them all; the mark then found each reached one still unmarked and traced
//! it again. With the image pre-marked the mark skips them, and a heap list
//! that only an image vector holds still survives.
//! (Re-homed from eval's pdump_test.rs, jitcall BOOT-FULL pdump batch C,
//! under the peer's option (a) ruling: the mapped-veclike trace counter is
//! carried by test-support builds and exposed through the VIS-VALUE row for
//! TaggedHeap::mapped_veclike_traces_for_test.)
//
// API-drift shims (post-split re-home): the pre-split test reached
// `Context`, `dump_to_file` and `load_from_dump` through the pdump test
// module; the evaluator is now built through the core test facade,
// `load_from_dump` lives in core's load_boot, and the obarray/tagged-heap
// probes go through their public accessors.
use crate::emacs_core::format_eval_result;
use crate::emacs_core::load_boot::load_from_dump;
use crate::emacs_core::pdump::dump_to_file;
use neovm_value::value::Value;

#[test]
fn the_stop_the_world_first_cycle_traces_each_image_veclike_once() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str("(defvar premark-probe (vector 'a 'b))")
        .expect("defvar should evaluate");
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("premark.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    let vector = loaded
        .obarray()
        .symbol_value_copied("premark-probe")
        .expect("restored vector");
    assert!(
        loaded.tagged_heap().mapped_image_owns_for_test(vector),
        "the vector must live in the mapped image for this test to mean anything"
    );
    assert!(vector.set_vector_slot(0, Value::list(vec![Value::fixnum(3), Value::fixnum(4)])));
    // GC stress collects at load-time safe points, before this test gets
    // the first cycle.
    let first_cycle = loaded.tagged_heap().is_partition_first_cycle();
    assert!(first_cycle || std::env::var_os("NEOVM_GC_STRESS").is_some());
    let (traces_before, _) = loaded.tagged_heap().mapped_veclike_traces_for_test();
    loaded.gc_collect_exact();
    let (traces_after, mapped) = loaded.tagged_heap().mapped_veclike_traces_for_test();
    if first_cycle {
        assert_eq!(
            traces_after - traces_before,
            mapped,
            "the first cycle must trace each image veclike exactly once"
        );
    }
    for round in 0..2 {
        let held = loaded
            .eval_str("(aref premark-probe 0)")
            .expect("read the image vector");
        assert_eq!(
            format_eval_result(&Ok(held)),
            "OK (3 4)",
            "after GC {} the list only the image holds must be intact",
            round + 1
        );
        loaded.gc_collect_exact();
    }
}
