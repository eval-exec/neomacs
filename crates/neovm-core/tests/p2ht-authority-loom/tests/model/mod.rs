//! Model of the proof-2 hash-table authority word, revalidation epoch and
//! user-test nesting count (`tmp/peer-p2ht/design.md` §2.1, §2.2, §3.1 Rev R2).
//!
//! The table's storage is modeled as one entry of two whole tagged words with
//! exactly two consistent states, plus a backing generation counter standing
//! for the slot/index arrays the real engine reallocates. Nothing here is
//! Lisp, a GC object or unsafe code.
#![allow(dead_code)]

use std::marker::PhantomData;
use std::rc::Rc;

use loom::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering};

/// Bit 63: a writer holds the table. Low bits: in-flight sealed readers.
pub const WRITER: u64 = 1 << 63;
pub const COUNT_MASK: u64 = (1 << 47) - 1;

/// Sequencing spin bound for the scripted actors. The scripts only wait on
/// flags that the other actor is guaranteed to store, so the cap exists to
/// turn a harness bug into a loud failure instead of a hang.
pub const SPIN_CAP: usize = 64;

/// Modeled consistent entry states: `(key, value)` before and after a writer.
pub const K1: usize = 0x10;
pub const V1: usize = 0x11;
pub const K2: usize = 0x20;
pub const V2: usize = 0x21;
/// State written by a removal: both words tombstoned.
pub const K_REMOVED: usize = 0x0;
pub const V_REMOVED: usize = 0x0;

pub struct Table {
    /// The one-word authority: WRITER bit + counted readers (§2.1).
    pub authority: AtomicU64,
    /// `switch_epoch` analogue: bumped before+after every mutation, never by
    /// the guard (§2.1, §3.1).
    pub epoch: AtomicU64,
    /// Rev R2 nesting count standing in for `mutable: bool`.
    pub guard_depth: AtomicU8,
    /// Modeled entry words. A writer publishes K2/V2 (or a removal) with two
    /// stores while holding WRITER.
    pub key_word: AtomicUsize,
    pub val_word: AtomicUsize,
    /// Backing generation standing for the reallocated slot/index arrays.
    pub live_gen: AtomicUsize,
    /// Highest backing generation the (modeled) allocator has freed.
    pub freed_gen: AtomicUsize,
    /// Payload generation. Even: the payload is stable. Odd: a writer is
    /// between the two stores of a replace. A reader loads this only after
    /// `reader_enter` or while it holds WRITER.
    pub pending: AtomicU64,
    /// Stands for `needs_hydration`. The real flag is a plain payload field.
    /// This atomic stands in so the model can assert the load's protocol
    /// instead of racing a non-atomic.
    pub needs_hydration: AtomicBool,
}

impl Table {
    pub fn new() -> Self {
        Self {
            authority: AtomicU64::new(0),
            epoch: AtomicU64::new(0),
            guard_depth: AtomicU8::new(0),
            key_word: AtomicUsize::new(K1),
            val_word: AtomicUsize::new(V1),
            live_gen: AtomicUsize::new(1),
            freed_gen: AtomicUsize::new(0),
            pending: AtomicU64::new(0),
            needs_hydration: AtomicBool::new(false),
        }
    }

    /// Sealed reader enter (§2.1): refuse while WRITER is set, otherwise
    /// CAS-increment the count. Parallel readers never block each other.
    pub fn reader_enter(&self) {
        let mut spins = 0;
        loop {
            let word = self.authority.load(Ordering::Acquire);
            if word & WRITER == 0 {
                let count = word & COUNT_MASK;
                assert!(count < COUNT_MASK, "reader count overflow");
                let got = self.authority.compare_exchange_weak(
                    word,
                    word + 1,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                );
                if got.is_ok() {
                    return;
                }
            }
            spins += 1;
            assert!(spins < SPIN_CAP, "reader_enter spin cap exceeded");
            loom::thread::yield_now();
        }
    }

    pub fn reader_exit(&self) {
        let was = self.authority.fetch_sub(1, Ordering::Release);
        assert!(was & COUNT_MASK > 0, "reader_exit without a reader count");
        assert!(was & WRITER == 0, "reader count held across a writer");
    }

