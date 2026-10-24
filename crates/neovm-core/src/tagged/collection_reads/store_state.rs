//! World-owned store protocol. Only the Baton holder mutates these Cells.
//!
//! A collector may publish another owner's windows only while the registered
//! World is parked in the stopped handshake, before releasing that owner.
//! Neither a registry lock alone nor a shared heap reference permits this.
use super::*;
use crate::tagged::gc::{BarrierWindow, TaggedHeap};
use std::cell::OnceCell;
use std::mem::{MaybeUninit, align_of, offset_of, size_of};
use std::ptr::NonNull;
use std::rc::Rc;

/// Mutator-owned fields stay together so registration can own this block
/// when several mutators eventually share one heap. Only the Baton holder
/// writes them today. The retained World owns the out-of-line ring allocation.
#[repr(C)]
pub(crate) struct MutatorStoreState {
    pub(super) revision: Cell<u64>,
    pub(super) writes: Cell<std::ptr::NonNull<[Cell<usize>; JOURNAL_SIZE]>>,
    pub(super) policy: Cell<JournalPolicy>,
    pub(super) observe: Cell<bool>,
    // Fill the ABI padding with Cells as well: a shared store-protocol
    // reference may remain live across the barrier's whole-heap loan.
    _padding: [Cell<u8>; size_of::<usize>() - 2],
}

/// Heap-owned publication, separate from the executing mutator's journal.
#[repr(C)]
pub(crate) struct StoreWindows {
    pub(crate) cons_window: Cell<BarrierWindow>,
    pub(crate) window: Cell<BarrierWindow>,
}

/// The authoritative store protocol lives inline at the heap tail while
/// installed, and in detached storage before installation/after retirement.
/// Migration is cold, with no Lisp callback or outstanding store reference.
#[repr(C)]
pub(crate) struct StoreState {
    pub(crate) mutator: MutatorStoreState,
    pub(crate) windows: StoreWindows,
    pub(crate) heap: Cell<*mut TaggedHeap>,
}

impl StoreState {
    fn new(writes: NonNull<[Cell<usize>; JOURNAL_SIZE]>) -> Self {
        initialize();
        Self {
            mutator: MutatorStoreState {
                revision: Cell::new(0),
                writes: Cell::new(writes),
                policy: Cell::new(if history_required() {
                    JournalPolicy::JournalOnly
                } else {
                    JournalPolicy::BeforeFirstCapture
                }),
                observe: Cell::new(false),
                _padding: [const { Cell::new(0) }; size_of::<usize>() - 2],
            },
            windows: StoreWindows {
                cons_window: Cell::new(BarrierWindow::NONE),
                window: Cell::new(BarrierWindow::NONE),
            },
            heap: Cell::new(std::ptr::null_mut()),
        }
    }

    #[inline(always)]
    pub(crate) fn sequence(&self) -> u64 {
        self.mutator.revision.get()
    }

    #[inline(always)]
    pub(crate) fn advance(&self) -> u64 {
        let next = self.mutator.revision.get().wrapping_add(1);
        self.mutator.revision.set(next);
        next
    }

    #[inline(always)]
    pub(super) fn journal(&self) -> &[Cell<usize>; JOURNAL_SIZE] {
        // SAFETY: ObservationWorld retains this one ring allocation across
        // inline/detached protocol moves. Every element is interior mutable.
        unsafe { self.mutator.writes.get().as_ref() }
    }

    #[inline(always)]
    pub(super) fn record(&self, owner: usize, revision: u64) {
        self.journal()[revision as usize & (JOURNAL_SIZE - 1)].set(owner);
    }

    pub(crate) fn publish_windows(&self, cons: BarrierWindow, ordinary: BarrierWindow) {
        self.windows.cons_window.set(cons);
        self.windows.window.set(ordinary);
    }
}

struct WorldObservations {
    // The ring outlives either scalar location, including retained certificates.
    _writes: crate::tagged::gc::AliasableBox<[Cell<usize>; JOURNAL_SIZE]>,
    detached: std::cell::UnsafeCell<MaybeUninit<StoreState>>,
    inline: Cell<Option<NonNull<StoreState>>>,
    dump: Cell<BarrierWindow>,
    cold: OnceCell<RefCell<State>>,
    bound: Cell<bool>,
}

/// Allocation lifetime only. Executing authority is the existing World Baton
/// or stopped-owner handshake; cloning this handle never grants execution.
#[derive(Clone)]
pub(crate) struct ObservationWorld(Rc<WorldObservations>);

