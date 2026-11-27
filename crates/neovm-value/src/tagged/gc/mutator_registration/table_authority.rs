//! Proof 2 sealed authority for shared hash tables (design §2.1/§2.2/§2.3).
//!
//! Three lease types are the only callers of the authority word on
//! [`HashTableObj`]: a reader capability minted from a live run access, a
//! writer capability likewise, and the collector's stop-all license minted
//! from [`AllStopped`]. The word and the user-test depth sit on the object,
//! not in the payload, so replacing the payload cannot clear either one.
//! No production Sim/Green path constructs the leases; the
//! proof executor (stage B3+) is their consumer. The word protocol matches
//! the B1 loom model exactly: readers CAS-increment the low-bit count and
//! never serialise with each other, a writer takes bit 63 with one
//! uncontended CAS from a fully quiescent word — which is the drain — and
//! semantic revalidation across user code stays on `switch_epoch`, not this
//! word.

use std::marker::PhantomData;
use std::ptr::addr_of_mut;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::super::mutator_gc::MutatorGcState;
use super::root_slot::OwnerRootSlot;
use super::token::Mutator;
use super::{AllStopped, RegistrationError, RunAccess};
use crate::tagged::header::HashTableObj;
use crate::tagged::value::{TAG_MASK, TaggedValue};
use crate::value::HashTableStorage;
use crate::value::LispHashTable;

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
            // SAFETY: the caller passes the owner from the lease's `'a`
            // `RunAccess` borrow. That `'a` borrow is the validity proof:
            // it keeps this box alive and immovable for the whole lease.
            unsafe { addr_of_mut!(state.hot) }
        })
}

#[cfg(any(test, feature = "test-support"))]
pub(crate) use super::authority_word::WRITER;
#[cfg(test)]
pub(crate) use super::authority_word::quiescent;
pub(crate) use super::authority_word::{
    reader_enter, reader_exit, try_writer_enter, writer_enter, writer_exit,
};

/// The shell configuration a reader may observe (B3 op bodies): the
/// immutable-after-creation test, and whether the table was made by
/// `define-hash-table-test` (lookups must run its test/hash functions).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TableDescription {
    pub test: crate::value::HashTableTest,
    pub(crate) user_defined: bool,
}

/// The §3.1 nesting count taken around one user test/hash callback.
///
/// Minted from a live [`TableRef`] paired with an [`OwnerRootSlot`] that
/// currently holds this hash table, borrowing that root handle for `'root`
/// (which also bounds `'root` within `'era`) and incrementing the owning
/// mutator's `user_test_holds` counter so root clearing, rendezvous
/// publication (`enter_native`/`acknowledge`), `retire_rooted`, `publish`,
/// and `finish` all refuse while the hold lives. Because every GC
/// publication/collection/retirement boundary on this era is excluded while
/// `user_test_holds != 0` and `'root` bounds the hold within `'era`, no
/// collection can reclaim the `HashTableObj` before `Drop` runs — even if a
/// caller overwrites `root_scratch[root.index]` through `&OwnerRootSlot`.
/// (Enabling GC safepoints during user-test callbacks in the future will
/// require pinning the table root independently of mutable `root_scratch`
/// slots.)
///
/// The hold owns no authority word: the callback runs with READER_COUNT and
/// WRITER both released — the accepted protocol — while the table-level count
/// keeps any owner's same-table mutation refusing, matching GNU's
/// `check_mutable_hash_table`. RAII Drop counts both `test_depth` and the
/// owner hold counter back down; `!Send`.
pub(crate) struct UserTestHold<'root> {
    table: *mut HashTableObj,
    owner_holds: Arc<AtomicUsize>,
    _root: PhantomData<&'root ()>,
    _thread: PhantomData<Rc<()>>,
}

