//! Tier-H hash snapshots: exact ownership, reader leases and retirement.
//!
//! Capture is admitted through `SingleMutatorWorld` at the start handshake and
//! records its heap identity. The worker reads only copied
//! callbacks and initialized key/value words in the original slots allocation.
//! One entry mutex serializes all writers and the complete worker read lease.
//! Writer policy either retires the original before reader admission, or defers
//! a still-unclaimed table. A finished reader never accesses its backing again.

use super::{CollectionScope, HashTableObj, TaggedValue, VecLikeType, load_value_atomic};
use crate::emacs_core::value::{HashTableEntry, HashTableStorage};
use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Mutex, MutexGuard};

const UNTOUCHED: u8 = 0;
const COPYING: u8 = 1;
const READY: u8 = 2;

const AVAILABLE: u8 = 0;
const READING: u8 = 1;
const DONE: u8 = 2;
const DEFERRED: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HashTableScanPolicy {
    /// Keep the original once-per-cycle copy behavior for comparison.
    AlwaysClone,
    /// Copy only before reader admission; DONE proves no reader can remain.
    CloneUntilTraced,
    /// First writer cancels an unclaimed scan; a running reader completes first.
    DeferWrites,
}

pub(crate) struct HashTableScanEntry {
    slots: *const Option<HashTableEntry>,
    slots_len: usize,
    callbacks: [Option<TaggedValue>; 2],
    reader: AtomicU8,
    cow: AtomicU8,
    dirty: AtomicBool,
    mutation: Mutex<()>,
}

/// Capture records constant-sized metadata per table. Entries live inline in
/// the exact-address map, without per-table Arc/Vecs or an eager slots walk.
pub(crate) struct HashTableScanSnapshot {
    entries: FxHashMap<usize, HashTableScanEntry>,
    /// The admitted heap; zero only for unbound test snapshots, which a heap
    /// refuses to install.
    heap_identity: usize,
    policy: HashTableScanPolicy,
    slots_len: usize,
    initialized_entries: usize,
    poisoned: AtomicBool,
}

// SAFETY: an admitted capture proves live owned table addresses of one heap.
// Before reader admission, policy keeps the original unchanged/retired or
// atomically cancels admission. The reader holds the entry mutex through every
// backing read; all writers acquire that same mutex before any payload borrow.
// DONE/DEFERRED forbid later reads even if resize has freed the captured backing.
// Retired originals are freed only after an explicit finish proves reader exit;
// abandonment retains them with the heap's other marker-readable storage. Bare
// captured pointers are never dereferenced without the entry's read lease.
unsafe impl Send for HashTableScanSnapshot {}
unsafe impl Sync for HashTableScanSnapshot {}
static_assertions::assert_impl_all!(HashTableScanSnapshot: Send, Sync);
static_assertions::assert_not_impl_any!(HashTableScanSnapshot: Clone);

