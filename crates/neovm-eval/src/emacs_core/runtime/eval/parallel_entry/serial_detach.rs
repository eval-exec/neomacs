//! L6.4, the suspend half (SERIAL-TURN-CONTEXT r1 point 3, E5.1-TYPES §2
//! `end`): the typed proofs that a serial turn took its owner off this OS
//! thread, and the one way back onto it.
//!
//! Detachment runs in a fixed order:
//! 1. the owner's host bindings swap out (`park_owner`, GNU's
//!    `unbind_for_thread_switch`, src/thread.c:95-99). This writes buffer
//!    storage, so the heap must still be installed;
//! 2. the carrier TLS goes down (`park_carrier_tls`): the heap slot, its
//!    hot-path mirrors and every heap-pointer record are null, so a
//!    collection while the owner is detached cannot meet a stale pointer on
//!    this thread.
//!
//! Reattachment reverses it. It first refuses, in release builds too, when
//! this thread already has a heap installed: one ACTIVE Context per OS
//! thread, any number dormant. It then clears the deferred caches
//! (`reset_carrier_tls`), as a reused Green carrier does, because a
//! collection while the owner was detached may have freed what they named.
//! Last, it reinstalls the owner's TLS and swaps its host bindings back in
//! (`resume_owner`).
//!
//! Scope: [`DetachedTls`] covers the carrier's heap-pointer TLS only. The
//! World-carried pieces (`green::carried::CarriedState`: processes, sqlite,
//! undo, terminal, and the rest of that roster), the thread-manager roster
//! and the GC-census roster stay up. L6.4's `end_serial` takes those down.
//!
//! The proofs are `!Send`: each one is a fact about this OS thread. They are
//! produced together and consumed together, so a half-detached owner cannot
//! exist, and a panic between the two steps aborts the process. Only
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

/// Both proofs of one detachment.
#[must_use = "a detached owner returns only through `SerialHost::reattach`"]
#[derive(Debug)]
pub(crate) struct DetachedOwner {
    host: DetachedHostBindings,
    tls: DetachedTls,
}

/// Why a detached owner did not come back onto this OS thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReattachRefusal {
    /// Another Context's heap is installed on this thread: its turn is live.
    HeapInstalled,
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
    /// Take this owner off its OS thread at the end of a serial turn: host
    /// bindings out first, then the carrier TLS down.
    #[cold]
    #[inline(never)]
    fn detach_serial_owner(&mut self) -> DetachedOwner {
        let _abort = AbortOnPanic("detach");
        let parked = self.park_owner();
        park_carrier_tls();
        DetachedOwner {
            host: DetachedHostBindings { parked },
            tls: DetachedTls {
                _thread: PhantomData,
            },
        }
    }

    /// Put this owner back on the OS thread for the next serial turn: refuse
    /// while another Context's heap is installed here, else clear the
    /// deferred caches, then return the owner's TLS and host bindings.
    #[cold]
    #[inline(never)]
    fn reattach_serial_owner(
        &mut self,
        detached: DetachedOwner,
    ) -> Result<(), (DetachedOwner, ReattachRefusal)> {
        if !neovm_value::tagged::gc::carrier_heap_is_acceptable_for_job_start() {
            return Err((detached, ReattachRefusal::HeapInstalled));
        }
        let _abort = AbortOnPanic("reattach");
        let DetachedOwner {
            host,
            tls: DetachedTls { .. },
        } = detached;
        reset_carrier_tls();
        self.resume_owner(host.parked);
        Ok(())
    }
}

impl SerialHost for Context {
    type Detached = DetachedOwner;
    type Refusal = ReattachRefusal;

    fn detach(&mut self, _: HostTransition) -> DetachedOwner {
        self.detach_serial_owner()
    }

    fn reattach(
        &mut self,
        detached: DetachedOwner,
        _: HostTransition,
    ) -> Result<(), (DetachedOwner, ReattachRefusal)> {
        self.reattach_serial_owner(detached)
    }
}

#[cfg(test)]
#[path = "tests/serial_detach_test.rs"]
mod tests;
