//! L6.4, the suspend half (SERIAL-TURN-CONTEXT r1 point 3, E5.1-TYPES §2
//! `end`): the typed proofs that a serial turn took its owner off this OS
//! thread, and the one way back onto it.
//!
//! Detachment first refuses, in release builds too, when this Context holds a
//! Green thread seat (`HostRefusal::Seated`), or unless this Context's own
//! heap is the one installed on the thread (`HostRefusal::NotInstalled`): a
//! Context swapped into the turn would otherwise take down the live Context's
//! TLS. It then runs in a fixed order:
//! 1. the owner's host bindings swap out (`park_owner`, GNU's
//!    `unbind_for_thread_switch`, src/thread.c:95-99). This writes buffer
//!    storage, so the heap must still be installed;
//! 2. when a Green runtime exists and this Context on this OS thread currently
//!    holds the World's carried state and warm carrier, both leave this thread
//!    into `GreenRuntime` (`stow_serial_world_state`). A Sim World, or a
//!    further owner whose World carriers are held elsewhere or already stowed,
//!    skips and records `DetachedWorldCarriers::NotStowed`;
//! 3. any open bump-allocation regions on this Context's heap are closed
//!    (`close_alloc_regions`, L6.7c (b)), so the next keeper stop or collection
//!    never meets `ImageError::RegionsOpen`;
//! 4. the carrier TLS goes down (`park_carrier_tls`): the heap slot, its
//!    hot-path mirrors and every heap-pointer record are null, so a
//!    collection while the owner is detached cannot meet a stale pointer on
//!    this thread.
//!
//! Reattachment reverses it in Green's order (`green/switch.rs`). It first
//! refuses, in release builds too, when this thread already has a heap
//! installed: one ACTIVE Context per OS thread, any number dormant. It then:
//! 1. clears the deferred caches (`reset_carrier_tls`), as a reused Green
//!    carrier does, before restoring carried TLS so `value` and
//!    `collection_reads` resets cannot clobber restored counters;
//! 2. acquires the World's carried TLS and warm `OwnerCarried` when they are
//!    stowed in `GreenRuntime` (`unstow_serial_world_state`);
//! 3. reinstalls the owner's TLS and swaps its host bindings back in
//!    (`resume_owner`), so terminal reconciliation and display-variable
//!    swap-in invalidations land on the restored World state rather than
//!    placeholders.
//!
//! Scope: [`DetachedTls`] covers the carrier's heap-pointer TLS only.
//! A serial owner is not entered on the thread manager and is not a
//! mutator-census entry, so this detach removes nothing from those rosters.
//! The World's owner slab stays registered: `trace_owner_slabs` traces every
//! owner whether or not it is running. A later activation that registers
//! this Context on either roster removes it in this same detach, before the
//! detach returns.
//!
//! The proofs are `!Send`: each one is a fact about this OS thread. They are
//! produced together and consumed together, so a half-detached owner cannot
//! exist, and a panic between these steps aborts the process. Only
//! [`SerialHost`] reaches the two steps, and only `OwnerSerialState` can call
//! it, so a proof always lies beside its own Context in that state's
//! `DormantContext`. It is dropped only together with that Context, and
//! dropping a dormant Context is ordinary teardown.

use super::super::Context;
use super::super::ParkedOwner;
use super::super::green::carrier_tls::{park_carrier_tls, reset_carrier_tls};
use super::serial_turn::{HostTransition, SerialHost};
use std::marker::PhantomData;

/// The owner's in-place buffer bindings are out of shared host storage.
#[must_use = "a detached owner returns only through `SerialHost::reattach`"]
#[derive(Debug)]
pub(crate) struct DetachedHostBindings {
    parked: ParkedOwner,
}

/// This OS thread's carrier heap-pointer TLS names no heap.
#[must_use = "a detached owner returns only through `SerialHost::reattach`"]
#[derive(Debug)]
pub(crate) struct DetachedTls {
    _thread: PhantomData<*const ()>,
}

