//! Proof 2 sealed authority for shared hash tables (design §2.1/§2.2/§2.3).
//!
//! Three lease types are the only callers of the authority word appended to
//! [`HashTableStorage`]: a reader capability minted from a live run access,
//! a writer capability likewise, and the collector's stop-all license minted
//! from [`AllStopped`]. No production Sim/Green path constructs them; the
//! proof executor (stage B3+) is their consumer. The word protocol matches
//! the B1 loom model exactly: readers CAS-increment the low-bit count and
//! never serialise with each other, a writer takes bit 63 with one
//! uncontended CAS from a fully quiescent word — which is the drain — and
//! semantic revalidation across user code stays on `switch_epoch`, not this
//! word.

use std::marker::PhantomData;
use std::ptr::addr_of_mut;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::super::mutator_gc::MutatorGcState;
use super::token::Mutator;
use super::{AllStopped, RegistrationError, RunAccess};
use crate::emacs_core::value::HashTableStorage;
use crate::emacs_core::value::LispHashTable;
use crate::tagged::header::HashTableObj;
use crate::tagged::value::{TAG_MASK, TaggedValue};

/// The owner-state pointer a [`TableMut`] mint keeps for growth charging.
///
/// Null when the owner carries no local GC state (it has not yet run): a
/// growth charge during such an edit bills no one, matching how the
/// registry sums only states that exist.
fn owner_gc_ptr(owner: &mut Mutator<'_>) -> *mut MutatorGcState {
    owner
        .local
        .as_deref_mut()
        .map_or(std::ptr::null_mut(), |state| {
            // SAFETY: `state` is a `&mut OwnerGcState` (the Box deref), so
            // `hot` is initialized and the borrow excludes every other
            // access for its duration.
            unsafe { addr_of_mut!(state.hot) }
        })
}

#[cfg(test)]
pub(crate) use super::authority_word::WRITER;
pub(crate) use super::authority_word::{
    quiescent, reader_enter, reader_exit, writer_enter, writer_exit,
};

/// The shell configuration a reader may observe (B3 op bodies): the
/// immutable-after-creation test, and whether the table was made by
/// `define-hash-table-test` (lookups must run its test/hash functions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TableDescription {
    pub test: crate::emacs_core::value::HashTableTest,
    pub user_defined: bool,
}

/// The §3.1 nesting count taken around one user test/hash callback.
///
/// Minted from a lease (the table is proved rooted), but owns no authority:
/// the callback runs with READER_COUNT and WRITER both released — the
/// accepted protocol — while the table-level count keeps any owner's
/// same-table mutation refusing, exactly GNU's `check_mutable_hash_table`.
/// The count enter is a direct atomic RMW on the shared byte, never routed
/// through the serial edit body, so it flips no Lisp-visible contents and
/// leaves `switch_epoch` untouched. RAII Drop counts back down; `!Send`.
pub struct UserTestHold {
    table: *mut HashTableObj,
    _thread: PhantomData<Rc<()>>,
}

impl UserTestHold {
    fn take(table: *mut HashTableObj) -> Result<Self, RegistrationError> {
        // SAFETY: the pointer came from a lease mint, which validated the
        // object; the byte is table-level metadata, not storage contents.
        let depth = unsafe { &(*table).table.test_depth };
        let was = depth.fetch_add(1, Ordering::AcqRel);
        if was == u8::MAX {
            // Design §3.1: the count saturates defensively and a saturating
            // enter refuses a new nested test rather than wrapping.
            depth.fetch_sub(1, Ordering::Release);
            return Err(RegistrationError::Exhausted);
        }
        Ok(Self {
            table,
            _thread: PhantomData,
        })
    }
}

impl Drop for UserTestHold {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: pairs with the enter at mint; unwind-safe by being the Drop.
        unsafe {
            (*self.table)
                .table
                .test_depth
                .fetch_sub(1, Ordering::Release)
        };
    }
}

static_assertions::assert_not_impl_any!(UserTestHold: Send, Sync, Clone, Copy);

/// The seal. No type outside this module can name it, so no out-of-module
/// `impl SealedTableAccess` can extend the capability surface (fixture (f)).
mod seal {
    pub trait Sealed {}
}

/// The sealed table-access capability: reaching a shared hash table's
/// storage through the authority word. Implemented only by the three lease
/// types below; the private supertrait makes further implementations a
/// compile error.
pub trait SealedTableAccess: seal::Sealed {
    /// The authority word this lease guards. Sealed: only lease types can
    /// expose it, and only the protocol functions above may change it.
    fn authority_word(&self) -> &AtomicU64;

