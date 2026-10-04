//! The write barrier's owner window: the one address range whose owners must
//! leave the inline barrier for its out-of-line part.
//!
//! Every state that makes `record_heap_write` do something is folded into a
//! single `(lo, len)` range over owner addresses:
//!
//! - **ALL** while a concurrent mark runs (the SATB log) or owner tracking is
//!   on (the dirty-owner tables): every heap write is recorded. Also ALL for
//!   the whole life of a heap under the census's remembered-set probe
//!   (`census.rs`), which must see every store.
//! - **The dump span** when only the dump partition is active (the steady
//!   state of a session with a loaded pdump): a write by a mapped owner may
//!   add it to the remembered set.
//! - **EMPTY** otherwise.
//!
//! An owner at address `a` needs the out-of-line barrier iff
//! `a.wrapping_sub(lo) < len`, or it is a non-cons owner that is tenured and
//! not yet remembered (see `barrier_gate`). With generations disabled,
//! a cons outside the window is stored inline. Rust cons stores use a
//! separate protocol mirror, equal to this window with generations disabled
//! and ALL otherwise; the outlined filter then checks generation eligibility.
//! GEN1 compiled stores keep the ordinary window and test the cons bitmap
//! inline. GEN0 observed mode widens only the compiled window with the
//! mutator's collection-read envelope; exact object marks filter those hits.
//!
//! The window is PROTOCOL STATE, like `TAGGED_HEAP_CONCURRENT_ACTIVE`: it is
//! recomputed and republished by [`TaggedHeap::publish_barrier_window`] at
//! every writer of one of its inputs (`set_write_tracking_mode`,
//! `extend_dump_span`, `launch_concurrent_mark`, `join_concurrent_mark`), and
//! re-derived whenever a heap is (re)installed on a thread
//! (`set_tagged_heap`). There is no drop guard, for the reason the
//! concurrent flag has none: its true-window spans many mutator frames.

use super::*;

/// An owner-address range tested with one wrapping subtraction and unsigned
/// compare. A compiled observation range may wrap, excluding one empty gap.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) struct BarrierWindow {
    lo: usize,
    len: usize,
}

/// Prepared protocol geometry from the executing mutator's existing journal.
/// The optional linear gap contains no locally observed owner and no mapped
/// owner. Its complement fits the unchanged native wrapping window predicate.
/// Contexts and certificates remain mutator-local; shared marks are atomic,
/// while only the installed mutator publishes its JitHeapState Cells.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CompiledObservationGate {
    pub(crate) observed: [BarrierWindow; 2],
    pub(crate) excluded: Option<BarrierWindow>,
}

impl CompiledObservationGate {
    pub(crate) const NONE: Self = Self {
        observed: [BarrierWindow::NONE; 2],
        excluded: None,
    };
}

impl BarrierWindow {
    /// No owner is covered.
    pub(crate) const NONE: Self = Self { lo: 0, len: 0 };
    /// Every owner is covered (heap addresses are 8-aligned, so the one
    /// address `usize::MAX` this leaves out is never an owner).
    pub(crate) const ALL: Self = Self {
        lo: 0,
        len: usize::MAX,
    };

    /// The window over `[lo, hi)`; empty when `hi <= lo`.
    pub(crate) fn span(lo: usize, hi: usize) -> Self {
        if hi <= lo {
            Self::NONE
        } else {
            Self { lo, len: hi - lo }
        }
    }

    /// Preserve the raw native predicate, including a wraparound range.
    pub(crate) const fn from_lo_len(lo: usize, len: usize) -> Self {
        Self { lo, len }
    }

    /// Whether an owner at `addr` must take the out-of-line barrier.
    #[inline(always)]
    pub(crate) fn covers(self, addr: usize) -> bool {
        addr.wrapping_sub(self.lo) < self.len
    }

    /// The window's first address (what compiled code subtracts).
    #[inline(always)]
    pub(crate) fn lo(self) -> usize {
        self.lo
    }

    /// The window's length in bytes (what compiled code compares against).
    #[inline(always)]
    pub(crate) fn len(self) -> usize {
        self.len
    }
}

impl TaggedHeap {
    /// Collection history is independent of GC barrier eligibility. A native
    /// generational store outside the owner window calls this before writing,
    /// including immediate values and owners already logged for a minor.
    /// Like the interpreter's setter, it journals and observes the projected
    /// owner. This touches only the mutator's Rust bookkeeping: no Lisp
    /// allocation, callback, collection or safe point can run here.
    #[cfg(feature = "jit")]
    pub(crate) extern "C" fn record_compiled_collection_write(bits: usize) {
        use super::super::collection_reads::{WriteProjection, record_projected_write};
        let owner = TaggedValue::from_bits(bits);
        let projection = if owner.is_cons() {
            WriteProjection::Cons
        } else {
            debug_assert!(owner.is_veclike());
            WriteProjection::VecLike
        };
        let projected = record_projected_write(owner, projection);
        debug_assert!(projected);
    }

