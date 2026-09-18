//! An explicit `(garbage-collect)` that arrives while the session's armed
//! concurrent first partition cycle is open, driven through the collector's
//! own entry points: a safe point arms and starts the concurrent cycle, and
//! the forced path must then finish the bootstrap with an exact trace of the
//! image. It used to stage the image's children for a GC thread that a
//! stop-the-world mark never starts, sweep a heap object that only an
//! unreachable image object held, and blacken the image around the dangling
//! pointer.

use crate::emacs_core::pdump::{dump_to_file, load_from_dump};
use crate::emacs_core::value::Value;

#[test]
fn garbage_collect_during_the_concurrent_first_cycle_keeps_what_only_the_image_holds() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::eval::Context::new();
    eval.eval_str("(defvar forced-first-cycle-probe (vector 'a 'b))")
        .expect("defvar should evaluate");
    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("forced-first-cycle.pdump");
    dump_to_file(&eval, &dump_path).expect("dump should succeed");
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");
    if loaded.gc_stress {
        // Stress mode collects synchronously at every safe point and never
        // arms the concurrent first cycle.
        return;
    }
    let vector = *loaded
        .obarray
        .symbol_value("forced-first-cycle-probe")
        .expect("restored vector");
    assert!(
        loaded.tagged_heap.mapped_image_owns_for_test(vector),
        "the vector must live in the mapped image for this test to mean anything"
    );
    let table = loaded
        .eval_str("(make-hash-table)")
        .expect("make a heap table");
    assert!(vector.set_vector_slot(0, table));
    // Only the image vector holds the table now, and nothing reaches the
    // vector.
    loaded
        .obarray
        .set_symbol_value("forced-first-cycle-probe", Value::NIL);
    assert!(loaded.tagged_heap.is_partition_first_cycle());

    // A safe point arms and starts the concurrent first cycle ...
    loaded.gc_collect_from_current_roots_impl(false);
    assert!(
        loaded.tagged_heap.concurrent_mark_running(),
        "the safe point must have started the concurrent first cycle"
    );
    // ... and `(garbage-collect)` arrives while it is open.
    loaded.gc_collect_exact();
    assert!(!loaded.tagged_heap.is_partition_first_cycle());
    assert!(
        loaded.tagged_heap.owns_heap_value_for_test(table),
        "the forced cycle must not free a table only an image object holds"
    );
    // The blackened image keeps it through a later cycle too.
    loaded.gc_collect_exact();
    assert!(loaded.tagged_heap.owns_heap_value_for_test(table));
}
