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
//! Reattachment reverses it. It first clears the deferred caches
//! (`reset_carrier_tls`), as a reused Green carrier does, because a
//! collection while the owner was detached may have freed what they named.
//! It then reinstalls the owner's TLS and swaps its host bindings back in
//! (`resume_owner`).
//!
//! The proofs are `!Send`: each one is a fact about this OS thread. They are
//! produced together and consumed together, so a half-detached owner cannot
//! exist.

use super::super::Context;
use super::super::ParkedOwner;
use super::super::green::carrier_tls::{park_carrier_tls, reset_carrier_tls};
use std::marker::PhantomData;

/// The owner's in-place buffer bindings are out of shared host storage.
#[must_use = "a detached owner returns only through `Context::reattach_serial_owner`"]
#[derive(Debug)]
pub(crate) struct DetachedHostBindings {
    parked: ParkedOwner,
}

/// This OS thread's carrier TLS names no heap.
#[must_use = "a detached owner returns only through `Context::reattach_serial_owner`"]
#[derive(Debug)]
pub(crate) struct DetachedTls {
    _thread: PhantomData<*const ()>,
}

/// Both proofs of one detachment.
#[must_use = "a detached owner returns only through `Context::reattach_serial_owner`"]
#[derive(Debug)]
pub(crate) struct DetachedOwner {
    host: DetachedHostBindings,
    tls: DetachedTls,
}

static_assertions::assert_not_impl_any!(DetachedHostBindings: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(DetachedTls: Send, Sync, Clone, Copy);
static_assertions::assert_not_impl_any!(DetachedOwner: Send, Sync, Clone, Copy);

impl Context {
    /// Take this owner off its OS thread at the end of a serial turn: host
    /// bindings out first, then the carrier TLS down.
    #[cold]
    #[inline(never)]
    #[cfg_attr(not(test), allow(dead_code))] // L6.4's end_serial is the caller.
    pub(crate) fn detach_serial_owner(&mut self) -> DetachedOwner {
        let parked = self.park_owner();
        park_carrier_tls();
        DetachedOwner {
            host: DetachedHostBindings { parked },
            tls: DetachedTls {
                _thread: PhantomData,
            },
        }
    }

    /// Put this owner back on the OS thread for the next serial turn: the
    /// deferred caches clear, then the owner's TLS and host bindings return.
    #[cold]
    #[inline(never)]
    #[cfg_attr(not(test), allow(dead_code))] // L6.4's begin is the caller.
    pub(crate) fn reattach_serial_owner(&mut self, detached: DetachedOwner) {
        let DetachedOwner {
            host,
            tls: DetachedTls { .. },
        } = detached;
        reset_carrier_tls();
        self.resume_owner(host.parked);
    }
}

#[cfg(test)]
#[path = "tests/serial_detach_test.rs"]
mod tests;