    /// The guarded table. Callers see only what the lease's own methods
    /// hand out; this exists for the lease implementations and in-crate
    /// assertions, not for storage walks.
    fn guarded_table(&self) -> &LispHashTable;
}

/// Validate a table value and take its object pointer. No reference into
/// the payload is created: leases reach storage through the pointer under
/// the word protocol only.
fn table_object(table: TaggedValue) -> Result<*mut HashTableObj, RegistrationError> {
    if table.gc_veclike_type() != Some(crate::tagged::header::VecLikeType::HashTable) {
        return Err(RegistrationError::Publication);
    }
    let ptr = (table.bits() & !TAG_MASK) as *mut HashTableObj;
    if ptr.is_null() || (ptr as usize) & TAG_MASK != 0 {
        return Err(RegistrationError::Publication);
    }
    Ok(ptr)
}

/// A reader capability for one shared hash table (design §2.2).
///
/// Minted only from a live [`RunAccess`] — possession is compiler-checkable
/// evidence of a registered, thread-pinned, same-heap owner. `!Send`,
/// `!Sync`, `!Clone`: the lease cannot cross a thread or alias. Its read
/// methods take the READER_COUNT for the duration of one storage walk and
/// copy whole tagged words out; no `&LispHashTable` and no storage slice
/// escapes. The minting owner's root slot keeps the table object alive for
/// the lease's lifetime.
pub struct TableRef<'access> {
    table: *mut HashTableObj,
    value: TaggedValue,
    _access: PhantomData<&'access ()>,
    _thread: PhantomData<Rc<()>>,
}

/// A writer capability for one shared hash table (design §2.2).
///
/// Minted only from a live [`RunAccess`], which it borrows exclusively for
/// its lifetime: one writer lease per owner at a time (an owner's own
/// in-flight reader would otherwise dead-lock the drain). Its edit methods
/// hold WRITER across the Rust-side edit only — no Lisp, no safepoint, no
/// manager lock — reusing the serial mutation body unchanged.
pub struct TableMut<'access> {
    table: *mut HashTableObj,
    value: TaggedValue,
    /// The minting owner's GC state, charged for storage growth the edit
    /// causes (design §4 Rev R3). Null when the owner has no local state
    /// yet; the exclusive `_access` borrow keeps the state's box alive and
    /// immovable for the lease's whole lifetime.
    owner_gc: *mut MutatorGcState,
    _access: PhantomData<&'access mut ()>,
    _thread: PhantomData<Rc<()>>,
}

/// The collector's stop-all writer license (design §2.2).
///
/// Minted only from [`AllStopped`]: every owner is stopped, so the word
/// must already be quiescent and the license edits without taking WRITER.
/// This is the "other threads cannot reach it any other way" clause for
/// the keeper's weak sweeps and pending-entry handling.
pub struct KeeperTableMut<'stop> {
    table: *mut HashTableObj,
    value: TaggedValue,
    _stop: PhantomData<&'stop ()>,
    _thread: PhantomData<Rc<()>>,
}

impl seal::Sealed for TableRef<'_> {}
impl seal::Sealed for TableMut<'_> {}
impl seal::Sealed for KeeperTableMut<'_> {}

macro_rules! impl_sealed_table_access {
    ($ty:ty) => {
        impl SealedTableAccess for $ty {
            #[inline]
            fn authority_word(&self) -> &AtomicU64 {
                // SAFETY: the mint validated the object and the lease keeps
                // the table alive (rooted value; stop-all for the keeper).
                unsafe { &(*self.table).table.data.authority }
            }

            #[inline]
            fn guarded_table(&self) -> &LispHashTable {
                // SAFETY: same liveness; read-only reach under the lease's
                // own protocol (callers of THIS method are in-crate).
                unsafe { &(*self.table).table }
            }
        }
    };
}

