//! Unique numeric page views. No descriptor reference, lock, or whole-heap
//! designator is used at slot commit; the finite raw fields were minted once
//! before the frozen bank became available to owners.

use super::{CatalogIdentity, HeapIdentity, MutatorKey, RegistrationError};
use crate::tagged::gc::arena_pages::PagedObject;
use crate::tagged::header::{
    BignumObj, FloatObj, GcHeader, HeapObjectKind, MarkParity, VecLikeHeader, VecLikeType,
};
use crate::tagged::transport::ConsRootSeal;
use crate::tagged::value::TaggedValue;
use malachite::integer::Integer;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Copied only inside the bank before a unique claim. Each physical page has
/// one live claim; the catalog mutex hands its bitmap words and occupancy
/// to a subsequent claim only after the preceding run has settled.
#[derive(Clone, Copy)]
pub(super) struct RawPage {
    pub(super) page: usize,
    pub(super) payload: *mut u8,
    pub(super) bits: *mut usize,
    pub(super) occupancy: *mut usize,
    pub(super) slots: usize,
}

impl RawPage {
    /// Zero-view validation allows swept holes below the frontier. Set bits
    /// never extend into the uninitialized tail, and occupancy is the popcount.
    /// # Safety
    /// No owner view can still write this page's bitmap or occupancy field.
    pub(super) unsafe fn has_initialized_frontier(self, frontier: usize) -> bool {
        if frontier > self.slots {
            return false;
        }
        let mut live = 0;
        // SAFETY: the frozen bank retains the descriptor allocation; all
        // unique runs have settled before this raw-only transition check.
        unsafe {
            for index in 0..self.slots.div_ceil(usize::BITS as usize) {
                let first = index * usize::BITS as usize;
                let count = frontier.saturating_sub(first).min(usize::BITS as usize);
                let allowed = if count == usize::BITS as usize {
                    usize::MAX
                } else {
                    (1usize << count) - 1
                };
                let bits = self.bits.add(index).read();
                if bits & !allowed != 0 {
                    return false;
                }
                live += bits.count_ones() as usize;
            }
            self.occupancy.read() == live
        }
    }
}

enum LeaseState {
    Active,
    Settled,
}

pub(super) struct RunCore {
    pub(super) page: RawPage,
    pub(super) identity: Arc<CatalogIdentity>,
    pub(super) owner: MutatorKey,
    pub(super) generation: u64,
    pub(super) claim: u64,
    pub(super) pool_index: usize,
    pub(super) parity: MarkParity,
    pub(super) start: usize,
    pub(super) used: usize,
    state: LeaseState,
    _thread: PhantomData<Rc<()>>,
}

impl RunCore {
    pub(super) fn new(
        page: RawPage,
        identity: Arc<CatalogIdentity>,
        owner: MutatorKey,
        generation: u64,
        claim: u64,
        pool_index: usize,
        parity: MarkParity,
        start: usize,
    ) -> Self {
        Self {
            page,
            identity,
            owner,
            generation,
            claim,
            pool_index,
            parity,
            start,
            used: start,
            state: LeaseState::Active,
            _thread: PhantomData,
        }
    }

    fn validate(&self, owner: MutatorKey, generation: u64) -> Result<(), RegistrationError> {
        if owner.heap_identity() != self.owner.heap_identity() {
            return Err(RegistrationError::WrongHeap);
        }
        if owner.owner() != self.owner.owner() {
            return Err(RegistrationError::WrongOwner);
        }
        if owner != self.owner || generation != self.generation {
            return Err(RegistrationError::StaleRegistration);
        }
        if self.identity.abandoned.load(Ordering::Acquire) {
            return Err(RegistrationError::Poisoned);
        }
        Ok(())
    }

    pub(super) fn retire(&mut self) {
        self.state = LeaseState::Settled;
    }

    /// Called only after the complete object write. Bits never describe
    /// Uninit storage, including while the arena is detached.
    ///
    /// # Safety
    /// The caller has completely initialized the slot at `self.used` through
    /// this run's exclusive payload pointer. That index is below `page.slots`,
    /// its allocation bit is clear, and no other view can access this page's
    /// bitmap or occupancy field until the run settles.
    unsafe fn commit_initialized_slot(&mut self) {
        let index = self.used;
        // SAFETY: slot selection proved the next index fits this exclusive
        // page. No other owner shares its bitmap words or occupancy field.
        unsafe {
            let word = self.page.bits.add(index / usize::BITS as usize);
            word.write(word.read() | (1usize << (index % usize::BITS as usize)));
            self.page.occupancy.write(self.page.occupancy.read() + 1);
        }
        self.used += 1;
    }

    fn empty(&self) -> bool {
        self.used == self.page.slots
    }
}

impl Drop for RunCore {
    fn drop(&mut self) {
        if matches!(self.state, LeaseState::Active) {
            // A lost claim never settles its bank obligation. Both the bank
            // and its raw backing remain retained; no destructor sees Uninit.
            self.identity.abandoned.store(true, Ordering::Release);
        }
    }
}

#[must_use = "settle the numeric run through its original catalog and owner"]
pub(in crate::tagged::gc) struct FloatRun {
    pub(super) core: RunCore,
}