    /// The window this heap's current state implies (see the module doc).
    pub(crate) fn barrier_window(&self) -> BarrierWindow {
        if self.concurrent_mark_running
            || self.write_tracking_mode != WriteTrackingMode::Disabled
            || self.census.as_deref().is_some_and(GenCensus::remset_probe)
        {
            BarrierWindow::ALL
        } else if self.partition_dump {
            // `extend_dump_span` sets the partition only with a non-empty
            // span, so `lo < hi` here.
            debug_assert!(self.dump_addr_lo < self.dump_addr_hi);
            BarrierWindow::span(self.dump_addr_lo, self.dump_addr_hi)
        } else {
            BarrierWindow::NONE
        }
    }

    /// The actual dump span, even while ordinary GC tracking requires ALL.
    /// Observation classification must not use an old TLS protocol mirror.
    pub(crate) fn collection_dump_window(&self) -> BarrierWindow {
        if self.partition_dump {
            BarrierWindow::span(self.dump_addr_lo, self.dump_addr_hi)
        } else {
            BarrierWindow::NONE
        }
    }

    /// Derive the Rust cons protocol mirror once per publication. This carries
    /// no mutator log or cache; future parallel mutators need the same window
    /// publication handshake as the existing ordinary mirror.
    pub(super) fn cons_barrier_window(&self, ordinary: BarrierWindow) -> BarrierWindow {
        if self.generational.enabled {
            BarrierWindow::ALL
        } else {
            ordinary
        }
    }

    /// Native GEN0 stores share the existing window test with certificate
    /// owners. The enclosing span is only a coarse rejection filter: its
    /// outlined setter checks the exact sticky mark before journaling.
    pub(super) fn compiled_barrier_window(
        &self,
        ordinary: BarrierWindow,
        gate: CompiledObservationGate,
    ) -> BarrierWindow {
        let [below, above] = gate.observed;
        if self.generational.enabled || (below.len == 0 && above.len == 0) {
            return ordinary;
        }
        if ordinary == BarrierWindow::ALL {
            return ordinary;
        }
        if let Some(gap) = gate.excluded {
            debug_assert_ne!(gap.len, 0);
            debug_assert!(ordinary.len == 0 || !gap.covers(ordinary.lo));
            let hi = gap.lo + gap.len;
            return BarrierWindow::from_lo_len(hi, gap.lo.wrapping_sub(hi));
        }
        if ordinary.len == 0 {
            debug_assert_eq!(above.len, 0, "no dump uses one full observation hull");
            return below;
        }
        let dump_hi = ordinary.lo + ordinary.len;
        if below.len == 0 {
            return BarrierWindow::span(ordinary.lo, dump_hi.max(above.lo + above.len));
        }
        let below_hi = below.lo + below.len;
        debug_assert!(below_hi <= ordinary.lo);
        if above.len == 0 {
            // Retain the dump and lower heap observations, but exclude the
            // growth gap above the last observed heap owner. The native test
            // already wraps, so a never-read allocation in that gap keeps
            // precisely the old outside-window instruction path.
            return if below_hi == ordinary.lo {
                BarrierWindow::span(below.lo, dump_hi)
            } else {
                BarrierWindow::from_lo_len(ordinary.lo, below_hi.wrapping_sub(ordinary.lo))
            };
        }
        debug_assert!(dump_hi <= above.lo);
        // Both sides contain required owners. Exclude the larger of the two
        // actual empty inter-region gaps; no required interval is split.
        // This deliberately keeps a growth gap, not the shortest usize arc.
        let lower_gap = ordinary.lo - below_hi;
        let upper_gap = above.lo - dump_hi;
        if lower_gap == 0 && upper_gap == 0 {
            BarrierWindow::span(below.lo, above.lo + above.len)
        } else if lower_gap >= upper_gap {
            BarrierWindow::from_lo_len(ordinary.lo, below_hi.wrapping_sub(ordinary.lo))
        } else {
            BarrierWindow::from_lo_len(above.lo, dump_hi.wrapping_sub(above.lo))
        }
    }

    /// THE publisher of the barrier window: recompute it from this heap's
    /// state and store it where the barriers read it — the thread-local
    /// mirror the Rust stores test and the heap field compiled code tests
    /// (`JitHeapState`). Called at every writer of an input.
    pub(super) fn publish_barrier_window(&mut self) {
        let window = self.barrier_window();
        let observed = super::super::collection_reads::compiled_observation_gate(
            self.collection_dump_window(),
        );
        self.jit
            .set_barrier_window(self.compiled_barrier_window(window, observed));
        TAGGED_HEAP_BARRIER_WINDOW.with(|w| w.set(window));
        TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.set(self.cons_barrier_window(window)));
    }

    /// Test hook: arm or disarm the concurrent SATB barrier exactly as
    /// `launch_concurrent_mark` / `join_concurrent_mark` do, without a GC
    /// thread (so nothing drains `satb_shared`). Tests used to flip the heap
    /// flag and its thread-local mirror by hand, which would now leave the
    /// barrier window stale and test the old gate.
    #[cfg(test)]
    pub(crate) fn set_concurrent_active_for_test(&mut self, on: bool) {
        self.close_alloc_regions();
        self.concurrent_mark_running = on;
        TAGGED_HEAP_CONCURRENT_ACTIVE.with(|c| c.set(on));
        self.publish_barrier_window();
    }
}

