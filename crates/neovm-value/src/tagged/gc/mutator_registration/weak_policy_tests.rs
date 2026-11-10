//! Fence negatives use serial setup or the real keeper stop, never a fake stop.

use super::*;
use crate::tagged::gc::ProofEra;
use crate::tagged::header::{HashTableObj, ImageResident, VecLikeHeader, VecLikeType};
use crate::value::{HashTableTest, HashTableWeakness, LispHashTable};

/// Real World root walk for the stopped case. The core shell passes the baton
/// walker; the raw chain-head slots obey the same contract as
/// `TaggedHeap::set_marker_chain_head_slots`.
pub(crate) type WorldRootsFn<K> = for<'k, 's, 'e, 'o> fn(
    &'k K,
    &'s mut AllStopped<'e>,
    &'o mut Vec<TaggedValue>,
) -> Result<
    Vec<*mut *mut crate::tagged::header::MarkerObj>,
    RendezvousError,
>;

/// One host shape per Context entry point. The core shell builds the host and
/// never passes a Context into value.
pub enum WeakPolicyHost<'a, 'heap, K: EraKeeper> {
    Keeper {
        heap: &'a mut TaggedHeap,
        keeper: &'a K,
        table: TaggedValue,
    },
    ForeignKeeper {
        first_heap: &'a TaggedHeap,
        first_keeper: &'a K,
        second_heap: &'a TaggedHeap,
        second_keeper: &'a K,
        table: TaggedValue,
    },
    Stopped {
        era: &'a ProofEra<'heap, K>,
        table: TaggedValue,
        world_roots: WorldRootsFn<K>,
    },
}

/// One row per former value lib-test.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeakPolicyCase {
    ExplicitCompleteInventory,
    ForeignAndNonTableDesignators,
    PendingActiveTestAndStorageAuthority,
    PermanentMappedAndCallbackPayloads,
    StoppedFenceRefusesBeforeMutationAndRecovers,
}

impl WeakPolicyCase {
    pub const ALL: &'static [Self] = &[
        Self::ExplicitCompleteInventory,
        Self::ForeignAndNonTableDesignators,
        Self::PendingActiveTestAndStorageAuthority,
        Self::PermanentMappedAndCallbackPayloads,
        Self::StoppedFenceRefusesBeforeMutationAndRecovers,
    ];
}

fn explicit_complete_inventory_body<K: EraKeeper>(
    heap: &mut TaggedHeap,
    keeper: &K,
    table: TaggedValue,
) {
    let none = WeakSweepPermit::mint(heap, keeper, &[]).unwrap();
    assert!(supported(heap).is_err());
    assert!(supported_with_weak(heap, Some(&none)).is_err());
    let permit = WeakSweepPermit::mint(heap, keeper, &[table]).unwrap();
    supported_with_weak(heap, Some(&permit)).unwrap();
    assert!(!permit.covers(heap, TaggedValue::NIL.bits()));
}

fn foreign_and_non_table_designators_body<K: EraKeeper>(
    first_heap: &TaggedHeap,
    first_keeper: &K,
    second_heap: &TaggedHeap,
    second_keeper: &K,
    table: TaggedValue,
) {
    assert!(matches!(
        WeakSweepPermit::mint(second_heap, first_keeper, &[table]),
        Err(RegistrationError::WrongHeap)
    ));
    assert!(matches!(
        WeakSweepPermit::mint(second_heap, second_keeper, &[table]),
        Err(RegistrationError::Publication)
    ));
    assert!(matches!(
        WeakSweepPermit::mint(first_heap, first_keeper, &[TaggedValue::fixnum(1)]),
        Err(RegistrationError::Publication)
    ));
}

fn pending_active_test_and_storage_authority_body<K: EraKeeper>(
    heap: &mut TaggedHeap,
    keeper: &K,
    table: TaggedValue,
) {
    let ptr = table
        .as_veclike_ptr()
        .unwrap()
        .cast::<HashTableObj>()
        .cast_mut();
    // SAFETY: this test is the sole serial keeper, with a rooted live shell;
    // it deliberately constructs refused states without entering any owner.
    unsafe {
        (*ptr).table.test_depth.store(1, Ordering::Release);
        assert!(WeakSweepPermit::mint(heap, keeper, &[table]).is_err());
        (*ptr).table.test_depth.store(0, Ordering::Release);
        for word in [1, super::super::table_authority::WRITER] {
            (*ptr).table.data.authority.store(word, Ordering::Release);
            assert!(WeakSweepPermit::mint(heap, keeper, &[table]).is_err());
        }
        (*ptr).table.data.authority.store(0, Ordering::Release);
        (*ptr).table.set_pending_dump_entries(Vec::new());
        assert!(WeakSweepPermit::mint(heap, keeper, &[table]).is_err());
        assert!(
            (*ptr).table.needs_hydration(),
            "refusal must not hydrate storage"
        );
        (*ptr).table.hydrate_pending();
    }
}

