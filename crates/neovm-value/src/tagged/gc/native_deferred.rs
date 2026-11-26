//! Deferred native reclamation for a stopped collection (E3.2).
//!
//! A stopped job may not run Lisp, host destruction, or a finalizer while it
//! holds the stopped heap loan. GNU runs user-ptr and module-function
//! finalizers inside the sweep (`cleanup_vector`, alloc.c ~:3141/:3160), so
//! the finalizer must still run before control returns to Lisp from the
//! collection that swept the object. A stopped sweep therefore detaches the
//! owned object and records it.
//!
//! The primary drain is the stopped completion itself: after
//! `AllStopped::trace` returns (the heap loan and image restoration are over,
//! the keeper thread still holds AllStopped) and before owners resume. The
//! serial completed-cycle block (on the Baton/main thread, before doomed Lisp
//! finalizers and `post-gc-hook`) is the backstop for serial collections.
//! SQLite is a GNU no-op at collection (alloc.c ~:3228): the record
//! reproduces only the existing explicit drop, with no added close.

use super::TaggedHeap;
use crate::tagged::header::{EmacsFinalizer, ModuleFunctionObj, SqliteObj, UserPtrObj};

/// One detached native object waiting for its post-stop reclamation.
pub(super) enum DeferredNativeReclaim {
    UserPtr {
        object: *mut UserPtrObj,
        native: *mut std::ffi::c_void,
        finalizer: EmacsFinalizer,
    },
    ModuleFunction {
        object: *mut ModuleFunctionObj,
        data: *mut std::ffi::c_void,
        finalizer: EmacsFinalizer,
    },
    Sqlite {
        object: *mut SqliteObj,
    },
}

/// The batch detached by a stopped sweep. Dropping the batch without running
/// the drain (heap teardown) frees only the inert Rust boxes, exactly like
/// `ReclamationMode::DropFallback`: no callback runs during thread teardown,
/// and a feature-enabled SQLite payload is retained rather than closed.
#[derive(Default)]
pub(super) struct PendingNativeDestroys {
    records: Vec<DeferredNativeReclaim>,
}

impl PendingNativeDestroys {
    pub(super) fn push(&mut self, record: DeferredNativeReclaim) {
        self.records.push(record);
    }

    /// Run the deferred reclamation once. The stopped completion calls this
    /// after the heap loan and image restoration have ended (AllStopped is
    /// still held on the keeper thread); the serial completed-cycle block
    /// calls it as the backstop. The finalizer contract excludes heap/TLS/
    /// evaluator reentry, so no heap view is required.
    pub(super) fn drain(&mut self) -> usize {
        let records = std::mem::take(&mut self.records);
        let count = records.len();
        for record in records {
            match record {
                DeferredNativeReclaim::UserPtr {
                    object,
                    native,
                    finalizer,
                } => {
                    // SAFETY: the stopped sweep detached this initialized
                    // object from every list and transferred its finalizer
                    // here; the drain owns both. The EmacsFinalizer contract
                    // excludes heap/TLS/evaluator reentry.
                    unsafe {
                        if let Some(finalizer) = finalizer {
                            finalizer(native);
                        }
                        drop(Box::from_raw(object));
                    }
                }
                DeferredNativeReclaim::ModuleFunction {
                    object,
                    data,
                    finalizer,
                } => {
                    // SAFETY: same ownership transfer as UserPtr; the module
                    // registered this callback for this closure data.
                    unsafe {
                        if let Some(finalizer) = finalizer {
                            finalizer(data);
                        }
                        drop(Box::from_raw(object));
                    }
                }
                DeferredNativeReclaim::Sqlite { object } => {
                    // SAFETY: this detached box owns the SQLite identity. The
                    // existing explicit arm drops it; nothing more (GNU's
                    // cleanup_vector is a no-op for PVEC_SQLITE).
                    unsafe { drop(Box::from_raw(object)) };
                }
            }
        }
        count
    }
}