static_assertions::assert_not_impl_any!(ObservationWorld: Send, Sync);

impl Default for ObservationWorld {
    fn default() -> Self {
        let writes = crate::tagged::gc::AliasableBox::new([const { Cell::new(0) }; JOURNAL_SIZE]);
        let store = StoreState::new(writes.root());
        Self(Rc::new(WorldObservations {
            _writes: writes,
            detached: std::cell::UnsafeCell::new(MaybeUninit::new(store)),
            inline: Cell::new(None),
            dump: Cell::new(BarrierWindow::NONE),
            cold: OnceCell::new(),
            bound: Cell::new(false),
        }))
    }
}

impl std::fmt::Debug for ObservationWorld {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ObservationWorld")
            .field(&self.root())
            .finish()
    }
}

impl ObservationWorld {
    pub(super) fn current() -> Self {
        STATE.with(|slot| slot.owner.borrow().clone())
    }

    pub(super) fn is_current(&self) -> bool {
        current_store_root() == self.root()
    }

    pub(super) fn store(&self) -> &StoreState {
        // SAFETY: the location descriptor selects the only initialized scalar
        // home. Heap retirement detaches before its storage can disappear.
        unsafe { self.root().as_ref() }
    }

    pub(super) fn with_state<R>(&self, f: impl FnOnce(&RefCell<State>) -> R) -> R {
        f(self.0.cold.get_or_init(|| RefCell::new(State::default())))
    }

    fn has_captures(&self) -> bool {
        self.0
            .cold
            .get()
            .is_some_and(|cold| !cold.borrow().captures.is_empty())
    }

    fn observe_scopes(&self, active: bool) {
        self.store().mutator.observe.set(active);
        if active {
            self.store()
                .mutator
                .policy
                .set(JournalPolicy::JournalAndObserve);
        } else if self.store().mutator.policy.get() == JournalPolicy::JournalAndObserve {
            self.store().mutator.policy.set(JournalPolicy::JournalOnly);
        }
    }

    /// The first heap may adopt a pending capture made before any allocation
    /// heap existed. Once bound, a state always belongs to that one World.
    pub(crate) fn for_heap() -> Self {
        STATE.with(|slot| {
            let current = slot.owner.borrow();
            if !current.0.bound.replace(true) {
                return current.clone();
            }
            let next = Self::default();
            next.0.bound.set(true);
            next
        })
    }

    pub(crate) fn root(&self) -> NonNull<StoreState> {
        self.0.inline.get().unwrap_or_else(|| {
            // SAFETY: Rc retains detached storage. It is initialized exactly
            // when inline is None; the pointer is never derived from &mut.
            unsafe { NonNull::new_unchecked(self.0.detached.get().cast::<StoreState>()) }
        })
    }

    fn refresh_location_tls(&self) {
        let _ = STATE.try_with(|slot| {
            if Rc::ptr_eq(&slot.owner.borrow().0, &self.0) {
                slot.store.set(self.root());
            }
        });
    }

    /// Install only from the heap allocation's provenance root, before any
    /// temporary heap loan. Neither home is accessible during this transfer.
    pub(crate) unsafe fn attach(
        &self,
        heap: NonNull<TaggedHeap>,
        destination: NonNull<StoreState>,
    ) {
        // Installation has no competing heap loan: its provenance root can
        // supply both the legitimate write pointer and retained location.
        unsafe { self.attach_via(heap, destination.as_ptr(), destination) };
    }

    /// The write pointer comes from a legitimate current heap loan; the saved
    /// descriptor comes independently from the allocation's provenance root.
    /// Publishing that descriptor does not dereference it during the loan.
    pub(crate) unsafe fn attach_via(
        &self,
        heap: NonNull<TaggedHeap>,
        write_pointer: *mut StoreState,
        destination: NonNull<StoreState>,
    ) {
        if self.0.inline.get() == Some(destination) {
            return;
        }
        if self.0.inline.get().is_none() {
            // SAFETY: detached is the initialized home. Destination belongs
            // to this owner and is unused while the metadata is detached.
            let moved = unsafe { self.root().as_ptr().read() };
            moved.heap.set(heap.as_ptr());
            unsafe { write_pointer.write(moved) };
        } else {
            // A legacy by-value fixture may move an attached heap before
            // reinstalling it. Its inline bytes moved with it; never read
            // through the obsolete location descriptor.
            unsafe { &*write_pointer }.heap.set(heap.as_ptr());
        }
        self.0.inline.set(Some(destination));
        self.refresh_location_tls();
    }

