//! Cold, source-thread-only retention of the persistent backend. This does
//! not admit compiled code to an N-mutator era or move a backend to a peer.

use super::super::shims::{
    CURRENT_LEAF_BASES, PENDING_FLOW, PENDING_ROOT_SWEEP_FLOOR, PENDING_SHIM_PANIC,
};
use super::{SHARED_JIT, SharedJit};
use crate::emacs_core::jit::cache::source_detachment::DetachedSourceCache;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceBackendSlot {
    Backend,
    PendingFlow,
    PendingPanic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceBackendError {
    HeapInstalled,
    NativeActivation,
    NativeFramePublished,
    RootSweepPending,
    PendingFlow,
    PendingPanic,
    FrontendBusy,
    Borrowed(SourceBackendSlot),
}

/// Actual backend allocation, confined to its original source thread. Keeping
/// it independently non-destructing also retains code backing during unwind.
#[must_use]
pub(crate) struct DetachedSourceBackend {
    backend: ManuallyDrop<Option<Box<SharedJit>>>,
    thread: std::thread::ThreadId,
    _local: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for DetachedSourceBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DetachedSourceBackend")
            .field("thread", &self.thread)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_not_impl_any!(DetachedSourceBackend: Send, Sync, Clone, Copy);

impl Drop for DetachedSourceBackend {
    fn drop(&mut self) {
        // Retain the module and arena allocation. Taking the ManuallyDrop
        // would free code a suspended frame may still name.
        let _retained = (&self.backend, self.thread, &self._local);
    }
}

impl DetachedSourceBackend {
    /// # Safety
    ///
    /// The actual sole source is inaccessible, with all Lisp, native, VM,
    /// compile, and heap or store callback frames returned. The cache argument
    /// is its actual withdrawn cache, retained with its Context and final root
    /// image. No execution or GC may follow until the complete return or
    /// publication protocol. Counter and pointer observations cannot establish
    /// frame absence. This is not complete JIT or TLS suspension: background
    /// and frontend scope state have their own refusal protocol, still required
    /// before transport.
    #[cold]
    #[inline(never)]
    pub(crate) unsafe fn withdraw(
        _cache: &DetachedSourceCache,
    ) -> Result<Self, SourceBackendError> {
        if neovm_value::tagged::gc::tagged_heap_is_installed() {
            return Err(SourceBackendError::HeapInstalled);
        }
        if crate::emacs_core::jit::cache::native_depth() != 0 {
            return Err(SourceBackendError::NativeActivation);
        }
        if CURRENT_LEAF_BASES.with(|slot| slot.get().is_inside()) {
            return Err(SourceBackendError::NativeFramePublished);
        }
        if PENDING_ROOT_SWEEP_FLOOR.with(|slot| slot.get().is_some()) {
            return Err(SourceBackendError::RootSweepPending);
        }
        PENDING_FLOW.with(|flow| {
            PENDING_SHIM_PANIC.with(|panic| {
                SHARED_JIT.with(|backend| {
                    // Obtain every real loan before inspecting or taking any
                    // payload. Refusal neither consumes control flow nor heals
                    // native residue; the source stays fail-closed.
                    let flow = flow.try_borrow_mut().map_err(|_| {
                        SourceBackendError::Borrowed(SourceBackendSlot::PendingFlow)
                    })?;
                    let panic = panic.try_borrow_mut().map_err(|_| {
                        SourceBackendError::Borrowed(SourceBackendSlot::PendingPanic)
                    })?;
                    let mut backend = backend
                        .try_borrow_mut()
                        .map_err(|_| SourceBackendError::Borrowed(SourceBackendSlot::Backend))?;
                    if flow.is_some() {
                        return Err(SourceBackendError::PendingFlow);
                    }
                    if panic.is_some() {
                        return Err(SourceBackendError::PendingPanic);
                    }
                    if backend.as_ref().is_some_and(|jit| jit.fbctx.is_none()) {
                        return Err(SourceBackendError::FrontendBusy);
                    }
                    Ok(Self {
                        backend: ManuallyDrop::new(backend.take()),
                        thread: std::thread::current().id(),
                        _local: PhantomData,
                    })
                })
            })
        })
    }
}

#[cfg(test)]
#[path = "tests/source_detachment_test.rs"]
mod tests;