impl Drop for PendingNativeDestroys {
    fn drop(&mut self) {
        for record in std::mem::take(&mut self.records) {
            match record {
                DeferredNativeReclaim::UserPtr { object, .. } => {
                    // SAFETY: the box is detached and exclusively owned; its
                    // Rust payload has no destructor. Teardown deliberately
                    // does not invoke the registered native finalizer.
                    unsafe { drop(Box::from_raw(object)) };
                }
                DeferredNativeReclaim::ModuleFunction { object, .. } => {
                    // SAFETY: as for UserPtr.
                    unsafe { drop(Box::from_raw(object)) };
                }
                DeferredNativeReclaim::Sqlite { object } => {
                    if !cfg!(feature = "sqlite") {
                        // SAFETY: without the sqlite feature nothing installed
                        // a teardown hook; the box is inert.
                        unsafe { drop(Box::from_raw(object)) };
                    }
                    // With the feature the explicit destructor borrows runtime
                    // registries; retain the box during automatic teardown,
                    // matching `ReclamationMode::DropFallback`.
                }
            }
        }
    }
}

impl TaggedHeap {
    /// Enter the stopped-collection mode in which a detached native payload is
    /// recorded instead of reclaimed inline. Idempotent: a second stop before
    /// the drain appends to the same batch.
    pub(super) fn begin_native_deferral(&mut self) {
        self.pending_native_destroys
            .get_or_insert_with(PendingNativeDestroys::default);
    }

    pub(super) fn deferring_native_reclaims(&self) -> bool {
        self.pending_native_destroys.is_some()
    }

    /// # Safety
    /// `object` must be an exclusively owned, detached, initialized
    /// [`UserPtrObj`] box ready for deferred reclamation.
    pub(super) unsafe fn push_deferred_user_ptr(&mut self, object: *mut UserPtrObj) {
        let pending = self
            .pending_native_destroys
            .as_mut()
            .expect("deferral checked by the caller");
        // SAFETY: the stopped sweep detached this initialized box and the
        // caller checked the deferral mode; the field is exclusively owned.
        let (native, finalizer) = unsafe { ((*object).ptr, (*object).finalizer.take()) };
        pending.push(DeferredNativeReclaim::UserPtr {
            object,
            native,
            finalizer,
        });
    }

    /// # Safety
    /// `object` must be an exclusively owned, detached, initialized
    /// [`ModuleFunctionObj`] box ready for deferred reclamation.
    pub(super) unsafe fn push_deferred_module_function(&mut self, object: *mut ModuleFunctionObj) {
        let pending = self
            .pending_native_destroys
            .as_mut()
            .expect("deferral checked by the caller");
        // SAFETY: as for UserPtr; the module registered the callback for this
        // closure data and the drain owns both.
        let (data, finalizer) = unsafe { ((*object).data, (*object).finalizer.take()) };
        pending.push(DeferredNativeReclaim::ModuleFunction {
            object,
            data,
            finalizer,
        });
    }

    /// # Safety
    /// `object` must be an exclusively owned, detached, initialized
    /// [`SqliteObj`] box ready for deferred reclamation.
    pub(super) unsafe fn push_deferred_sqlite(&mut self, object: *mut SqliteObj) {
        let pending = self
            .pending_native_destroys
            .as_mut()
            .expect("deferral checked by the caller");
        pending.push(DeferredNativeReclaim::Sqlite { object });
    }

    /// Run the batch recorded by a stopped sweep. Returns the number of
    /// objects reclaimed. The stopped completion (collect_stopped_kind) calls
    /// this before returning to the owner side; the evaluator's
    /// completed-cycle block is the serial backstop.
    #[doc(hidden)]
    pub fn drain_pending_native_destroys(&mut self) -> usize {
        let Some(mut pending) = self.pending_native_destroys.take() else {
            return 0;
        };
        // Leaving the deferral mode after the drain restores ordinary
        // in-sweep reclamation for later serial collections.
        pending.drain()
    }

    /// Test-only: enter the stopped deferral mode without a real stop, and
    /// count the records a sweep has queued.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn begin_native_deferral_for_test(&mut self) {
        self.begin_native_deferral();
    }

    /// Test-only record count. Returns `None` when no deferral is active.
    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn pending_native_destroy_count_for_test(&self) -> Option<usize> {
        self.pending_native_destroys
            .as_ref()
            .map(|pending| pending.records.len())
    }
}