impl<'root> UserTestHold<'root> {
    /// # Safety
    /// `table` must point to a live [`HashTableObj`] belonging to the active
    /// era bounded by `'root`, and `owner_holds` must be the minting owner's
    /// `user_test_holds` counter so that every GC publication, collection
    /// rendezvous, and retirement on that era is refused until this
    /// [`UserTestHold`] drops.
    unsafe fn take(
        table: *mut HashTableObj,
        owner_holds: Arc<AtomicUsize>,
    ) -> Result<Self, RegistrationError> {
        let next_holds = owner_holds
            .load(Ordering::Relaxed)
            .checked_add(1)
            .ok_or(RegistrationError::Exhausted)?;
        // SAFETY: the pointer came from a lease mint, which validated the
        // object; the byte is table-level metadata, not storage contents.
        // CAS prevents a transient `255 -> 0` wrap that a concurrent writer
        // could observe as `test_depth == 0`.
        let depth = unsafe { &(*table).test_depth };
        depth
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |was| {
                was.checked_add(1)
            })
            .map_err(|_| RegistrationError::Exhausted)?;
        owner_holds.store(next_holds, Ordering::Relaxed);
        Ok(Self {
            table,
            owner_holds,
            _root: PhantomData,
            _thread: PhantomData,
        })
    }
}

impl Drop for UserTestHold<'_> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: `'root` bounds this hold within `'era`, and `owner_holds > 0`
        // excluded every GC publication/collection/retirement on this era until
        // this drop runs; pairs with `take`.
        unsafe {
            (*self.table).test_depth.fetch_sub(1, Ordering::Release);
        }
        let current = self.owner_holds.load(Ordering::Relaxed);
        debug_assert!(current > 0, "user_test_holds underflow");
        self.owner_holds
            .store(current.saturating_sub(1), Ordering::Relaxed);
    }
}

static_assertions::assert_not_impl_any!(UserTestHold<'static>: Send, Sync, Clone, Copy);

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
/// must already be quiescent. Enter CASes that quiescent word to WRITER and
/// Drop runs `writer_exit`, so a second lease on the same table returns
/// [`RegistrationError::Busy`] instead of nesting. The `&AllStopped` borrow
/// stays shared: collector visits reborrow it.
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
                unsafe { &(*self.table).authority }
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
        let word = unsafe { &(*self.table).authority };
        reader_exit(word);
    }
}

/// Pairs `writer_enter` / `writer_exit` around the reader-mint hydrate
/// window only. A panic still releases WRITER. This is not a lease.
struct WriterHold(*const AtomicU64);

impl Drop for WriterHold {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: the pointer is the object's authority word, and
        // `writer_enter` succeeded for this window.
        unsafe { writer_exit(&*self.0) }
    }
}

