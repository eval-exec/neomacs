//! Real allocator carves with a legacy and a separately published young ID.
use super::super::scan_contract::SingleMutatorWorld;
use super::*;

struct TwoLogs<'a> {
    heap: HeapIdentity,
    ids: Vec<YoungRegistrationId>,
    legacy: &'a mut YoungBlockLog,
    peer: &'a mut YoungBlockLog,
}
impl ParallelYoungLogs for TwoLogs<'_> {
    fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }
    fn registrations(&self) -> &[YoungRegistrationId] {
        &self.ids
    }
    fn resolve_mut(
        &mut self,
        id: YoungRegistrationId,
    ) -> Result<&mut YoungBlockLog, YoungRegistryError> {
        if id == self.legacy.id() {
            Ok(self.legacy)
        } else if id == self.peer.id() {
            Ok(self.peer)
        } else {
            Err(YoungRegistryError::UnknownRegistration)
        }
    }
}

fn carved_block(allocator: &mut ConsAllocator, log: &mut YoungBlockLog) -> ConsBlock {
    let mut block = ConsBlock::new();
    block.reserve_tail(8);
    allocator.register(&block);
    let mut head = std::ptr::null_mut();
    let live = block.sweep(&mut head);
    assert_eq!(live, 0);
    // SAFETY: newly owned storage, exact rebuilt free list, no active worker.
    unsafe { allocator.publish(&block, head, live) };
    // SAFETY: registered live storage returns one exclusive eight-cell grant.
    let (run, count) = unsafe { allocator.reserve_run(8) }.unwrap();
    assert_eq!(ConsBlock::block_base_for_ptr(run), block.base_addr());
    allocator
        .note_carve(ConsBlockBase::of(&block), log)
        .unwrap();
    // SAFETY: every cell remains unused; refund precedes stop-all capture.
    unsafe { allocator.return_unused(run, count) };
    block
}

#[test]
fn parallel_major_captures_real_peer_payload_and_clears_flags_only_at_promotion() {
    let mut heap = TaggedHeap::new();
    let identity = heap.heap_identity();
    let mut allocator = ConsAllocator::new(identity);
    let peer_id = YoungRegistrationId::issue_parallel(identity).unwrap();
    let mut peer_slot = super::super::young_registry::YoungLogSlot::new_parallel(peer_id);
    let mut peer = peer_slot.take_owner_payload().unwrap();
    // SAFETY: direct serial fixture owns heap and every block, with no TLS
    // aliases, workers, callbacks or allocation safepoints throughout.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let legacy_id = world.current_young_registration_id();
    world
        .with_stopped_young_logs(|_, canonical, _| {
            let legacy = canonical.resolve_mut(legacy_id).unwrap();
            let blocks = [
                carved_block(&mut allocator, legacy),
                carved_block(&mut allocator, &mut peer),
            ];
            let bases = blocks.each_ref().map(ConsBlockBase::of);
            let backing = [legacy.blocks().as_ptr(), peer.blocks().as_ptr()];
            let mut logs = TwoLogs {
                heap: identity,
                ids: vec![legacy_id, peer_id],
                legacy,
                peer: &mut peer,
            };
            // SAFETY: every grant is refunded, both canonical payloads are uniquely
            // borrowed, and all registered backing blocks outlive the handoff.
            let mut stopped = unsafe { allocator.stopped_world() };
            stopped.capture_parallel_major(&mut logs).unwrap();
            assert_eq!(stopped.state.mark_work.len(), 2);
            assert!(stopped.state.pending_mutators.is_empty());
            for base in bases {
                let block = stopped.state.block_for_base(base);
                assert!(
                    block.young.load(Ordering::Relaxed),
                    "capture must retain young flags until promotion"
                );
                assert!(block.young_log_slot.is_none());
            }
            assert!(logs.legacy.blocks().is_empty() && logs.legacy.pending_index().is_none());
            assert!(logs.peer.blocks().is_empty() && logs.peer.pending_index().is_none());
            assert_eq!([logs.legacy.id(), logs.peer.id()], [legacy_id, peer_id]);
            assert_eq!(
                [logs.legacy.blocks().as_ptr(), logs.peer.blocks().as_ptr()],
                backing
            );
            drop(logs);
            stopped
                .finish_promotion(
                    super::super::generational::GenerationCycle::Major,
                    &blocks,
                    0,
                    canonical,
                )
                .unwrap();
            assert!(stopped.state.mark_work.is_empty());
            assert_eq!(stopped.state.sweep_work, bases);
            for base in bases {
                assert!(
                    !stopped
                        .state
                        .block_for_base(base)
                        .young
                        .load(Ordering::Relaxed)
                );
            }
            drop(stopped);
            for base in bases {
                allocator.unregister(base);
            }
        })
        .unwrap();
    peer_slot.restore_owner_payload(peer).unwrap();
    assert_eq!(peer_slot.id(), peer_id);
    assert!(peer_slot.resident().unwrap().blocks().is_empty());
}

