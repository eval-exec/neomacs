//! Adapter protocol only: the second job repeats the census of old survivors.
//! Fresh replacements and sole-young-family mixed tapes are the next item.
use super::*;
use crate::tagged::gc::{MinorCollectionError, TaggedHeap};
use crate::tagged::header::GcHeader;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Age {
    Young,
    Old,
    Missing,
}
#[derive(Clone, Copy)]
enum Pass {
    First,
    Second,
}
enum YoungCredit {
    Unchanged,
    Peer {
        bases: Vec<crate::tagged::gc::cons_allocator::ConsBlockBase>,
        registration: crate::tagged::gc::young_registry::YoungRegistrationId,
    },
}
#[derive(Debug, PartialEq, Eq)]
enum YoungPayload {
    Empty,
    Pending,
}
struct YoungPublication {
    owner: OwnerId,
    registration: crate::tagged::gc::young_registry::YoungRegistrationId,
    payload: YoungPayload,
}
fn ages(heap: &TaggedHeap, graph: GraphWords) -> [Age; 4] {
    graph.0.map(|word| age(heap, word))
}
pub(super) fn age(heap: &TaggedHeap, word: WordIdentity) -> Age {
    let value = TaggedValue::from_bits(word.0);
    if !heap.owns_heap_value_for_test(value) {
        return Age::Missing;
    }
    if value.is_cons() {
        match heap.old_cons_trailer(value) {
            Some((trailer, index)) => match trailer.is_old(index) {
                true => Age::Old,
                false => Age::Young,
            },
            None => Age::Missing,
        }
    } else {
        let Some(header) = value.gc_heap_ptr() else {
            return Age::Missing;
        };
        // SAFETY: the real stop retains the heap; the membership check
        // above proved this exact noncons allocation live before this read.
        match unsafe { &*header.cast::<GcHeader>() }.tenured {
            true => Age::Old,
            false => Age::Young,
        }
    }
}
fn rebind(era: &ParallelHeapEra<'_>, stopped: &AllStopped<'_>) -> YoungCredit {
    let (legacy, peer) = {
        let gate = era.registry().lock().unwrap();
        (
            Arc::clone(&gate.entries[0]),
            Arc::clone(
                gate.entries
                    .iter()
                    .find(|entry| entry.key.owner() == 1)
                    .unwrap(),
            ),
        )
    };
    stopped.check_world_roots(era.heap_identity()).unwrap();
    let mut slot = era.collector.try_borrow_mut().unwrap();
    let heap = slot.as_mut().unwrap().registration_heap_mut();
    if !heap.generational_enabled() {
        return YoungCredit::Unchanged;
    }
    // Verify the real captured inventory and promotion, not only report flags.
    heap.generational.verify = true;
    let mut from = legacy.state.try_lock().unwrap();
    let mut to = peer.state.try_lock().unwrap();
    let from = from.image.as_mut().unwrap().young_mut();
    let to = to.image.as_mut().unwrap().young_mut();
    let registration = to.id();
    // SAFETY: genuine AllStopped, all native images retain their canonical
    // payloads, and these two unique entry locks cover real setup blocks.
    let mut catalog = unsafe { heap.cons_allocator.stopped_world() };
    let bases = catalog.rebind_young_for_test(from, to);
    assert!(!bases.is_empty());
    YoungCredit::Peer {
        bases,
        registration,
    }
}
pub(super) fn exercise(
    era: &ParallelHeapEra<'_>,
    graph: GraphWords,
    cohort: Cohort,
    commands: &Commands,
    await_all: &impl Fn(),
) {
    let mut receipts = Vec::new();
    for pass in [Pass::First, Pass::Second] {
        let mut stopped = contention::retry(
            Deadline::contention(),
            || era.stop(),
            contention::rendezvous,
        )
        .unwrap();
        let before = era
            .with_stopped_heap_for_test(&stopped, |heap| {
                (CollectionCycle(heap.gc_collections()), ages(heap, graph))
            })
            .unwrap();
        let credit = match pass {
            Pass::First => rebind(era, &stopped),
            Pass::Second => YoungCredit::Unchanged,
        };
        let report = era.collect_stopped_minor(&mut stopped);
        match &report {
            Ok(_) => {}
            Err(MinorCollectionError::GenerationalDisabled) => {
                stopped.trace(|_, _| Ok(())).unwrap();
            }
            Err(MinorCollectionError::Collection(error)) => {
                panic!("minor adapter unexpected refusal: {error:?}")
            }
        }
        let after = era
            .with_stopped_heap_for_test(&stopped, |heap| {
                (CollectionCycle(heap.gc_collections()), ages(heap, graph))
            })
            .unwrap();
        let entries = era.registry().lock().unwrap().entries.clone();
        let mut published = Vec::new();
        for entry in &entries {
            let state = entry.state.try_lock().unwrap();
            let image = state.image.as_ref().unwrap();
            published.push(YoungPublication {
                owner: OwnerId(entry.key.owner()),
                registration: image.young_id(),
                payload: match image.young_empty() {
                    true => YoungPayload::Empty,
                    false => YoungPayload::Pending,
                },
            });
        }
        match credit {
            YoungCredit::Unchanged => {}
            YoungCredit::Peer {
                bases,
                registration,
            } => {
                let mut slot = era.collector.try_borrow_mut().unwrap();
                let heap = slot.as_mut().unwrap().registration_heap_mut();
                // SAFETY: mark/promotion/sweep finished in this matching stop;
                // no response or mutable heap borrow escaped the keeper.
                let catalog = unsafe { heap.cons_allocator.stopped_world() };
                catalog.assert_rebound_young_finished_for_test(&bases, registration);
            }
        }
        stopped.complete().unwrap();
        // Assertions run after completion; all still-native owners can always
        // be released by the parent fixture's independent command guard.
        assert!(
            published
                .iter()
                .all(|image| image.payload == YoungPayload::Empty)
        );
        for image in &published {
            assert_eq!(
                image.registration.heap_identity(),
                era.heap_identity(),
                "{:?}",
                image.owner
            );
        }
        match report {
            Ok(report) => {
                assert!(!report.major);
                assert_eq!(report.frozen_owners, cohort.roles().len());
                assert!(after.0 > before.0);
                match pass {
                    Pass::First => assert_eq!(before.1, [Age::Young; 4]),
                    Pass::Second => assert_eq!(before.1, [Age::Old; 4]),
                }
                assert_eq!(after.1, [Age::Old; 4]);
                eprintln!(
                    "P3SYM_C6_MINOR_ADAPTER epoch={} cycle={} owners={} ages={:?}",
                    report.epoch, report.cycle, report.frozen_owners, after.1
                );
                receipts.push(report);
            }
            Err(MinorCollectionError::GenerationalDisabled) => {
                assert_eq!(
                    after, before,
                    "GEN0 refusal changes no cycle or known graph age"
                );
                eprintln!("P3SYM_C6_MINOR_ADAPTER_REFUSAL GenerationalDisabled");
            }
            Err(MinorCollectionError::Collection(error)) => {
                panic!("unexpected minor result after cleanup: {error:?}")
            }
        }
        for tx in &commands.0 {
            tx.send(Command::Repark).unwrap();
        }
        await_all();
    }
    if let [first, second] = receipts.as_slice() {
        assert!(second.epoch > first.epoch);
        assert!(second.cycle > first.cycle);
    }
}