impl_sealed_table_access!(TableRef<'_>);
impl_sealed_table_access!(TableMut<'_>);
impl_sealed_table_access!(KeeperTableMut<'_>);

impl Drop for TableRef<'_> {
    #[inline]
    fn drop(&mut self) {
        // The lease's scope is the storage walk's extent: releasing the
        // count at drop is the protocol (the B1 model's reader-exit assert
        // checks the pairing). A dropped reader lease can never leave WRITER
        // set — readers never hold that bit.
        // SAFETY: the count was taken at mint; this pairs with it.
        let word = unsafe { &(*self.table).table.data.authority };
        reader_exit(word);
    }
}

impl<'access> TableRef<'access> {
    /// Mint a reader lease. The run access proves a registered owner on
    /// this thread; the table value must be one the owner has rooted (B2
    /// tests hold it on their own stack; the B3 executor passes slot
    /// homes). Pending dump entries are hydrated here, before any word is
    /// taken, matching `as_hash_table`'s first-access contract.
    pub fn enter(
        access: &mut RunAccess<'_, 'access>,
        table: TaggedValue,
    ) -> Result<Self, RegistrationError> {
        access.owner.check_thread()?;
        let ptr = table_object(table)?;
        // SAFETY: the mint validated the object; hydration is a one-time
        // serial-world edit performed before any reader counts exist.
        unsafe {
            if (*ptr).table.needs_hydration() {
                (*ptr).table.hydrate_pending();
            }
            reader_enter(&(*ptr).table.data.authority);
        }
        Ok(Self {
            table: ptr,
            value: table,
            _access: PhantomData,
            _thread: PhantomData,
        })
    }

    /// One storage walk under the reader count: `f` sees a table no writer
    /// is editing (writers only proceed after this count drains). Copy
    /// whole tagged words out; revalidation across user code is the
    /// caller's `switch_epoch` protocol, not this word.
    pub fn read<R>(&self, f: impl FnOnce(&HashTableStorage) -> R) -> R {
        // SAFETY: the lease keeps the rooted table alive; the count taken
        // at mint excludes any concurrent writer for the whole walk.
        let storage = unsafe { &(*self.table).table.data };
        f(storage)
    }

    /// Take an independent storage snapshot while writers are excluded.
    /// The unrooted copy is consumed immediately by the sealed shell allocator,
    /// with no Lisp, safepoint or collection between snapshot and root install.
    /// GNU permits copying inside a user test; that copy starts mutable even
    /// while the original table remains guarded by its own nesting count.
    pub(super) fn snapshot_for_copy(&self) -> Result<LispHashTable, RegistrationError> {
        let source = self.guarded_table();
        // The stopped weak-sweep admission covers a closed set of serially
        // prepared shells. A copied weak shell would escape that set. Refuse
        // before cloning backing storage, allocating a shell or publishing it.
        // The ordinary serial copy-hash-table primitive remains unchanged.
        if source.weakness.is_some() {
            return Err(RegistrationError::Publication);
        }
        let table = source.clone();
        table.test_depth.store(0, Ordering::Relaxed);
        debug_assert!(quiescent(&table.data.authority));
        Ok(table)
    }

    /// The shell configuration a sealed read may observe (B3): the test and
    /// whether lookups run user-defined functions. Both are fixed at
    /// creation, so this races nothing a writer may edit.
    pub fn describe(&self) -> TableDescription {
        let table = self.guarded_table();
        TableDescription {
            test: table.test,
            user_defined: table.user_cmp_function.is_some(),
        }
    }

    /// The §3.1 nesting count as the mutating ops read it (GNU's mutability
    /// check): nonzero means some owner is inside this table's test or hash
    /// function and a mutation must refuse.
    pub fn user_test_depth(&self) -> u8 {
        self.guarded_table().test_depth.load(Ordering::Acquire)
    }

    /// Take the §3.1 nesting count for one callback window. The hold owns
    /// no authority: drop the minting lease (and every other lease on this
    /// table) before running the callback, then let this Drop release the
    /// count when the window closes.
    pub fn hold_user_test(&self) -> Result<UserTestHold, RegistrationError> {
        UserTestHold::take(self.table)
    }
}

impl Drop for TableMut<'_> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: WRITER was taken at mint; the edit is done or unwound —
        // either way the Rust-side edit section has ended and the word
        // returns to quiescent.
        let word = unsafe { &(*self.table).table.data.authority };
        writer_exit(word);
    }
}