/// Whether this detachment stowed the World's carried TLS and warm
/// `OwnerCarried` into `GreenRuntime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetachedWorldCarriers {
    /// Sim World, or this Context did not hold the World's carried state when
    /// it detached (either held on another thread or already stowed).
    NotStowed,
    /// This Context held the World's carried TLS and warm `OwnerCarried` on
    /// this OS thread, and this detach stowed both into `GreenRuntime`.
    Stowed,
}

static NEXT_SERIAL_PARK_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_serial_park_id() -> std::num::NonZeroU64 {
    let id = NEXT_SERIAL_PARK_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    std::num::NonZeroU64::new(id).expect("serial park id counter exhausted")
}

/// Ownership of a serial-turn parked `RootBatchLease` across a mid-turn keeper
/// collection (L6.7c (c) F4):
/// - On a Green World, the lease is stored inside `GreenRuntime::parked`
///   under a unique key so an abnormal owner unwind never triggers
///   `RootBatchLease::drop`'s `std::mem::forget`; `GreenRuntime::finish_terminated_roots`
///   retires any remaining lease cleanly at World teardown.
/// - On a non-Green Sim era without a `GreenRuntime`, the lease stays inline
///   and dropping it unretired aborts fail-closed (matching Q3).
#[derive(Debug)]
enum SerialParkedLease {
    Green(std::num::NonZeroU64),
    Inline(Option<neovm_value::tagged::transport::RootBatchLease>),
}

impl Drop for SerialParkedLease {
    fn drop(&mut self) {
        if let Self::Inline(Some(_)) = self {
            tracing::error!(
                "non-Green serial-turn parked RootBatchLease dropped unretired without a GreenRuntime; aborting fail-closed (L6.7c (c) Q3/F4)"
            );
            std::process::abort();
        }
    }
}

/// Parked private roots published across a mid-turn keeper collection
/// (L6.7c (c)), kept beside `DetachedOwner` so reattach restores JIT pins
/// and retires the `RootBatchLease` under Q2 in exact reverse order.
#[derive(Debug)]
struct SerialParkedRoots {
    lease: SerialParkedLease,
    #[cfg(feature = "jit")]
    jit: crate::emacs_core::jit::green_owner::ParkedJit,
}

/// Both proofs of one detachment, plus whether this detach stowed the World's
/// carried state and any mid-turn collect root lease.
#[must_use = "a detached owner returns only through `SerialHost::reattach`"]
#[derive(Debug)]
pub(crate) struct DetachedOwner {
    host: DetachedHostBindings,
    tls: DetachedTls,
    world: DetachedWorldCarriers,
    parked_roots: Option<SerialParkedRoots>,
}

impl DetachedOwner {
    #[cfg(test)]
    pub(crate) fn world_carriers(&self) -> DetachedWorldCarriers {
        self.world
    }
}

/// Why an owner did not leave, or did not come back onto, this OS thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HostRefusal {
    /// Reattach: another Context's heap is installed on this thread, so its
    /// turn is live.
    HeapInstalled,
    /// Reattach: this Context belongs to a Green World whose carried TLS and
    /// warm `OwnerCarried` are still held by another owner rather than stowed
    /// in `GreenRuntime`; reattaching would run against placeholders.
    WorldHeldElsewhere,
    /// Detach: this Context's heap is not the one installed on this thread,
    /// so taking the TLS down would take down another Context's.
    NotInstalled,
    /// Detach: this Context holds a Green thread seat (Main or Worker); a
    /// serial owner must remain unseated so `Context::drop` does not run
    /// Green scheduler teardown.
    Seated,
}