    /// Witness accessor: in-flight reader count right now (assertions only).
    pub fn reader_count_now(&self) -> u64 {
        self.authority.load(Ordering::Acquire) & COUNT_MASK
    }

    /// Sealed writer enter (§2.1): one CAS from a fully quiescent word, so a
    /// writer only proceeds after every in-flight reader has drained.
    pub fn writer_enter(&self) {
        let mut spins = 0;
        loop {
            let got = self.authority.compare_exchange_weak(
                0,
                WRITER,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            if got.is_ok() {
                let now = self.authority.load(Ordering::Acquire);
                assert_eq!(now, WRITER, "writer entered with readers in flight");
                return;
            }
            spins += 1;
            assert!(spins < SPIN_CAP, "writer_enter drain spin cap exceeded");
            loom::thread::yield_now();
        }
    }

    pub fn writer_exit(&self) {
        self.authority.store(0, Ordering::Release);
    }

    /// Load the payload generation. Legal only under the reader count or
    /// WRITER, and the generation must be even: the odd window is the
    /// replace's unsynchronized middle, which those holds exclude.
    pub fn observe_pending(&self) -> u64 {
        let word = self.authority.load(Ordering::Acquire);
        let under_reader = word & WRITER == 0 && word & COUNT_MASK > 0;
        let under_writer = word & WRITER != 0 && word & COUNT_MASK == 0;
        assert!(
            under_reader || under_writer,
            "payload generation loaded without a word"
        );
        let generation = self.pending.load(Ordering::Acquire);
        assert_eq!(generation % 2, 0, "payload generation observed mid-replace");
        generation
    }

    /// Reader enter for an unhydrated table. The pending flag is read only
    /// under the reader count. Hydration runs under WRITER, then the reader
    /// count is taken again and dropped before return so a concurrent
    /// writer's drain can finish.
    pub fn reader_enter_hydrating(&self) {
        let mut passes = 0;
        loop {
            passes += 1;
            assert!(passes <= 3, "hydration enter did not settle");
            self.reader_enter();
            let _generation = self.observe_pending();
            if !self.needs_hydration.load(Ordering::Acquire) {
                self.reader_exit();
                return;
            }
            self.reader_exit();
            self.writer_enter();
            let _generation = self.observe_pending();
            if self.needs_hydration.load(Ordering::Acquire) {
                self.replace_payload();
            }
            self.writer_exit();
        }
    }

    /// Payload replace under WRITER. The odd generation is the window a
    /// reader that skips the word can observe. The published payload is
    /// hydrated, so a concurrent enter settles in one extra pass.
    pub fn replace_payload(&self) {
        let word = self.authority.load(Ordering::Acquire);
        assert_ne!(word & WRITER, 0, "replace without WRITER");
        assert_eq!(word & COUNT_MASK, 0, "replace with readers");
        let generation = self.pending.load(Ordering::Acquire);
        assert_eq!(generation % 2, 0, "replace started mid-replace");
        self.pending.store(generation + 1, Ordering::Release);
        loom::thread::yield_now();
        self.key_word.store(K2, Ordering::Release);
        self.val_word.store(V2, Ordering::Release);
        self.needs_hydration.store(false, Ordering::Release);
        self.pending.store(generation + 2, Ordering::Release);
    }

    /// Publish the (K2, V2) mutation under WRITER, with the before+after
    /// epoch bumps of the real choke point (mutate.rs:455-460).
    pub fn writer_mutate(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.key_word.store(K2, Ordering::Release);
        self.val_word.store(V2, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    /// Remove the entry under WRITER (remhash shape).
    pub fn writer_remove(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
        self.key_word.store(K_REMOVED, Ordering::Release);
        self.val_word.store(V_REMOVED, Ordering::Release);
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    /// Growth under WRITER: a fresh backing generation, the old one freed.
    /// Legal only because the drain guarantees zero readers on the old gen.
    pub fn writer_grow(&self) {
        let prev = self.live_gen.fetch_add(1, Ordering::AcqRel);
        self.freed_gen.store(prev, Ordering::Release);
    }

    /// A sealed reader's copy of the entry: whole words, under the count.
    pub fn copy_entry(&self) -> (usize, usize) {
        let value = self.val_word.load(Ordering::Acquire);
        loom::thread::yield_now();
        let key = self.key_word.load(Ordering::Acquire);
        (key, value)
    }

    pub fn entry_consistent(entry: (usize, usize)) -> bool {
        matches!(entry, (K1, V1) | (K2, V2) | (K_REMOVED, V_REMOVED))
    }

    /// Rev R2 guard: enter a user test/hash function. A direct RMW, not a
    /// writer choke; flips no Lisp-visible contents, bumps no epoch.
    pub fn guard_enter(&self) {
        let depth = self.guard_depth.fetch_add(1, Ordering::AcqRel) + 1;
        assert!(depth > 0, "test depth wrapped");
    }

    pub fn guard_exit(&self) {
        let depth = self.guard_depth.fetch_sub(1, Ordering::Release);
        assert!(depth > 0, "guard_exit without a guard held");
    }

    /// `check_mutable_hash_table` under N mutators (§3.1 Rev R2): any owner's
    /// mutation signals while ANY owner is inside a test on this table.
    pub fn check_mutable(&self) -> Result<(), &'static str> {
        if self.guard_depth.load(Ordering::Acquire) != 0 {
            Err("hash table test modifies table")
        } else {
            Ok(())
        }
    }
}

/// Thread-confined lease stand-ins (§2.2): only the enter/exit protocol mints
/// them, and they cannot cross a thread.
pub struct ReaderLease<'t> {
    pub table: &'t Table,
    _confined: PhantomData<Rc<()>>,
}

pub struct WriterLease<'t> {
    pub table: &'t Table,
    _confined: PhantomData<Rc<()>>,
}

impl<'t> ReaderLease<'t> {
    pub fn enter(table: &'t Table) -> Self {
        table.reader_enter();
        Self {
            table,
            _confined: PhantomData,
        }
    }

