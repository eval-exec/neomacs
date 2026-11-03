//! Full-job coverage for a real nonempty peer young payload, without adding
//! parallel allocation. Setup storage is serial; only canonical log credit is
//! rebound after AllStopped under the actual stopped allocator catalog.
#![cfg(not(feature = "gc-memory-telemetry"))]
use super::*;
use crate::emacs_core::eval::Context;
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::TaggedValue as Value;

#[test]
fn full_collection_resolves_nonempty_peer_young_and_promotes_before_response() {
    Context::with_test_collection_era(
        |context| {
            let child = context
                .tagged_heap
                .alloc_cons(Value::fixnum(907), Value::NIL);
            let head = context.tagged_heap.alloc_cons(child, Value::NIL);
            // SAFETY: serial setup owns this heap, with no collection/callback
            // between allocation and admission of this freshly live graph.
            let root = unsafe { SharedRoot::new(&context.tagged_heap, head) };
            (root, [head.bits(), child.bits()])
        },
        |era, (root, words)| {
            let mut owner = era
                .register(907, 0)
                .unwrap()
                .activate(era.heap_identity(), 907)
                .unwrap();
            let peer_key = owner.key();
            owner.enter_native(std::slice::from_ref(&root)).unwrap();
            drop(root); // the stopped publication alone retains this graph
            let mut stopped = era.stop().unwrap();
            let (legacy, peer) = {
                let gate = era.registry().lock().unwrap();
                (
                    Arc::clone(&gate.entries[0]),
                    era.registry().validate(&gate, peer_key).unwrap(),
                )
            };
            let peer_id = peer.young;
            let bases = {
                stopped.check_world_roots(era.heap_identity()).unwrap();
                let mut slot = era.core.collector.try_borrow_mut().unwrap();
                let heap = slot.as_mut().unwrap().registration_heap_mut();
                if heap.generational_enabled() {
                    // No control gate survives into the allocator lock. All
                    // participants are stopped; the keeper alone borrows these
                    // two canonical entry images and the real heap catalog.
                    let mut from = legacy.state.try_lock().unwrap();
                    let mut to = peer.state.try_lock().unwrap();
                    let from = from.image.as_mut().unwrap().young_mut();
                    let to = to.image.as_mut().unwrap().young_mut();
                    assert_eq!(to.id(), peer_id);
                    // SAFETY: matching AllStopped, closed regions and no worker
                    // loan; these are actual heap-owned setup blocks.
                    let mut catalog = unsafe { heap.cons_allocator.stopped_world() };
                    let bases = catalog.rebind_young_for_test(from, to);
                    assert!(!bases.is_empty());
                    assert!(from.blocks().is_empty() && from.pending_index().is_none());
                    assert_eq!(to.blocks(), bases.as_slice());
                    Some(bases)
                } else {
                    assert!(
                        legacy
                            .state
                            .try_lock()
                            .unwrap()
                            .image
                            .as_ref()
                            .unwrap()
                            .young_empty()
                    );
                    assert!(
                        peer.state
                            .try_lock()
                            .unwrap()
                            .image
                            .as_ref()
                            .unwrap()
                            .young_empty()
                    );
                    None // GEN0 has no young-log membership by design
                }
            };
            let report = era.collect_stopped(&mut stopped).unwrap();
            assert!(report.major);
            {
                let state = peer.state.try_lock().unwrap();
                let image = state.image.as_ref().unwrap();
                assert_eq!(image.young_id(), peer_id);
                assert!(
                    image.young_empty(),
                    "peer log consumed before its response commits"
                );
            }
            era.with_stopped_heap_for_test(&stopped, |heap| {
                for word in words {
                    assert!(heap.owns_heap_value_for_test(Value::from_bits(word)));
                }
            })
            .unwrap();
            if let Some(bases) = bases {
                let mut slot = era.core.collector.try_borrow_mut().unwrap();
                let heap = slot.as_mut().unwrap().registration_heap_mut();
                // SAFETY: same stopped epoch; mark/promotion/sweep finished and
                // no owner has received a resume image yet.
                let catalog = unsafe { heap.cons_allocator.stopped_world() };
                catalog.assert_rebound_young_finished_for_test(&bases, peer_id);
            }
            stopped.complete().unwrap();
            owner.leave_native().unwrap();
            assert!(owner.entry.state.try_lock().unwrap().image.is_none());
            let local_young = owner.local.as_ref().unwrap().cold.young.resident().unwrap();
            assert_eq!(local_young.id(), peer_id);
            assert!(local_young.blocks().is_empty() && local_young.pending_index().is_none());
            owner.retire_rooted(&[]).unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
        },
    );
}