static_assertions::assert_not_impl_any!(DetachedHostBindings: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(DetachedTls: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(DetachedOwner: Send, Sync, Clone, Copy);

/// Aborts the process when a panic unwinds out of a detach or reattach. The
/// owner would be left half-detached (bindings out with the TLS up, or the
/// TLS reset with the bindings still out), which nothing can repair. A Green
/// switch does the same (`green/switch.rs`).
struct AbortOnPanic(&'static str);

impl Drop for AbortOnPanic {
    fn drop(&mut self) {
        if std::thread::panicking() {
            tracing::error!(
                step = self.0,
                "a serial owner panicked between its detach steps; it would stay half-detached, so the process aborts"
            );
            std::process::abort();
        }
    }
}

impl Context {
    /// Take this owner off its OS thread at the end of a serial turn: refuse
    /// when seated in a Green runtime or unless its own heap is the installed
    /// one, else host bindings out, then the carried world (when this Context
    /// on this OS thread holds it), then the carrier TLS down.
    #[cold]
    #[inline(never)]
    fn detach_serial_owner(&mut self) -> Result<DetachedOwner, HostRefusal> {
        if self.owner.slab().and_then(|s| s.green_seat()).is_some() {
            return Err(HostRefusal::Seated);
        }
        if !neovm_value::tagged::gc::tagged_heap_is_current(&self.tagged_heap) {
            return Err(HostRefusal::NotInstalled);
        }
        let _abort = AbortOnPanic("detach");
        let parked = self.park_owner();
        let world = if self.stow_serial_world_state() {
            DetachedWorldCarriers::Stowed
        } else {
            DetachedWorldCarriers::NotStowed
        };
        // L6.7c (b): close any bump-allocation regions opened during this
        // serial turn before taking down carrier TLS, so the keeper's next
        // stop / collection never observes `ImageError::RegionsOpen`.
        self.tagged_heap.close_alloc_regions();
        // SAFETY: `self` has parked its owner bindings and stowed carried
        // World state under `AbortOnPanic`; no TLS heap loan is live, and
        // `reattach_serial_owner` calls `reset_carrier_tls()` before returning.
        unsafe { park_carrier_tls() };
        Ok(DetachedOwner {
            host: DetachedHostBindings { parked },
            tls: DetachedTls {
                _thread: PhantomData,
            },
            world,
            parked_roots: None,
        })
    }

    /// Take this owner off its OS thread for a mid-turn keeper collection
    /// (L6.7c (c)): mirrors `green_park_impl` / `park_world_seat_with_roots`
    /// ordering under one `AbortOnPanic` so every World object held only by
    /// this parked `SerialTurn`'s private execution state (`specpdl`,
    /// `vm_frames`, `lexenv`, `catch_stack`, TLS scratch, JIT pins) is rooted
    /// in the heap's `TransportRootTable` before the keeper's collecting stop.
    #[cold]
    #[inline(never)]
    pub(super) fn detach_serial_owner_for_collect(
        &mut self,
        epoch: neovm_value::tagged::gc::FacadeEpochRetention,
        _: HostTransition,
    ) -> Result<DetachedOwner, HostRefusal> {
        if self.owner.slab().and_then(|s| s.green_seat()).is_some() {
            return Err(HostRefusal::Seated);
        }
        if !neovm_value::tagged::gc::tagged_heap_is_current(&self.tagged_heap) {
            return Err(HostRefusal::NotInstalled);
        }
        let _abort = AbortOnPanic("serial-collect-detach");
        // 1. L6.7c (b): close open bump-allocation regions before the keeper's
        //    collecting stop.
        self.tagged_heap.close_alloc_regions();
        // 2. Capture JIT pins before host-binding exchange.
        #[cfg(feature = "jit")]
        let parked_jit = crate::emacs_core::jit::green_owner::ParkedJit::prepare(self)
            .expect("serial turn JIT root preparation must succeed before collect park");
        // 3. Swap in-place buffer bindings out into `specpdl`/slab BEFORE
        //    tracing private roots (Catch 2).
        let parked = self.park_owner();
        // 4. Collect private execution roots + thread-local roots + JIT roots.
        let mut roots = Vec::new();
        self.trace_private_roots(&mut roots);
        let heap_id = self.tagged_heap.tagged_heap_identity();
        let gc_epoch = self.tagged_heap.gc_collections();
        let scan = neovm_value::tagged::gc::CacheRootScan::Parked;
        super::super::for_each_thread_local_root_source(heap_id, gc_epoch, scan, |_, collect| {
            collect(&mut roots);
        });
        #[cfg(feature = "jit")]
        roots.extend_from_slice(parked_jit.roots());
        // 5. Reserve, finalize, and publish `RootBatchLease` into the heap's
        //    `TransportRootTable` (traced by `EraCore::collect_stopped_kind`
        //    via `collect_shared_root_gc_roots`).
        let prepared = neovm_value::tagged::transport::PreparedRootBatch::reserve_retained(
            &self.tagged_heap,
            epoch,
            roots.len(),
        )
        .expect("serial turn parked root batch reservation must succeed");
        // SAFETY: this serial turn holds exclusive World access on this OS
        // thread after `park_owner` completed. Every root in `roots` is live
        // on `self.tagged_heap`, and no GC, Lisp, or handoff occurs between
        // collection, finalization, and publication.
        let finalized = unsafe { prepared.finalize(&roots) }
            .expect("serial turn parked root batch finalization must succeed");
        let lease = finalized
            .publish_finalized_root_batch()
            .expect("serial turn parked root batch publication must succeed");
        drop(roots);
        // 6. Hand lease ownership to `GreenRuntime::parked` when a Green
        //    runtime exists (F4: prevents `RootBatchLease::drop` leak on
        //    abnormal unwind), then stow World carriers and park carrier TLS.
        let parked_lease = {
            let runtime = self.green_runtime_ref();
            if let Some(runtime) = runtime.as_ref() {
                let id = next_serial_park_id();
                let replaced = runtime.parked.borrow_mut().insert_serial(id, lease);
                debug_assert!(
                    replaced.is_none(),
                    "monotonic serial park id must be unique"
                );
                SerialParkedLease::Green(id)
            } else {
                SerialParkedLease::Inline(Some(lease))
            }
        };
        let world = if self.stow_serial_world_state() {
            DetachedWorldCarriers::Stowed
        } else {
            DetachedWorldCarriers::NotStowed
        };
        // SAFETY: owner bindings are parked and World state stowed under
        // `AbortOnPanic`; `reattach_serial_owner` resets carrier TLS before reuse.
        unsafe { park_carrier_tls() };
        Ok(DetachedOwner {
            host: DetachedHostBindings { parked },
            tls: DetachedTls {
                _thread: PhantomData,
            },
            world,
            parked_roots: Some(SerialParkedRoots {
                lease: parked_lease,
                #[cfg(feature = "jit")]
                jit: parked_jit,
            }),
        })
    }

    /// Q2 non-marking phase retirement for a serial-turn collect
    /// `RootBatchLease`: fast-path `try_finish()` without `drain_to_quiescent`,
    /// also draining any previously deferred leases in `GreenRuntime::parked`
    /// (pentry NIT-2), falling back to `GreenRuntime::parked.retained`
    /// (fail-closed over-rooting), and aborting fail-closed if deferral occurs
    /// on a non-Green Sim era with no `GreenRuntime` (Q3).
    #[cold]
    #[inline(never)]
    fn retire_serial_parked_root_lease(
        &self,
        lease: neovm_value::tagged::transport::RootBatchLease,
    ) {
        let phase_safe = !self.gc_driver_active
            && matches!(
                self.tagged_heap.mark_phase(),
                neovm_value::tagged::gc::MarkPhase::Idle
                    | neovm_value::tagged::gc::MarkPhase::Sweeping
            )
            && !self.tagged_heap.gc_locks_poisoned();
        let finish_retrying = |mut lease: neovm_value::tagged::transport::RootBatchLease| loop {
            match lease.try_finish() {
                Ok(_) => return Ok(()),
                Err(failure)
                    if matches!(
                        failure.error(),
                        neovm_value::tagged::transport::RootBatchError::Busy
                    ) =>
                {
                    (lease, _) = failure.into_parts();
                    std::thread::yield_now();
                }
                Err(failure) => return Err(failure.into_parts().0),
            }
        };
        let deferred = if phase_safe {
            finish_retrying(lease).err()
        } else {
            Some(lease)
        };
        if let Some(lease) = deferred {
            let runtime = self.green_runtime_ref();
            if let Some(runtime) = runtime.as_ref() {
                runtime.parked.borrow_mut().push_retained(lease);
            } else {
                tracing::error!(
                    "cannot defer serial-turn parked RootBatchLease retirement without a GreenRuntime; aborting fail-closed (L6.7c (c) Q3)"
                );
                std::process::abort();
            }
        } else if phase_safe {
            let runtime = self.green_runtime_ref();
            if let Some(runtime) = runtime.as_ref() {
                runtime
                    .parked
                    .borrow_mut()
                    .drain_retained_with(finish_retrying);
            }
        }
    }

    /// Put this owner back on the OS thread for the next serial turn: refuse
    /// while another Context's heap is installed here or while this Green
    /// World's carried state is still held by another owner, else clear the
    /// deferred caches, acquire the carried world when stowed (Green's order),
    /// then return the owner's TLS and host bindings.
    #[cold]
    #[inline(never)]
    fn reattach_serial_owner(
        &mut self,
        detached: DetachedOwner,
    ) -> Result<(), (DetachedOwner, HostRefusal)> {
        if !neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start() {
            return Err((detached, HostRefusal::HeapInstalled));
        }
        if self.serial_world_held_elsewhere() {
            return Err((detached, HostRefusal::WorldHeldElsewhere));
        }
        let _abort = AbortOnPanic("reattach");
        let DetachedOwner {
            host,
            tls: DetachedTls { .. },
            world,
            parked_roots,
        } = detached;
        match world {
            DetachedWorldCarriers::Stowed => {
                debug_assert!(
                    self.serial_world_is_stowed(),
                    "a detach that stowed the World's carriers must meet them stowed at reattach"
                );
            }
            DetachedWorldCarriers::NotStowed => {}
        }
        reset_carrier_tls();
        self.unstow_serial_world_state();
        if let Some(mut parked_roots) = parked_roots {
            // Restore JIT pins with explicit heap identity (`pin_redefined_function_for_heap`,
            // F1) while `lease` still roots them, before `resume_owner` swaps host bindings
            // back in (Catch 2).
            #[cfg(feature = "jit")]
            parked_roots
                .jit
                .resume(self)
                .expect("the original serial owner resumes in its own heap");
            self.resume_owner(host.parked);
            let lease = match &mut parked_roots.lease {
                SerialParkedLease::Green(id) => {
                    let id = *id;
                    self.green_runtime_ref()
                        .as_ref()
                        .expect("GreenRuntime present across serial collect reattach")
                        .parked
                        .borrow_mut()
                        .remove_serial(id)
                        .expect("serial parked RootBatchLease present on reattach")
                }
                SerialParkedLease::Inline(slot) => slot
                    .take()
                    .expect("inline serial parked RootBatchLease present on reattach"),
            };
            self.retire_serial_parked_root_lease(lease);
        } else {
            self.resume_owner(host.parked);
        }
        Ok(())
    }
}

/// Thread-local marker for the currently active `SerialTurn` on this OS thread
/// (L6.7c (c) F3 / Q2 + NIT-1): stores zero bytes on `#[repr(C)] Context`,
/// zero heap `Value`s, and zero `TaggedHeap` pointers, so `Context` layout
/// stays pinned and `park_carrier_tls()` never interacts with it. Includes
/// monotonic `context_id` (`Context::context_instance_id()`) so two `Context`s
/// sharing the same World `HeapIdentity` at a reused or swapped address cannot
/// falsely match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ActiveSerialTurn {
    ctx_addr: usize,
    heap: neovm_value::tagged::gc::HeapIdentity,
    context_id: u64,
}

impl ActiveSerialTurn {
    #[inline]
    pub(super) fn for_context(ctx: &Context) -> Self {
        Self {
            ctx_addr: std::ptr::from_ref(ctx) as usize,
            heap: ctx.tagged_heap.heap_identity(),
            context_id: ctx.context_instance_id(),
        }
    }

    /// Closed exact-identity match: true ONLY when address, `HeapIdentity`,
    /// and monotonic `context_instance_id` all match `ctx`.
    #[inline]
    pub(super) fn matches(self, ctx: &Context) -> bool {
        self == Self::for_context(ctx)
    }
}

std::thread_local! {
    static ACTIVE_SERIAL_TURN: std::cell::Cell<Option<ActiveSerialTurn>> =
        const { std::cell::Cell::new(None) };
}

impl Context {
    /// True while this `Context` is inside a live `SerialTurn` on this OS
    /// thread (`SerialHost::enter_turn` .. `leave_turn`). Deny-by-default:
    /// returns `false` when no turn is active or any field of
    /// [`ActiveSerialTurn`] differs. Ensures unbridged `SerialTurn::with_world`
    /// turns never fall through to direct `TaggedHeap` STW collection even
    /// under `NEOVM_HASH_TEST_PARITY=off` or direct `gc_collect_exact` calls
    /// (L6.7c (c) F3 / Q2 + NIT-1).
    #[inline]
    pub(crate) fn in_active_serial_turn(&self) -> bool {
        ACTIVE_SERIAL_TURN.with(|slot| slot.get().is_some_and(|active| active.matches(self)))
    }
}

/// The only non-test `SerialHost`: a [`HostTransition`] arrives by value, and
/// a host must never forward it to another host. The guard row
/// `l64_context_is_the_only_production_serial_host` keeps it so.
impl SerialHost for Context {
    type Detached = DetachedOwner;
    type Refusal = HostRefusal;

    fn detach(&mut self, _: HostTransition) -> Result<DetachedOwner, HostRefusal> {
        self.detach_serial_owner()
    }

    fn reattach(
        &mut self,
        detached: DetachedOwner,
        _: HostTransition,
    ) -> Result<(), (DetachedOwner, HostRefusal)> {
        self.reattach_serial_owner(detached)
    }

    /// L6.7c (a) + F3: inhibit safe-point (threshold) GC and unbridged direct
    /// STW GC for the whole `SerialTurn` so Lisp allocation debt defers to the
    /// keeper's next stop instead of firing an uncoordinated direct minor/STW
    /// collection inside the turn.
    fn enter_turn(&mut self, _: HostTransition) {
        let current = ActiveSerialTurn::for_context(self);
        ACTIVE_SERIAL_TURN.with(|slot| {
            let prev = slot.replace(Some(current));
            debug_assert!(
                prev.is_none(),
                "at most one active SerialTurn per OS thread"
            );
        });
        self.gc_inhibit_depth += 1;
    }

    /// L6.7c (a) + F3: paired 1:1 with `enter_turn` by `SerialTurn`'s RAII
    /// lifecycle (`end`, `park`, or unwind `Drop`), so depth cannot leak.
    fn leave_turn(&mut self, _: HostTransition) {
        let expected = ActiveSerialTurn::for_context(self);
        ACTIVE_SERIAL_TURN.with(|slot| {
            if slot.get() == Some(expected) {
                slot.set(None);
            }
        });
        debug_assert!(
            self.gc_inhibit_depth > 0 || std::thread::panicking(),
            "SerialTurn::leave_turn called with gc_inhibit_depth == 0"
        );
        self.gc_inhibit_depth = self.gc_inhibit_depth.saturating_sub(1);
    }
}

#[cfg(test)]
#[path = "tests/serial_detach_test.rs"]
mod tests;