fn permanent_mapped_and_callback_payloads_body<K: EraKeeper>(
    heap: &mut TaggedHeap,
    keeper: &K,
    table: TaggedValue,
) {
    let ptr = table
        .as_veclike_ptr()
        .unwrap()
        .cast::<HashTableObj>()
        .cast_mut();
    // SAFETY: rooted owned header, exclusive serial keeper. Restore the
    // temporary generation marker before any real collection/destruction.
    unsafe {
        let header = &mut (*ptr).header.gc;
        let generation = header.generation;
        let tenured = header.tenured;
        header.make_permanent(&ImageResident::for_test());
        assert!(WeakSweepPermit::mint(heap, keeper, &[table]).is_err());
        header.generation = generation;
        header.tenured = tenured;
    }
    let permit = WeakSweepPermit::mint(heap, keeper, &[table]).unwrap();
    // A decoded image owns its shell, not this heap's owned-object catalog.
    let mapped = Box::new(HashTableObj {
        header: VecLikeHeader::new(VecLikeType::HashTable),
        table: LispHashTable::new_with_options(
            HashTableTest::Eq,
            4,
            Some(HashTableWeakness::Key),
            1.5,
            0.8,
        ),
    });
    let mapped_ptr = std::ptr::from_ref(&*mapped).cast_mut();
    let mapped_value = unsafe { TaggedValue::from_veclike_ptr(mapped_ptr.cast()) };
    assert!(WeakSweepPermit::mint(heap, keeper, &[mapped_value]).is_err());
    heap.mapped_veclike_objects
        .push(crate::tagged::gc::arena_pages::MappedVecLikeObject::new(
            mapped_ptr.cast(),
            std::mem::size_of::<HashTableObj>(),
        ));
    assert!(supported_with_weak(heap, Some(&permit)).is_err());
    heap.mapped_veclike_objects.pop();
    heap.alloc_user_ptr(std::ptr::null_mut(), None);
    assert!(supported_with_weak(heap, Some(&permit)).is_err());
}

fn stopped_fence_refuses_before_mutation_and_recovers_body<K: EraKeeper>(
    era: &ProofEra<'_, K>,
    table: TaggedValue,
    world_roots: WorldRootsFn<K>,
) {
    let mut stopped = era.stop().unwrap();
    let cycle = era
        .with_stopped_heap_for_test(&stopped, TaggedHeap::gc_collections)
        .unwrap();
    let lease = KeeperTableMut::enter(&stopped, table).unwrap();
    let authority = std::ptr::from_ref(lease.authority_word());
    let depth = std::ptr::from_ref(&lease.guarded_table().test_depth);
    drop(lease);
    for word in [1, super::super::table_authority::WRITER] {
        // SAFETY: fault injection into the rooted shell while the actual
        // stop remains held; no storage borrow or owner exists here.
        unsafe { (*authority).store(word, Ordering::Release) };
        assert!(era.collect_stopped(&mut stopped, world_roots).is_err());
        unsafe { (*authority).store(0, Ordering::Release) };
    }
    unsafe { (*depth).store(1, Ordering::Release) };
    assert!(era.collect_stopped(&mut stopped, world_roots).is_err());
    unsafe { (*depth).store(0, Ordering::Release) };
    assert_eq!(
        era.with_stopped_heap_for_test(&stopped, TaggedHeap::gc_collections)
            .unwrap(),
        cycle
    );
    era.collect_stopped(&mut stopped, world_roots).unwrap();
    stopped.complete().unwrap();
}

/// One runner for the weak-policy subtree. The case match is exhaustive over
/// the typed list; a host that does not fit its case is a loud invariant
/// failure, never a silent skip.
///
/// # Safety
///
/// The `Stopped` host's `world_roots` must return only slots that satisfy
/// `TaggedHeap::set_marker_chain_head_slots`: each slot points to a valid
/// `*mut MarkerObj` living inside a live `BufferText`'s storage, remains valid
/// for the GC cycle, and the caller holds exclusive access to the heap and the
/// buffer manager for that cycle. The core shell passes the real baton walker,
/// not a synthetic slot list.
pub(crate) unsafe fn run_weak_policy_case<K: EraKeeper + std::fmt::Debug>(
    case: WeakPolicyCase,
    host: WeakPolicyHost<'_, '_, K>,
) {
    match (case, host) {
        (
            WeakPolicyCase::ExplicitCompleteInventory,
            WeakPolicyHost::Keeper {
                heap,
                keeper,
                table,
            },
        ) => explicit_complete_inventory_body(heap, keeper, table),
        (
            WeakPolicyCase::ForeignAndNonTableDesignators,
            WeakPolicyHost::ForeignKeeper {
                first_heap,
                first_keeper,
                second_heap,
                second_keeper,
                table,
            },
        ) => foreign_and_non_table_designators_body(
            first_heap,
            first_keeper,
            second_heap,
            second_keeper,
            table,
        ),
        (
            WeakPolicyCase::PendingActiveTestAndStorageAuthority,
            WeakPolicyHost::Keeper {
                heap,
                keeper,
                table,
            },
        ) => pending_active_test_and_storage_authority_body(heap, keeper, table),
        (
            WeakPolicyCase::PermanentMappedAndCallbackPayloads,
            WeakPolicyHost::Keeper {
                heap,
                keeper,
                table,
            },
        ) => permanent_mapped_and_callback_payloads_body(heap, keeper, table),
        (
            WeakPolicyCase::StoppedFenceRefusesBeforeMutationAndRecovers,
            WeakPolicyHost::Stopped {
                era,
                table,
                world_roots,
            },
        ) => stopped_fence_refuses_before_mutation_and_recovers_body(era, table, world_roots),
        (case, _) => panic!("weak-policy case {case:?} does not match its host shape"),
    }
}