    /// Retire via the legitimate heap loan, including a by-value shutdown or
    /// Drop move. The stale location is compared, never dereferenced: `source`
    /// addresses the current heap's moved scalar bytes.
    pub(crate) unsafe fn detach(&self, source: *const StoreState) {
        if self.0.inline.get().is_none() {
            return;
        }
        // SAFETY: the caller's current heap loan owns its initialized tail.
        // Moving it leaves that tail uninitialized until the next attach.
        unsafe {
            self.0
                .detached
                .get()
                .cast::<StoreState>()
                .write(source.read())
        };
        self.0.inline.set(None);
        self.store().heap.set(std::ptr::null_mut());
        self.store()
            .publish_windows(BarrierWindow::NONE, BarrierWindow::NONE);
        self.refresh_location_tls();
    }

    pub(crate) fn is_detached(&self) -> bool {
        self.0.inline.get().is_none()
    }

    pub(crate) fn publish_dump_window(&self, dump: BarrierWindow) {
        self.0.dump.set(dump);
    }

    pub(crate) fn install(&self, dump: BarrierWindow) {
        STATE.with(|slot| {
            if !Rc::ptr_eq(&slot.owner.borrow().0, &self.0) {
                let previous = slot.owner.borrow().clone();
                let previous_dump = previous.0.dump.get();
                previous.observe_scopes(previous.has_captures());
                if previous.has_captures()
                    && !slot
                        .suspended
                        .borrow()
                        .iter()
                        .any(|owner| Rc::ptr_eq(&owner.0, &previous.0))
                {
                    slot.suspended.borrow_mut().push(previous);
                }
                // A changed dump span discards negative protocol refinements,
                // as the old thread-local envelope did. Same-span round trips
                // retain valid refinements, including Off/Observed toggles.
                // Keep positive dependencies and open captures with their
                // World, so a temporary switch and return preserves them.
                if previous_dump != dump
                    && let Some(cold) = self.0.cold.get()
                {
                    cold.borrow_mut().observed.discard_exclusions();
                }
                *slot.owner.borrow_mut() = self.clone();
                slot.store.set(self.root());
                reset_recent_reads_for_heap_installation();
            }
        });
        self.publish_dump_window(dump);
        refresh_membership();
    }
}

// Preserve the original TLS allocation and pointer-slot displacement until
// the explicitly separate TLS-removal experiment. No reserve is accessed.
#[repr(C)]
struct LegacyStateLayout {
    captures: Vec<Capture>,
    writes: Box<[usize; JOURNAL_SIZE]>,
    policy: JournalPolicy,
    observed: observed_envelope::ObservedEnvelope,
}

#[repr(C)]
pub(super) struct StateSlot {
    owner: RefCell<ObservationWorld>,
    _prefix: [MaybeUninit<usize>; 2],
    store: Cell<NonNull<StoreState>>,
    // An installation-only round trip preserves an open capture. Actual
    // foreign projections reject it in the cold observer instead. This list
    // fits inside the existing TLS reserve and is never read by a hot store.
    suspended: RefCell<Vec<ObservationWorld>>,
    // Scratch for the existing TLS RecentReads, reset at World installation.
    // No owner identity is stored, and no hot projection reads these masks.
    recent_dirty: [Cell<u64>; 4],
    _reserve: [MaybeUninit<u8>;
        size_of::<RefCell<LegacyStateLayout>>() - 9 * size_of::<usize>() - 4 * size_of::<u64>()],
}

const _: () = {
    assert!(size_of::<StateSlot>() == size_of::<RefCell<LegacyStateLayout>>());
    assert!(align_of::<StateSlot>() == align_of::<RefCell<LegacyStateLayout>>());
    assert!(offset_of!(StateSlot, store) == 4 * size_of::<usize>());
    assert!(offset_of!(StateSlot, recent_dirty) == 9 * size_of::<usize>());
    assert!(JOURNAL_SIZE.is_power_of_two());
};

impl Default for StateSlot {
    fn default() -> Self {
        let owner = ObservationWorld::default();
        let store = Cell::new(owner.root());
        Self {
            owner: RefCell::new(owner),
            _prefix: [MaybeUninit::uninit(); 2],
            store,
            suspended: RefCell::new(Vec::new()),
            recent_dirty: [const { Cell::new(0) }; 4],
            _reserve: [MaybeUninit::uninit();
                size_of::<RefCell<LegacyStateLayout>>()
                    - 9 * size_of::<usize>()
                    - 4 * size_of::<u64>()],
        }
    }
}