    pub fn copy_entry(&self) -> (usize, usize) {
        self.table.copy_entry()
    }
}

impl Drop for ReaderLease<'_> {
    fn drop(&mut self) {
        self.table.reader_exit();
    }
}

impl<'t> WriterLease<'t> {
    pub fn enter(table: &'t Table) -> Self {
        table.writer_enter();
        Self {
            table,
            _confined: PhantomData,
        }
    }

    pub fn mutate(&self) {
        self.table.writer_mutate();
    }

    pub fn grow(&self) {
        self.table.writer_grow();
    }
}

impl Drop for WriterLease<'_> {
    fn drop(&mut self) {
        self.table.writer_exit();
    }
}

/// RAII user-test hold (Rev R2): the Drop is the unwind-safe fetch_sub that
/// replaces GNU's specpdl-recorded `restore_mutability`.
pub struct GuardHold<'t> {
    table: &'t Table,
    _confined: PhantomData<Rc<()>>,
}

impl<'t> GuardHold<'t> {
    pub fn enter(table: &'t Table) -> Self {
        table.guard_enter();
        Self {
            table,
            _confined: PhantomData,
        }
    }
}

impl Drop for GuardHold<'_> {
    fn drop(&mut self) {
        self.table.guard_exit();
    }
}

static_assertions::assert_not_impl_any!(ReaderLease<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(WriterLease<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(GuardHold<'static>: Send, Sync);

/// Bounded wait for a sequencing flag: the scripts only wait on stores the
/// other actor is guaranteed to make.
pub fn wait_for(flag: &loom::sync::atomic::AtomicBool) {
    let mut spins = 0;
    while !flag.load(Ordering::Acquire) {
        spins += 1;
        assert!(spins < SPIN_CAP, "sequencing spin cap exceeded");
        loom::thread::yield_now();
    }
}

/// Blocking sequencing latch for scripts that chain more than one wait:
/// a waiter leaves the runnable set instead of yielding in a spin loop,
/// which keeps Loom's exploration of deep flag chains tractable. The
/// mutex/condvar edges carry the same happens-before the atomic flags do.
#[derive(Default)]
pub struct Latch {
    flag: loom::sync::Mutex<bool>,
    ready: loom::sync::Condvar,
}

impl Latch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self) {
        let mut guard = self.flag.lock().unwrap();
        *guard = true;
        drop(guard);
        self.ready.notify_all();
    }

    pub fn wait(&self) {
        let mut guard = self.flag.lock().unwrap();
        while !*guard {
            guard = self.ready.wait(guard).unwrap();
        }
    }
}