impl HashTableScanSnapshot {
    /// An unbound snapshot for protocol tests that never install it in a heap.
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_capacity(0, 0)
    }

    #[cfg(test)]
    pub(crate) fn with_capacity(tables: usize, _initialized_entries: usize) -> Self {
        Self::unbound_with_policy(tables, HashTableScanPolicy::CloneUntilTraced)
    }

    #[cfg(test)]
    pub(crate) fn unbound_with_policy(tables: usize, policy: HashTableScanPolicy) -> Self {
        Self::with_identity(tables, policy, 0)
    }

    /// Build an empty snapshot for the admitted heap's start handshake.
    pub(crate) fn with_policy(
        tables: usize,
        policy: HashTableScanPolicy,
        world: &super::scan_contract::SingleMutatorWorld<'_>,
    ) -> Self {
        Self::with_identity(tables, policy, world.heap_identity())
    }

    fn with_identity(tables: usize, policy: HashTableScanPolicy, heap_identity: usize) -> Self {
        Self {
            entries: FxHashMap::with_capacity_and_hasher(tables, Default::default()),
            heap_identity,
            policy,
            slots_len: 0,
            initialized_entries: 0,
            poisoned: AtomicBool::new(false),
        }
    }

    /// The heap whose start handshake admitted this snapshot.
    pub(crate) fn heap_identity(&self) -> usize {
        self.heap_identity
    }

    /// Capture one registry-proven owned Box hash table. Eligibility remains
    /// fixed for this cycle even if its current contents or weakness change.
    /// Permanently/generation-black owners need no worker claim or snapshot.
    ///
    /// # Safety
    /// `owner` must address a live, complete owned `HashTableObj` of this
    /// snapshot's admitted heap, captured during its admission. The Box cannot
    /// be freed until worker join or abandonment. Every later mutable accessor
    /// must hold this snapshot's guard before borrowing the payload; backing
    /// storage may change only according to its reader policy.
    pub(crate) unsafe fn capture_owned(&mut self, owner: usize, scope: CollectionScope) -> bool {
        if self.entries.contains_key(&owner) {
            return false;
        }
        let object = unsafe { &*(owner as *const HashTableObj) };
        debug_assert_eq!(object.header.type_tag, VecLikeType::HashTable);
        let table = &object.table;
        if object.header.gc.black_by_generation(scope)
            || table.weakness.is_some()
            || table.needs_hydration()
        {
            return false;
        }
        let (slots, slots_len) = table.data.concurrent_slots_snapshot();
        self.slots_len += slots_len;
        self.initialized_entries += table.data.len();
        self.entries.insert(
            owner,
            HashTableScanEntry {
                slots,
                slots_len,
                callbacks: [table.user_cmp_function, table.user_hash_function],
                reader: AtomicU8::new(AVAILABLE),
                cow: AtomicU8::new(UNTOUCHED),
                dirty: AtomicBool::new(false),
                mutation: Mutex::new(()),
            },
        );
        true
    }

    /// Exact address lookup must precede the worker's header dereference.
    /// The mutator keeps its snapshot Arc alive while borrowing/locking this
    /// entry, rather than allocating an Arc for every table.
    #[inline]
    pub(crate) fn get(&self, owner: usize) -> Option<&HashTableScanEntry> {
        self.entries.get(&owner)
    }

    /// The guard borrows the snapshot's poison flag as well as its entry;
    /// retaining the snapshot Arc keeps both alive through the activation.
    /// Acquire before the preimage barrier, which also reads current slots.
    #[cold]
    #[inline(never)]
    pub(crate) fn lock_mutation(&self, owner: usize) -> Option<HashTableMutationGuard<'_>> {
        let entry = self.get(owner)?;
        assert!(!self.is_poisoned(), "Tier-H snapshot poisoned");
        Some(entry.lock_mutation(&self.poisoned, self.policy))
    }

    /// Elect a reader before claiming its header, retaining the same mutex
    /// used by every writer through the full scan. None forbids backing reads:
    /// DEFERRED requires mutator tracing; DONE is already handled.
    pub(crate) fn lock_reader<'a>(
        &'a self,
        entry: &'a HashTableScanEntry,
    ) -> Option<HashTableReadGuard<'a>> {
        assert!(!self.is_poisoned(), "Tier-H snapshot poisoned");
        // Terminal phases are monotonic. DONE publishes every routed child;
        // DEFERRED canceled admission before any worker header claim.
        if matches!(entry.reader.load(Ordering::Acquire), DONE | DEFERRED) {
            return None;
        }
        let lock = entry
            .mutation
            .lock()
            .expect("Tier-H mutation lock poisoned");
        assert!(!self.is_poisoned(), "Tier-H snapshot poisoned");
        match entry
            .reader
            .compare_exchange(AVAILABLE, READING, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => Some(HashTableReadGuard {
                entry,
                poisoned: &self.poisoned,
                scan_started: false,
                scan_complete: false,
                finished: false,
                _lock: lock,
            }),
            Err(DONE | DEFERRED) => None,
            Err(_) => panic!("Tier-H reader left an incomplete lease"),
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn slot_count(&self) -> usize {
        self.slots_len
    }

    pub(crate) fn initialized_entry_count(&self) -> usize {
        self.initialized_entries
    }

    pub(crate) fn descriptor_bytes(&self) -> usize {
        0
    }

    /// Every unwound/incomplete guard release-publishes into this single flag,
    /// making the coordinator's failure check O(1), independent of table count.
    pub(crate) fn is_poisoned(&self) -> bool {
        self.poisoned.load(Ordering::Acquire)
    }

    /// Test helper for a single complete lease; DONE cannot be scanned again.
    ///
    /// # Safety
    /// `entry` must belong to this current-cycle snapshot. Capture and every
    /// writer must obey its mutex/retirement protocol.
    #[cfg(test)]
    pub(crate) unsafe fn scan_entry(
        &self,
        entry: &HashTableScanEntry,
        push: impl FnMut(TaggedValue),
    ) {
        let mut reader = self
            .lock_reader(entry)
            .expect("Tier-H scan already handled");
        unsafe { reader.scan(push) };
        reader.finish();
    }
}

impl HashTableScanEntry {
    /// Acquire before the preimage barrier, not merely before the write:
    /// current-child enumeration must also be serialized against writers.
    #[cold]
    #[inline(never)]
    fn lock_mutation<'a>(
        &'a self,
        poisoned: &'a AtomicBool,
        policy: HashTableScanPolicy,
    ) -> HashTableMutationGuard<'a> {
        let lock = self.mutation.lock().expect("Tier-H mutation lock poisoned");
        assert!(
            !poisoned.load(Ordering::Acquire),
            "Tier-H snapshot poisoned"
        );
        if policy == HashTableScanPolicy::DeferWrites
            && !matches!(self.reader.load(Ordering::Acquire), DONE | DEFERRED)
        {
            match self.reader.compare_exchange(
                AVAILABLE,
                DEFERRED,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) | Err(DONE | DEFERRED) => {}
                Err(_) => panic!("Tier-H writer acquired an incomplete reader lease"),
            }
        }
        HashTableMutationGuard {
            entry: self,
            poisoned,
            policy,
            _lock: lock,
        }
    }

    pub(crate) fn is_deferred(&self) -> bool {
        self.reader.load(Ordering::Acquire) == DEFERRED
    }

    pub(crate) fn reader_done(&self) -> bool {
        self.reader.load(Ordering::Acquire) == DONE
    }
}