/// Register only cold misses, before any publication shortcut or replay
/// rejection. The unchanged indexed word probe never pays this bookkeeping.
pub(super) fn note_recent_miss(bits: usize) {
    if bits != 0 {
        let index = ((bits >> 3) ^ (bits >> 11)) & 255;
        STATE.with(|slot| {
            let mask = &slot.recent_dirty[index / 64];
            mask.set(mask.get() | (1 << (index % 64)));
            RECENT_READS.with(|recent| recent.dirty.set(true));
        });
    }
}

pub(super) fn take_recent_dirty() -> Option<[u64; 4]> {
    STATE
        .try_with(|slot| {
            let dirty = std::array::from_fn(|index| slot.recent_dirty[index].replace(0));
            #[cfg(test)]
            RECENT_CLEARED_MASKS.with(|count| count.set(count.get() + 4));
            dirty
        })
        .ok()
}

/// One TLS pointer load and no RefCell borrow. The TLS owner retains the root
/// for this entire synchronous noncalling store operation.
#[inline(always)]
pub(crate) fn with_store<R>(f: impl FnOnce(&StoreState) -> R) -> R {
    STATE.with(|slot| {
        // SAFETY: slot.owner retains the allocation; installation updates the
        // root synchronously before mutation resumes. No store can hand off.
        f(unsafe { slot.store.get().as_ref() })
    })
}

/// Clone lifetime before borrowing cold data. No TLS-owner borrow is held
/// across cold publication, and the callback cannot return a borrowed State.
pub(super) fn with_state<R>(f: impl FnOnce(&RefCell<State>) -> R) -> R {
    ObservationWorld::current().with_state(f)
}

pub(crate) fn current_store_root() -> NonNull<StoreState> {
    STATE.with(|slot| slot.store.get())
}

pub(super) fn carried_placeholder() -> Box<dyn std::any::Any> {
    Box::new(ObservationWorld::default())
}

pub(super) fn swap_carried(slot: &mut dyn std::any::Any) {
    let arriving = slot
        .downcast_mut::<ObservationWorld>()
        .expect("carried World observation owner");
    // Foreign local captures do not migrate with this World. Crossing an OS
    // thread boundary cannot retain their unrecorded dependency projections.
    reject_foreign_capture_reads();
    STATE.with(|slot| {
        let departing = slot.owner.borrow().clone();
        departing.observe_scopes(departing.has_captures());
        std::mem::swap(&mut *slot.owner.borrow_mut(), arriving);
        let owner = slot.owner.borrow();
        slot.store.set(owner.root());
    });
    refresh_membership();
    reset_recent_reads_for_heap_installation();
}

/// Restore the existing private membership mirror after local activation,
/// scope exit, or a carried transfer. Open scopes from other local Worlds
/// enter only cold observation paths; an ordinary idle store reads only the installed heap TLS slot.
pub(super) fn refresh_membership() {
    STATE.with(|slot| {
        let current = slot.owner.borrow();
        let mut suspended = slot.suspended.borrow_mut();
        suspended.retain(|owner| !Rc::ptr_eq(&owner.0, &current.0) && owner.has_captures());
        let active = current.has_captures() || !suspended.is_empty();
        current.observe_scopes(active);
        ACTIVE.with(|membership| membership.set(active));
    });
}

/// The current World's dependency map cannot describe a different World's
/// projection. Reject suspended foreign captures rather than certify a result
/// with missing reads. Merely installing another heap does not call this.
#[cold]
#[inline(never)]
pub(super) fn reject_foreign_capture_reads() {
    STATE.with(|slot| {
        for owner in slot.suspended.borrow().iter() {
            if let Some(cold) = owner.0.cold.get() {
                for capture in &mut cold.borrow_mut().captures {
                    capture.overflow = true;
                }
            }
        }
    });
}

pub(crate) fn clear_installed_store_heap(_heap: *mut TaggedHeap) {
    // No heap access is permitted here: the caller may already retain an
    // exclusive QuiescentCollector. Its preparation detached the protocol.
    let _ = STATE.try_with(|slot| {
        debug_assert!(
            slot.owner.borrow().is_detached(),
            "detach before an exclusive collector loan"
        );
    });
}