impl<'access> TableMut<'access> {
    /// Mint a writer lease. Borrows the run access exclusively for the
    /// lease's lifetime: one writer per owner, and never alongside that
    /// owner's own reader lease (the drain would wait on itself).
    pub fn enter(
        access: &mut RunAccess<'_, 'access>,
        table: TaggedValue,
    ) -> Result<Self, RegistrationError> {
        access.owner.check_thread()?;
        let ptr = table_object(table)?;
        // The exclusive `_access` borrow outlives the lease, so the owner's
        // boxed GC state cannot move while this raw pointer is held.
        let owner_gc = owner_gc_ptr(access.owner);
        // SAFETY: the mint validated the object; hydration happens inside
        // the serial edit body itself, under WRITER.
        unsafe {
            writer_enter(&(*ptr).table.data.authority);
        }
        Ok(Self {
            table: ptr,
            value: table,
            owner_gc,
            _access: PhantomData,
            _thread: PhantomData,
        })
    }

    /// GNU's mutability check on the writer side (§3.1): `true` when this
    /// table's nesting count is nonzero and the edit must refuse instead.
    /// Read it before `edit`, mirroring where the serial builtins check.
    pub fn user_test_mutation_refused(&self) -> bool {
        self.guarded_table().test_depth.load(Ordering::Acquire) != 0
    }

    /// One Rust-side edit under WRITER: the drain has already happened at
    /// mint, so `f` is the table's only editor. The body reuses the serial
    /// choke point unchanged — `switch_epoch` bump, plan invalidation,
    /// hydration, heap-write note — one code path, not a fork (design
    /// §2.2). No Lisp, no safepoint, no allocation inside `f`. Storage
    /// growth inside the edit is charged to the minting owner (§4 Rev R3)
    /// HERE, by measuring the table's `sealed_storage_bytes` around the
    /// body in this same inactive window: the serial Vec growth sites stay
    /// exactly main's code, so Sim's pacing is untouched and no TLS is
    /// involved.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut LispHashTable) -> R) -> R {
        let owner_gc = self.owner_gc;
        // SAFETY: WRITER is held, so no sealed reader is in flight and the
        // serial body's single-editor assumption holds; the table is rooted
        // for the lease's lifetime.
        unsafe {
            // Parallel owners have no installed heap TLS. Route the ordinary
            // owned non-cons remembered claim to the lease holder explicitly;
            // the serial choke's TLS note cannot do this on their behalf.
            // Admission excludes concurrent marking and mapped hash tables.
            let header = &(*self.table).header.gc;
            if !owner_gc.is_null() && header.tenured && header.claim_remembered() {
                (*owner_gc).remset.push(self.value);
            }
            crate::tagged::mutate::with_hash_table_mut_inactive(self.value, |ht| {
                let before = ht.data.sealed_storage_bytes();
                let result = f(ht);
                let grown = ht.data.sealed_storage_bytes().saturating_sub(before);
                if grown > 0 && !owner_gc.is_null() {
                    // SAFETY: mint computed this pointer from the owner's
                    // boxed GC state under the exclusive `_access` borrow,
                    // which keeps the box alive and immovable for the
                    // lease's lifetime.
                    (*owner_gc).bytes_since_gc += grown;
                }
                result
            })
            .expect("mint validated the table type")
        }
    }
}

impl Drop for KeeperTableMut<'_> {
    #[inline]
    fn drop(&mut self) {
        // The keeper never takes the word, so there is nothing to release;
        // the Drop exists to mark the license scoped and must_use-like.
    }
}

impl<'stop> KeeperTableMut<'stop> {
    /// Mint the keeper license. At stop-all no owner can run, so the word
    /// must already be quiescent — that assertion IS the safety argument
    /// for editing without the word protocol.
    pub fn enter(
        _stopped: &'stop AllStopped<'_>,
        table: TaggedValue,
    ) -> Result<Self, RegistrationError> {
        let ptr = table_object(table)?;
        // SAFETY: the mint validated the object.
        let word = unsafe { &(*ptr).table.data.authority };
        if !quiescent(word) {
            // A retained owner lease is a broken stop invariant. Refuse before
            // touching payload storage in release builds as well as debug.
            return Err(RegistrationError::Busy);
        }
        Ok(Self {
            table: ptr,
            value: table,
            _stop: PhantomData,
            _thread: PhantomData,
        })
    }