pub(crate) struct HashTableReadGuard<'a> {
    entry: &'a HashTableScanEntry,
    poisoned: &'a AtomicBool,
    scan_started: bool,
    scan_complete: bool,
    finished: bool,
    _lock: MutexGuard<'a, ()>,
}

impl HashTableReadGuard<'_> {
    /// Read all initialized fields before honoring stop. Typed Option matching
    /// is safe under this lease: writers cannot change the original, and any
    /// pre-admission copy retired it before mutating the live replacement.
    ///
    /// # Safety
    /// Capture/writers must obey this snapshot's lifetime protocol. The caller
    /// must route every child before finishing a freshly claimed header.
    pub(crate) unsafe fn scan(&mut self, mut push: impl FnMut(TaggedValue)) {
        assert!(
            !self.finished && !self.scan_started,
            "Tier-H reader scanned twice"
        );
        self.scan_started = true;
        // SAFETY: capture recorded a complete typed Vec allocation. Reader
        // admission and the held mutex keep it alive/immutable for this slice.
        let slots = unsafe { std::slice::from_raw_parts(self.entry.slots, self.entry.slots_len) };
        for entry in slots.iter().flatten() {
            // No raw Option representation, None payload or padding is read.
            // Both addresses come from the initialized typed Some variant.
            push(load_value_atomic(&entry.key));
            push(load_value_atomic(&entry.value));
        }
        for callback in self.entry.callbacks.into_iter().flatten() {
            push(callback);
        }
        self.scan_complete = true;
    }

    /// Publish quiescence after a complete scan, or after a failed header claim
    /// that owed no scan. Queued descendants remain owned by the job/stop handoff.
    pub(crate) fn finish(&mut self) {
        assert!(!self.finished, "Tier-H reader finished twice");
        assert!(
            !self.scan_started || self.scan_complete,
            "Tier-H scan interrupted"
        );
        self.entry.reader.store(DONE, Ordering::Release);
        self.finished = true;
    }
}

impl Drop for HashTableReadGuard<'_> {
    fn drop(&mut self) {
        if !self.finished || std::thread::panicking() {
            self.poisoned.store(true, Ordering::Release);
        }
    }
}

pub(crate) struct HashTableMutationGuard<'a> {
    entry: &'a HashTableScanEntry,
    poisoned: &'a AtomicBool,
    policy: HashTableScanPolicy,
    _lock: MutexGuard<'a, ()>,
}

impl Drop for HashTableMutationGuard<'_> {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        if std::thread::panicking() || self.entry.cow.load(Ordering::Acquire) == COPYING {
            self.poisoned.store(true, Ordering::Release);
        }
    }
}

impl HashTableMutationGuard<'_> {
    /// Detach on the cycle's first write and transfer ownership before READY.
    /// The returned original must remain in this mutator's log until the
    /// coordinator completes termination after joining all snapshot readers.
    #[cold]
    #[inline(never)]
    pub(crate) fn clone_on_first_write(
        &mut self,
        storage: &mut HashTableStorage,
        retired: &mut Vec<Vec<Option<HashTableEntry>>>,
    ) -> bool {
        match self.policy {
            HashTableScanPolicy::AlwaysClone => {}
            HashTableScanPolicy::CloneUntilTraced => {
                match self.entry.reader.load(Ordering::Acquire) {
                    DONE => return false,
                    AVAILABLE => {}
                    _ => panic!("Tier-H COW attempted outside a writer-admitted phase"),
                }
            }
            HashTableScanPolicy::DeferWrites => {
                assert!(self.entry.is_deferred() || self.entry.reader_done());
                return false;
            }
        }
        match self.entry.cow.compare_exchange(
            UNTOUCHED,
            COPYING,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {}
            Err(READY) => return false,
            Err(_) => panic!("Tier-H COW was not completed under its mutation guard"),
        }
        assert_eq!(
            storage.concurrent_slots_snapshot(),
            (self.entry.slots, self.entry.slots_len),
            "Tier-H original changed before first-write retirement",
        );
        retired.push(storage.clone_slots_for_concurrent_mark());
        self.entry.cow.store(READY, Ordering::Release);
        true
    }

    /// Admit one CURRENT-child retrace into the winning mutator's local log.
    #[inline]
    pub(crate) fn claim_dirty(&self) -> bool {
        if self.entry.dirty.load(Ordering::Acquire) {
            return false;
        }
        self.entry
            .dirty
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

#[cfg(test)]
#[path = "tests/concurrent_hash_tests.rs"]
mod tests;