impl TableRef<'_> {
    /// Mint a reader lease. The borrow of the run access is the lease
    /// lifetime, so `cons`, a safepoint, and `finish` are excluded while it
    /// lives. The borrow is shared: two readers from one run stay legal.
    /// Pending dump entries are hydrated under WRITER, then the reader
    /// count is taken. Already-hydrated tables skip the writer CAS.
    ///
    /// `needs_hydration` reads the payload. That read happens only while
    /// this thread holds the reader count or WRITER. Hydration is one-way;
    /// a payload replace that parks entries again is why the check repeats.
    pub fn enter<'a>(
        access: &'a RunAccess<'_, '_>,
        table: TaggedValue,
    ) -> Result<TableRef<'a>, RegistrationError> {
        access.owner.check_thread()?;
        let ptr = table_object(table)?;
        // SAFETY: the mint validated the object. The pending flag is a
        // plain payload field, so it is read only under the reader count
        // or under WRITER. `WriterHold` releases WRITER if hydration panics.
        unsafe {
            let word = std::ptr::addr_of!((*ptr).authority);
            loop {
                reader_enter(&*word);
                if !(*ptr).table.needs_hydration() {
                    break;
                }
                reader_exit(&*word);
                writer_enter(&*word);
                let hold = WriterHold(word);
                if (*ptr).table.needs_hydration() {
                    (*ptr).table.hydrate_pending();
                }
                drop(hold);
            }
        }
        Ok(TableRef {
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
    pub(crate) fn read<R>(&self, f: impl FnOnce(&HashTableStorage) -> R) -> R {
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
        Ok(source.clone())
    }

    /// The shell configuration a sealed read may observe (B3): the test and
    /// whether lookups run user-defined functions. Both are fixed at
    /// creation, so this races nothing a writer may edit.
    pub(crate) fn describe(&self) -> TableDescription {
        let table = self.guarded_table();
        TableDescription {
            test: table.test,
            user_defined: table.user_cmp_function.is_some(),
        }
    }

    /// The §3.1 nesting count as the mutating ops read it (GNU's mutability
    /// check): nonzero means some owner is inside this table's test or hash
    /// function and a mutation must refuse.
    pub(crate) fn user_test_depth(&self) -> u8 {
        // SAFETY: the lease keeps the object alive; the byte is metadata
        // beside the payload, not a storage field.
        unsafe { (*self.table).test_depth.load(Ordering::Acquire) }
    }

    /// Take the §3.1 nesting count for one callback window.
    ///
    /// `root` must be a live [`OwnerRootSlot`] of `run`'s owner that currently
    /// roots this exact hash table. The returned [`UserTestHold`] borrows
    /// `root` (and thus cannot outlive the root handle or its era) and holds
    /// an owner loan blocking root clearing and owner finish until dropped.
    /// Drop the minting lease (and every other lease on this table) before
    /// running the callback, then let this Drop release the count when the
    /// window closes.
    pub(crate) fn hold_user_test<'root, 'era>(
        &self,
        run: &RunAccess<'_, 'era>,
        root: &'root OwnerRootSlot<'era>,
    ) -> Result<UserTestHold<'root>, RegistrationError> {
        root.check(run)?;
        let local = run
            .owner
            .local
            .as_ref()
            .ok_or(RegistrationError::StaleRegistration)?;
        let slot_bits = local
            .root_slot_bits(root.index)
            .ok_or(RegistrationError::StaleRegistration)?;
        if slot_bits != self.value.bits() {
            return Err(RegistrationError::Publication);
        }
        // SAFETY: `self` validated `self.table`, `root.check(run)` verified
        // `root` belongs to this owner's active era (and `'root` bounds the
        // hold within `'era`), `slot_bits == self.value.bits()` verified
        // `self.table` is rooted at mint, and `owner_holds` excludes every GC
        // publication, rendezvous, and retirement on the era until drop.
        unsafe { UserTestHold::take(self.table, Arc::clone(&local.user_test_holds)) }
    }
}

impl Drop for TableMut<'_> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: WRITER was taken at mint; the edit is done or unwound —
        // either way the Rust-side edit section has ended and the word
        // returns to quiescent.
        let word = unsafe { &(*self.table).authority };
        writer_exit(word);
    }
}