    /// Inspect collector storage without allowing a payload reference to escape.
    pub(in crate::tagged::gc) fn inspect<R>(
        &self,
        f: impl for<'table> FnOnce(&'table LispHashTable) -> R,
    ) -> R {
        // SAFETY: the live AllStopped borrow retained by the lease excludes
        // every owner; the collector retains this allocation during the visit.
        unsafe { f(&(*self.table).table) }
    }

    /// Collector-only storage edit. Unlike a mutator edit, weak sweep has no
    /// pre-store barrier, hydration, revision hook, or TLS access. This scopes
    /// the existing collector retain body; it performs no Lisp or safepoint.
    pub(in crate::tagged::gc) fn edit_storage<R>(
        &mut self,
        f: impl for<'table> FnOnce(&'table mut LispHashTable) -> R,
    ) -> R {
        // SAFETY: the live AllStopped borrow excludes all owners and workers.
        unsafe { f(&mut (*self.table).table) }
    }

    /// One keeper edit without the word protocol: stop-all proves no
    /// sealed reader or writer can exist. Same serial body as [`TableMut`].
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut LispHashTable) -> R) -> R {
        // SAFETY: every owner is stopped; the serial single-editor
        // assumption holds without taking the word.
        unsafe {
            crate::tagged::mutate::with_hash_table_mut_inactive(self.value, f)
                .expect("mint validated the table type")
        }
    }
}

static_assertions::assert_not_impl_any!(TableRef<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(TableMut<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(KeeperTableMut<'static>: Send, Sync, Clone, Copy);

/// The §2.3 convention guard: the restricted executor must reach shared
/// hash tables only through the sealed op bodies added in stage B3 —
/// never by naming the serial choke points or the observation helpers'
/// targets directly. This scans the executor's own source and fails on a
/// banned path, so a future op body that bypasses the seal trips a test
/// rather than silently forking the mutation path.
#[cfg(test)]
#[test]
fn restricted_lisp_does_not_name_the_serial_choke_points() {
    let source = include_str!("restricted_lisp.rs");
    for banned in [
        "with_hash_table_mut",
        "as_hash_table",
        "mutate::",
        "table_authority",
    ] {
        assert!(
            !source.contains(banned),
            "restricted_lisp.rs names `{banned}`: the executor must go through the sealed ops"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_transitions_match_the_b1_model() {
        let word = std::sync::Arc::new(AtomicU64::new(0));
        assert!(quiescent(&word));

        // Parallel readers nest as a count and never set the reserved bit.
        reader_enter(&word);
        reader_enter(&word);
        assert_eq!(word.load(Ordering::Acquire), 2);
        assert!(!quiescent(&word));

        // A writer must drain first: it cannot take WRITER while readers
        // are in flight. The B1 model proved this exhaustively; here the
        // live functions must show the same refusal.
        let writer_word = std::sync::Arc::clone(&word);
        let blocked = std::thread::spawn(move || {
            writer_enter(&writer_word);
            writer_word.load(Ordering::Acquire)
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        // Readers drain; the waiting writer then holds the whole word.
        reader_exit(&word);
        reader_exit(&word);
        let held = blocked.join().expect("writer must finish after the drain");
        assert_eq!(held, WRITER);
        assert!(!quiescent(&word));

        // Writer exit restores full quiescence: readers can enter again.
        writer_exit(&word);
        assert!(quiescent(&word));
        reader_enter(&word);
        assert_eq!(word.load(Ordering::Acquire), 1);
        reader_exit(&word);
        assert!(quiescent(&word));
    }

    #[test]
    fn parallel_reader_counts_never_exceed_their_owners() {
        let word = AtomicU64::new(0);
        let observed = AtomicU64::new(0);
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..1000 {
                        reader_enter(&word);
                        let now = word.load(Ordering::Acquire);
                        observed.fetch_max(now, Ordering::AcqRel);
                        reader_exit(&word);
                    }
                });
            }
        });
        assert!(quiescent(&word), "every count is returned");
        assert!(
            observed.load(Ordering::Acquire) <= 4,
            "the word only counts in-flight readers"
        );
    }

    #[test]
    fn reserved_bits_outside_the_count_are_rejected_by_readers() {
        // A word with WRITER set: readers must refuse (spin) rather than
        // increment under a writer. Verified by observing no count change
        // over a bounded window, then clearing so the probe can finish.
        let word = std::sync::Arc::new(AtomicU64::new(WRITER));
        let reader_word = std::sync::Arc::clone(&word);
        let probe = std::thread::spawn(move || {
            reader_enter(&reader_word);
            reader_word.load(Ordering::Acquire)
        });
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert_eq!(
            word.load(Ordering::Acquire),
            WRITER,
            "no reader count while the writer holds the word"
        );
        writer_exit(&word);
        let entered = probe.join().expect("reader enters after writer exit");
        assert_eq!(entered, 1);
        reader_exit(&word);
        assert!(quiescent(&word));
    }
}