#[must_use = "settle the numeric run through its original catalog and owner"]
pub(in crate::tagged::gc) struct BignumRun {
    pub(super) core: RunCore,
}

macro_rules! run_observers {
    ($ty:ty) => {
        impl $ty {
            pub(in crate::tagged::gc) fn validate(
                &self,
                owner: MutatorKey,
                generation: u64,
            ) -> Result<(), RegistrationError> {
                self.core.validate(owner, generation)
            }

            pub(in crate::tagged::gc) fn used(&self) -> usize {
                self.core.used - self.core.start
            }

            pub(in crate::tagged::gc) fn capacity(&self) -> usize {
                self.core.page.slots - self.core.start
            }

            pub(in crate::tagged::gc) fn empty(&self) -> bool {
                self.core.empty()
            }
        }

        impl std::fmt::Debug for $ty {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($ty))
                    .field("owner", &self.core.owner)
                    .field("generation", &self.core.generation)
                    .field("used", &self.core.used)
                    .field("capacity", &self.core.page.slots)
                    .finish_non_exhaustive()
            }
        }
    };
}

run_observers!(FloatRun);
run_observers!(BignumRun);

impl FloatRun {
    /// Selection alone changes no frontier, bits, occupancy or accounting.
    pub(in crate::tagged::gc) fn next_slot(&mut self) -> Option<UninitFloatSlot<'_>> {
        if self.core.empty() {
            None
        } else {
            Some(UninitFloatSlot { run: self })
        }
    }
}

impl BignumRun {
    pub(in crate::tagged::gc) fn next_slot(&mut self) -> Option<UninitBignumSlot<'_>> {
        if self.core.empty() {
            None
        } else {
            Some(UninitBignumSlot { run: self })
        }
    }
}

/// The slot is still Uninit. Dropping this temporary leaves selection undone.
pub(in crate::tagged::gc) struct UninitFloatSlot<'run> {
    run: &'run mut FloatRun,
}

pub(in crate::tagged::gc) struct UninitBignumSlot<'run> {
    run: &'run mut BignumRun,
}

impl<'run> UninitFloatSlot<'run> {
    /// The frontend has prechecked charge overflow and rooted destination.
    /// This full write, commit, and immediate sealed handoff cannot safepoint.
    pub(in crate::tagged::gc) fn initialize(self, value: f64) -> InitializedNumeric<'run> {
        let core = &mut self.run.core;
        // SAFETY: selection holds the unique run borrow, and used < slots.
        let ptr = unsafe {
            core.page
                .payload
                .add(core.used * FloatObj::SLOT_BYTES)
                .cast::<FloatObj>()
        };
        // SAFETY: the selected slot is uninitialized exclusive storage. Write
        // the header and payload before publishing its allocation bit.
        unsafe {
            ptr.write(FloatObj {
                header: GcHeader::new_marked(HeapObjectKind::Float, core.parity),
                value,
            });
            core.commit_initialized_slot();
            InitializedNumeric::new(core.owner.heap_identity(), TaggedValue::from_float_ptr(ptr))
        }
    }
}

impl<'run> UninitBignumSlot<'run> {
    pub(in crate::tagged::gc) fn initialize(self, value: Integer) -> InitializedNumeric<'run> {
        let core = &mut self.run.core;
        // SAFETY: selection holds the unique run borrow, and used < slots.
        let ptr = unsafe {
            core.page
                .payload
                .add(core.used * BignumObj::SLOT_BYTES)
                .cast::<BignumObj>()
        };
        // SAFETY: no stale Integer is read or dropped. Move the owned scratch
        // into a complete object before its bit or initialized frontier moves.
        unsafe {
            ptr.write(BignumObj {
                header: VecLikeHeader {
                    gc: GcHeader::new_marked(HeapObjectKind::VecLike, core.parity),
                    type_tag: VecLikeType::Bignum,
                },
                value,
            });
            core.commit_initialized_slot();
            InitializedNumeric::new(
                core.owner.heap_identity(),
                TaggedValue::from_veclike_ptr(ptr.cast()),
            )
        }
    }
}

/// Only complete initialized storage creates this handoff. The run remains
/// borrowed until the existing sealed transport bridge installs the root.
#[must_use = "install the initialized numeric object in its prevalidated root home"]
pub(crate) struct InitializedNumeric<'run> {
    heap: HeapIdentity,
    value: TaggedValue,
    _run: PhantomData<&'run mut ()>,
    _thread: PhantomData<Rc<()>>,
}

impl InitializedNumeric<'_> {
    fn new(heap: HeapIdentity, value: TaggedValue) -> Self {
        Self {
            heap,
            value,
            _run: PhantomData,
            _thread: PhantomData,
        }
    }

    pub(crate) fn into_root_parts(self, _: ConsRootSeal) -> (HeapIdentity, TaggedValue) {
        (self.heap, self.value)
    }
}

static_assertions::assert_not_impl_any!(FloatRun: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(BignumRun: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(UninitFloatSlot<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(UninitBignumSlot<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(InitializedNumeric<'static>: Send, Sync, Clone, Copy);