/// Publish the executing mutator's observation envelope before its revision
/// snapshot and object read. This has no allocation, callback or safe point,
/// and does not borrow collection history (its caller may already hold it).
/// Certificates and Context execution remain mutator-local: another Lisp
/// mutator needs shared revision/journal publication, not just this atomic mark.
pub(crate) fn publish_collection_observation_window(observed: CompiledObservationGate) {
    TAGGED_HEAP.with(|slot| {
        let heap = slot.get();
        if heap.is_null() {
            return;
        }
        // SAFETY: installation retains this heap; only its executing mutator
        // publishes JitHeapState Cells. The collector never reads that state.
        let heap = unsafe { &*heap };
        let ordinary = heap.barrier_window();
        heap.jit
            .set_barrier_window(heap.compiled_barrier_window(ordinary, observed));
    });
}

/// Cold native refusal refinement. A certificate-free owner outside every
/// ordinary GC obligation can be stored inline after excluding its empty
/// local interval. This performs Rust bookkeeping only: no Lisp allocation,
/// callback, collection or safe point. Another mutator's shared mark alone
/// does not create a dependency in this mutator's private certificate journal.
#[cfg(feature = "jit")]
#[cold]
#[inline(never)]
pub(crate) extern "C" fn neovm_jit_unobserved_collection_owner(owner_bits: i64) -> i8 {
    use super::super::collection_reads::{CompiledJournalMode, compiled_journal_mode};
    if compiled_journal_mode() != CompiledJournalMode::Observed {
        return 0;
    }
    let bits = owner_bits as usize;
    let owner = TaggedValue::from_bits(bits);
    if !owner.is_cons() {
        return 0;
    }
    let address = bits & !super::super::value::TAG_MASK;
    let eligible = TAGGED_HEAP.with(|slot| {
        let heap = slot.get();
        if heap.is_null() {
            return false;
        }
        // SAFETY: this leaf executes under the installed Context's exclusive
        // mutator scope. The collector does not change these heap protocol
        // inputs; ordinary ALL preserves tracking and concurrent SATB writes.
        let heap = unsafe { &*heap };
        !heap.generational.enabled && !heap.barrier_window().covers(address)
    });
    i8::from(eligible && super::super::collection_reads::exclude_unobserved_compiled_cons(bits))
}

/// Classify this mutator's observations against the currently installed
/// heap's real dump span. No journal borrow, callback or safe point occurs.
pub(crate) fn current_collection_dump_window() -> BarrierWindow {
    TAGGED_HEAP.with(|slot| {
        let heap = slot.get();
        if heap.is_null() {
            BarrierWindow::NONE
        } else {
            // SAFETY: exclusive Context installation retains this heap for
            // its executing mutator; only immutable dump bounds are read.
            unsafe { &*heap }.collection_dump_window()
        }
    })
}

#[cfg(test)]
impl TaggedHeap {
    /// Test hook: drain the SATB buffer the barrier logs pre-images into.
    pub(crate) fn take_satb_shared_for_test(&mut self) -> Vec<TaggedValue> {
        std::mem::take(&mut *self.satb_shared.lock().unwrap())
    }

    /// Test hook: is this owner queued for remembered-child tracing?
    pub(crate) fn is_remembered_for_test(&self, owner: TaggedValue) -> bool {
        self.mapped_remembered.contains(&owner.bits())
            || (self.generational.enabled
                && (self
                    .generational
                    .r_seed
                    .iter()
                    .any(|value| value.bits() == owner.bits())
                    || self.mutators().any(|mutator| {
                        mutator
                            .remset
                            .iter()
                            .any(|value| value.bits() == owner.bits())
                            || mutator.r_mapped_seen.contains(&owner.bits())
                    })))
    }

    /// Test hook: is `value` a tenured (old-generation) heap object?
    pub(crate) fn is_tenured_for_test(&self, value: TaggedValue) -> bool {
        self.value_is_tenured(value)
    }

    /// Test hook: the window compiled code tests (`JitHeapState`).
    pub(crate) fn jit_barrier_window_for_test(&self) -> BarrierWindow {
        self.jit.barrier_window()
    }
}

/// The barrier window this thread's stores currently test.
#[cfg(test)]
pub(crate) fn published_barrier_window() -> BarrierWindow {
    TAGGED_HEAP_BARRIER_WINDOW.with(|w| w.get())
}

/// The protocol window Rust cons stores currently test.
#[cfg(test)]
pub(crate) fn published_cons_barrier_window() -> BarrierWindow {
    TAGGED_HEAP_CONS_BARRIER_WINDOW.with(|w| w.get())
}
