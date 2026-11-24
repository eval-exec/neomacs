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
//! 3. the carrier TLS goes down (`park_carrier_tls`): the heap slot, its
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

/// Both proofs of one detachment, plus whether this detach stowed the World's
/// carried state.
#[must_use = "a detached owner returns only through `SerialHost::reattach`"]
#[derive(Debug)]
pub(crate) struct DetachedOwner {
    host: DetachedHostBindings,
    tls: DetachedTls,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "carries the detach's carried-state disposition; tests assert it directly"
        )
    )]
    world: DetachedWorldCarriers,
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
        park_carrier_tls();
        Ok(DetachedOwner {
            host: DetachedHostBindings { parked },
            tls: DetachedTls {
                _thread: PhantomData,
            },
            world,
        })
    }

    /// Put this owner back on the OS thread for the next serial turn: refuse
    /// while another Context's heap is installed here, else clear the
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
        let _abort = AbortOnPanic("reattach");
        let DetachedOwner {
            host,
            tls: DetachedTls { .. },
            world: _,
        } = detached;
        reset_carrier_tls();
        self.unstow_serial_world_state();
        self.resume_owner(host.parked);
        Ok(())
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
}

#[cfg(test)]
#[path = "tests/serial_detach_test.rs"]
mod tests;