#[test]
fn parallel_major_refuses_missing_peer_before_draining_either_payload() {
    let mut heap = TaggedHeap::new();
    let identity = heap.heap_identity();
    let mut allocator = ConsAllocator::new(identity);
    let peer_id = YoungRegistrationId::issue_parallel(identity).unwrap();
    let mut peer_slot = super::super::young_registry::YoungLogSlot::new_parallel(peer_id);
    let mut peer = peer_slot.take_owner_payload().unwrap();
    // SAFETY: same direct, exclusively stopped fixture contract as above.
    let mut world = unsafe { SingleMutatorWorld::from_heap(&mut heap) };
    let legacy_id = world.current_young_registration_id();
    world
        .with_stopped_young_logs(|_, canonical, _| {
            let legacy = canonical.resolve_mut(legacy_id).unwrap();
            let blocks = [
                carved_block(&mut allocator, legacy),
                carved_block(&mut allocator, &mut peer),
            ];
            let bases = blocks.each_ref().map(ConsBlockBase::of);
            let mut logs = TwoLogs {
                heap: identity,
                ids: vec![legacy_id],
                legacy,
                peer: &mut peer,
            };
            // SAFETY: both blocks and unique payloads remain retained and stopped.
            let mut stopped = unsafe { allocator.stopped_world() };
            assert!(matches!(
                stopped.capture_parallel_major(&mut logs),
                Err(YoungRegistryError::UnknownRegistration)
            ));
            assert!(matches!(
                stopped.capture_parallel_minor(&mut logs),
                Err(YoungRegistryError::UnknownRegistration)
            ));
            assert_eq!(logs.legacy.blocks(), &bases[..1]);
            assert_eq!(logs.peer.blocks(), &bases[1..]);
            assert!(stopped.state.mark_work.is_empty());
            assert_eq!(stopped.state.pending_mutators, vec![legacy_id, peer_id]);
            for base in bases {
                let block = stopped.state.block_for_base(base);
                assert!(block.young.load(Ordering::Relaxed));
                assert!(block.young_log_slot.is_some());
            }
            // Restore complete canonical coverage and finish the real handoff.
            logs.ids.push(peer_id);
            stopped.capture_parallel_major(&mut logs).unwrap();
            drop(logs);
            stopped
                .finish_promotion(
                    super::super::generational::GenerationCycle::Major,
                    &blocks,
                    0,
                    canonical,
                )
                .unwrap();
            drop(stopped);
            for base in bases {
                allocator.unregister(base);
            }
        })
        .unwrap();
    peer_slot.restore_owner_payload(peer).unwrap();
}

impl ConsStopAll<'_> {
    /// Test-only reassignment of real, already-carved setup blocks. The caller
    /// holds AllStopped and unique borrows of both actual published payloads.
    pub(in crate::tagged::gc) fn rebind_young_for_test(
        &mut self,
        from: &mut YoungBlockLog,
        to: &mut YoungBlockLog,
    ) -> Vec<ConsBlockBase> {
        assert_eq!(from.id().heap_identity(), self.state.identity);
        assert_eq!(to.id().heap_identity(), self.state.identity);
        assert_ne!(from.id(), to.id());
        assert!(!from.blocks().is_empty());
        assert!(to.blocks().is_empty() && to.pending_index().is_none());
        assert!(self.state.mark_work.is_empty());
        assert_eq!(self.state.pending_mutators.as_slice(), &[from.id()]);
        assert_eq!(from.pending_index().unwrap().index(), 0);
        for (index, base) in from.blocks().iter().copied().enumerate() {
            let block = self.state.block_for_base(base);
            let backlink = block.young_log_slot.unwrap();
            assert_eq!(backlink.registration, from.id());
            assert_eq!(backlink.index.index(), index);
            assert!(block.young.load(Ordering::Relaxed));
        }
        let bases: Vec<_> = from.drain_blocks().collect();
        assert!(
            from.remove_pending(&mut self.state.pending_mutators)
                .is_none()
        );
        for &base in &bases {
            let index = to.push(base);
            self.state.block_for_base_mut(base).young_log_slot = Some(YoungLogSlot {
                registration: to.id(),
                index,
            });
        }
        to.ensure_pending(&mut self.state.pending_mutators);
        assert_eq!(self.state.pending_mutators.as_slice(), &[to.id()]);
        bases
    }

    pub(in crate::tagged::gc) fn assert_rebound_young_finished_for_test(
        &self,
        bases: &[ConsBlockBase],
        peer: YoungRegistrationId,
    ) {
        assert!(!self.state.pending_mutators.contains(&peer));
        assert!(self.state.mark_work.is_empty());
        let mut retained = 0;
        for base in bases {
            if let Some(&slot) = self.state.blocks.get(base) {
                let block = self.state.block(slot);
                assert!(!block.young.load(Ordering::Relaxed));
                assert!(block.young_log_slot.is_none());
                retained += 1;
            }
        }
        assert!(
            retained > 0,
            "the surviving graph retains at least one real block"
        );
    }
}