impl TableMut<'_> {
    /// Mint a writer lease. The exclusive borrow of the run access is the
    /// lease lifetime: `cons`, a safepoint, and `finish` are excluded while
    /// it lives, and it cannot overlap this owner's reader lease.
    pub fn enter<'a>(
        access: &'a mut RunAccess<'_, '_>,
        table: TaggedValue,
    ) -> Result<TableMut<'a>, RegistrationError> {
        access.owner.check_thread()?;
        let ptr = table_object(table)?;
        // The `'a` borrow of `access` is the validity proof for `owner_gc`:
        // the owner's box cannot move while this lease lives.
        let owner_gc = owner_gc_ptr(access.owner);
        // SAFETY: the mint validated the object; hydration happens inside
        // the serial edit body itself, under WRITER.
        unsafe {
            writer_enter(&(*ptr).authority);
        }
        Ok(TableMut {
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
    pub(crate) fn user_test_mutation_refused(&self) -> bool {
        // SAFETY: the lease keeps the object alive; the byte is metadata.
        unsafe { (*self.table).test_depth.load(Ordering::Acquire) != 0 }
    }

    /// One Rust-side edit under WRITER: the drain has already happened at
    /// mint, so `f` is the table's only editor. The body reuses the serial
    /// choke point unchanged — `switch_epoch` bump, plan invalidation,
    /// hydration, heap-write note — one code path, not a fork (design
    /// §2.2). No Lisp, no safepoint, no allocation inside `f`. Storage
    /// growth inside the edit is charged to the minting owner (§4 Rev R3)
    /// HERE, by measuring the table's `sealed_storage_bytes` around the
    /// body in this same inactive window. The private inactive body stays
    /// uncharged because this wrapper charges its owner's delta. Public
    /// serial wrappers charge backing growth separately; no heap TLS is
    /// involved in this sealed owner's charge.
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut LispHashTable) -> R) -> R {
        let owner_gc = self.owner_gc;
        // SAFETY: WRITER is held, so no sealed reader is in flight and the
        // serial body's single-editor assumption holds; the table is rooted
        // for the lease's lifetime.
        unsafe {
            // Parallel owners have no installed heap TLS. Route the remembered
            // barrier to the lease holder's own `MutatorGcState` explicitly,
            // mirroring `TaggedHeap::remember_owner` (`gc.rs:1706-1742`):
            // - Mapped (`!header.flags.boxed()`) tables insert into
            //   `r_mapped_seen` (merged into `heap.mapped_remembered` at stop,
            //   `collection.rs:955-957`) and push to `remset` so the next
            //   stopped minor scans them.
            // - Permanent boxed (`header.flags.boxed() && header.generation.permanent()`)
            //   tables also record into `r_mapped_seen` so persistent
            //   remembered membership survives across cycles, plus
            //   `claim_remembered()` -> `remset`.
            // - Ordinary tenured boxed tables use `claim_remembered()` -> `remset`.
            // Admission excludes concurrent major marking (`CollectorQuiescence`),
            // so `major_mark` is always false here.
            if !owner_gc.is_null() {
                let header = &(*self.table).header.gc;
                if !header.flags.boxed() {
                    if (*owner_gc).r_mapped_seen.insert(self.value.bits()) {
                        (*owner_gc).remset.push(self.value);
                    }
                } else if header.tenured {
                    if header.generation.permanent() {
                        (*owner_gc).r_mapped_seen.insert(self.value.bits());
                    }
                    if header.claim_remembered() {
                        (*owner_gc).remset.push(self.value);
                    }
                }
            }
            crate::tagged::mutate::with_hash_table_mut_inactive(self.value, |ht| {
                let before = ht.data.sealed_storage_bytes();
                let result = f(ht);
                let grown = ht.data.sealed_storage_bytes().saturating_sub(before);
                if grown > 0 && !owner_gc.is_null() {
                    // SAFETY: the lease's `'a` borrow is the validity proof
                    // for this pointer. Mint took it from the owner's boxed
                    // GC state under that borrow, which keeps the box alive
                    // and immovable for the lease's lifetime.
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
        // SAFETY: enter's CAS took WRITER; this pairs with it on every
        // return path, including unwind. A nested enter sees Busy until
        // this store restores the quiescent word.
        let word = unsafe { &(*self.table).authority };
        writer_exit(word);
    }
}

impl KeeperTableMut<'_> {
    /// Mint the keeper license. One CAS from a quiescent word to WRITER.
    /// A nested lease on the same table returns [`RegistrationError::Busy`]
    /// and does not touch the payload. The stop borrow stays shared.
    pub fn enter<'stop>(
        _stopped: &'stop AllStopped<'_>,
        table: TaggedValue,
    ) -> Result<KeeperTableMut<'stop>, RegistrationError> {
        let ptr = table_object(table)?;
        // SAFETY: the mint validated the object. `try_writer_enter` is one
        // CAS, not a spin: the collector's `expect` must not wait on itself.
        let word = unsafe { &(*ptr).authority };
        if !try_writer_enter(word) {
            // A retained owner lease, or a keeper lease already held on
            // this table, is a broken stop invariant. Refuse before
            // touching payload storage.
            return Err(RegistrationError::Busy);
        }
        Ok(KeeperTableMut {
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

    /// One keeper edit. Enter already holds WRITER for the lease lifetime.
    /// Same serial body as [`TableMut`].
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut LispHashTable) -> R) -> R {
        // SAFETY: enter's CAS holds WRITER for this lease, and the live
        // AllStopped borrow excludes every owner. The serial single-editor
        // assumption holds because that word is held.
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
#[cfg(any(test, feature = "test-support"))]
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
