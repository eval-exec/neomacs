//! Peer-young collection shell.
//! Serial setup admits a real collection era. The owner body and its
//! assertions stay in value. World roots are the baton walker.
use crate::emacs_core::eval::Context;
use crate::tagged::gc::mutator_registration::era_fixtures::EraFixtureExt;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;
use neovm_eval::test_support::authority::baton_collect_parallel_world_roots;
use neovm_value::test_support::authority::run_peer_young_collection_test_for_test;

#[test]
fn full_collection_resolves_nonempty_peer_young_and_promotes_before_response() {
    Context::with_test_collection_era(
        |context| {
            let child =
                unsafe { context.tagged_heap_mut() }.alloc_cons(Value::fixnum(907), Value::NIL);
            let head = unsafe { context.tagged_heap_mut() }.alloc_cons(child, Value::NIL);
            // SAFETY: serial setup owns this heap, with no collection/callback
            // between allocation and admission of this freshly live graph.
            let root = unsafe { SharedRoot::new(context.tagged_heap(), head) };
            (root, [head.bits(), child.bits()])
        },
        |era, (root, words)| {
            run_peer_young_collection_test_for_test(
                "full_collection_resolves_nonempty_peer_young_and_promotes_before_response",
                era,
                root,
                words,
                baton_collect_parallel_world_roots,
            );
        },
    );
}
