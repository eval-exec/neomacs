//! The Green runtime (Phase 0 step 2, round 3d): GNU's Lisp threads as one
//! OS thread per Lisp thread, over one World, one running at a time.
//!
//! A Green World ([`ThreadingMode::Green`](super::ThreadingMode)) starts its
//! runtime at the first `make-thread`. That admits threads (round 3c), then
//! captures what a further owner is built from ([`GreenBootstrap`]) and
//! issues the World's [`Baton`](super::world::Baton). Each Lisp thread then
//! runs on its own OS thread, with its own [`Context`] over the same World
//! views: its own specpdl, handlers and current buffer, and its own owner
//! slab of dynamic bindings. Only the thread holding the Baton runs Lisp.
//!
//! What a further owner shares, copies or starts fresh is the
//! thread-local audit in design-r3.md §11.

mod api;
mod bootstrap;
mod carried;
mod carrier;
pub(crate) mod carrier_tls;
#[allow(
    dead_code,
    reason = "S2 control service stays dormant until owned host adapters activate"
)]
mod host_service;
mod owner_carried;
mod root_registry;
mod scheduler;
mod switch;

pub(crate) use api::{green_make_thread, green_world};
pub(crate) use bootstrap::GreenBootstrap;
#[cfg(debug_assertions)]
pub(crate) use bootstrap::debug_assert_native_tables_open;
pub(crate) use scheduler::{ThreadKey, Wait};
pub(crate) use switch::{GreenSeat, SelectTurn};

use super::admission::AdmissionError;
use super::{Context, ThreadingMode};
use crate::tagged::gc::{CollectorQuiescenceError, FacadeMarkExclusionError};

/// The World's Green runtime, once started: what further owners are built
/// from, the scheduler, and the state every Lisp thread shares. Held by the
/// World and traced with it.
pub(crate) struct GreenRuntime {
    pub(super) bootstrap: GreenBootstrap,
    pub(super) control: std::sync::Arc<scheduler::Control>,
    /// Each parked thread's private roots (plan-3d §4).
    pub(super) parked: std::cell::RefCell<host_service::WorldRootRegistry>,
    /// A started thread's function until its OS thread runs it.
    pub(super) pending_starts:
        std::cell::RefCell<rustc_hash::FxHashMap<ThreadKey, crate::emacs_core::value::Value>>,
    /// The World's thread-local state while its holder is parked, and
    /// placeholders for it otherwise (`carried`, plan-3w.md section 7).
    pub(super) carried: std::cell::RefCell<carried::CarriedState>,
    /// The warm GNU-global state while its holder is parked
    /// (`owner_carried`, plan-3w.md section 10).
    pub(super) owner_carried: std::cell::RefCell<Option<owner_carried::OwnerCarried>>,
    /// A `kill-emacs` run by another thread, for main to exit with.
    pub(super) exit: std::cell::Cell<Option<super::ShutdownRequest>>,
    /// GNU reports an outgoing owner's failed constant exchange in the next
    /// owner, which has already become current. Scalar identities need no GC
    /// roots. A same-owner reacquire consumes the marker without signalling.
    pub(super) outgoing_constant_error:
        std::cell::Cell<Option<(ThreadKey, crate::emacs_core::intern::SymId)>>,
    /// The bounded carrier OS-thread pool (S-track). Carriers share its
    /// `Arc` while idle; it owns every carrier's join handle. Visible to the
    /// Green module and its children only, matching `CarrierPool`.
    carriers: carrier::CarrierPool,

    /// The standard case table, one for every thread as in GNU:
    /// `set-standard-case-table` sets it here, and each thread installs it
    /// as it resumes.
    pub(super) standard_case_table: std::cell::Cell<Option<crate::emacs_core::value::Value>>,
    /// An error signalled into a thread before it first ran, as
    /// `(SYMBOL, DATA)`: held from its start to its first switch, as GNU
    /// `post_acquire_global_lock` raises nothing while a thread has no
    /// handlers.
    pub(super) deferred_errors: std::cell::RefCell<
        rustc_hash::FxHashMap<
            ThreadKey,
            (
                crate::emacs_core::value::Value,
                crate::emacs_core::value::Value,
            ),
        >,
    >,
}

impl std::fmt::Debug for GreenRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GreenRuntime").finish_non_exhaustive()
    }
}

impl GreenRuntime {
    /// The process exit KEY takes on resuming, if any: main exits with a
    /// `kill-emacs` another thread ran, and every other thread ends once
    /// main has ended.
    pub(super) fn exit_for(
        &self,
        key: ThreadKey,
        control: &scheduler::Control,
    ) -> Option<super::ShutdownRequest> {
        match key {
            ThreadKey::Main => self.exit.take(),
            ThreadKey::Worker(_) => {
                (control.lifecycle() == scheduler::Lifecycle::Closing).then(|| {
                    self.exit.get().unwrap_or(super::ShutdownRequest {
                        exit_code: 0,
                        restart: false,
                    })
                })
            }
        }
    }

    /// Visit every value the runtime holds. Out of line: the World's trace
    /// reaches it only in a Green World.
    #[cold]
    #[inline(never)]
    pub(super) fn trace_roots(&self, visit: &mut dyn FnMut(crate::emacs_core::value::Value)) {
        self.bootstrap.trace_roots(visit);
        // Parked RootBatches are traced by the heap's transport root table,
        // including compiled operand windows and the pinned source objects.
        for value in self.pending_starts.borrow().values() {
            visit(*value);
        }
        if let Some(table) = self.standard_case_table.get() {
            visit(table);
        }
        if let Some(state) = self.owner_carried.borrow().as_ref() {
            state.trace_roots(visit);
        }
        for (symbol, data) in self.deferred_errors.borrow().values() {
            visit(*symbol);
            visit(*data);
        }
    }
}

/// Cold scheduling selection, not a Lisp-visible error. A refused World uses
/// the existing ordinary thread implementation and never issues a Baton.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GreenAdmissionRefusal {
    NativeReadinessUnavailable {
        cause: crate::host_effect::wake::NativeReadinessCause,
    },
}

/// Thread startup additionally requires native readiness. The lower keeper
/// constructor's errors remain unchanged for serial collection/parallel owners.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GreenThreadStartError {
    #[error("green native readiness is unavailable: {0:?}")]
    Admission(GreenAdmissionRefusal),
    #[error(transparent)]
    Runtime(#[from] GreenStartError),
}

/// Why the Green runtime could not start, or a Green operation refused.
#[derive(Debug, thiserror::Error)]
pub(crate) enum GreenStartError {
    #[error("the World does not run green Lisp threads")]
    NotGreen,
    #[error("green Lisp threads are unavailable while a display or input host is attached")]
    ThreadAffineHost,
    #[error("green Lisp threads cannot start while garbage collection is inhibited")]
    GcInhibited,
    #[error(transparent)]
    Admission(#[from] AdmissionError),
    #[error(transparent)]
    Collector(#[from] CollectorQuiescenceError),
    #[error(transparent)]
    MarkExclusion(#[from] FacadeMarkExclusionError),
}

impl Context {
    #[cold]
    pub(crate) fn green_admission_readiness(&self) -> Result<(), GreenAdmissionRefusal> {
        self.processes.green_native_readiness().map_err(|cause| {
            self.processes.observe_green_admission_refusal(|cause| {
                let refusal = GreenAdmissionRefusal::NativeReadinessUnavailable { cause };
                tracing::warn!(target: "neovm::green::admission", ?refusal,
                    "Green admission refused; retaining ordinary Lisp threads");
            });
            GreenAdmissionRefusal::NativeReadinessUnavailable { cause }
        })
    }

    /// Start this World's Green runtime from its first owner: admit
    /// threads, capture the bootstrap further owners are built from, and
    /// issue the World's Baton to this thread. Refused unless the World is
    /// Green, no display or input host is attached and
    /// collection is not inhibited.
    #[cold]
    #[inline(never)]
    pub(crate) fn start_green_runtime(&mut self) -> Result<super::world::Baton, GreenStartError> {
        if self.threading_mode() != ThreadingMode::Green {
            return Err(GreenStartError::NotGreen);
        }
        if self.display_host.is_some() || self.input_rx.as_ref().is_some() {
            return Err(GreenStartError::ThreadAffineHost);
        }
        if self.gc_inhibit_depth != 0 {
            return Err(GreenStartError::GcInhibited);
        }
        if !self.threads_admitted() {
            self.admit_threads()?;
        }
        let bootstrap = GreenBootstrap::capture(self);
        let baton = self.issue_green_baton()?;
        self.install_green_runtime(Box::new(GreenRuntime {
            bootstrap,
            control: Default::default(),
            parked: std::cell::RefCell::new(host_service::WorldRootRegistry::new(
                self.tagged_heap.heap_identity(),
            )),
            pending_starts: Default::default(),
            exit: Default::default(),
            carried: std::cell::RefCell::new(carried::CarriedState::placeholders()),
            owner_carried: Default::default(),
            standard_case_table: std::cell::Cell::new(self.cached_standard_case_table),
            outgoing_constant_error: std::cell::Cell::new(None),
            deferred_errors: Default::default(),
            carriers: carrier::CarrierPool::new(),
        }));
        Ok(baton)
    }

    /// A further owner of the World BATON belongs to, on this OS thread,
    /// which holds the Baton: its own execution state over the World's
    /// views, starting in the creator's current buffer, with the World's
    /// canonical registries and the startup subr tables.
    #[cold]
    #[inline(never)]
    pub(crate) fn new_green_owner(baton: &super::world::Baton) -> Context {
        // The job prologue restored the World's carried state before this
        // call (the carrier reset and `unstow_carried_state` run in
        // `run_carrier_job`). This owner is about to run on this OS thread;
        // reserve the dense SymId cache once instead of regrowing it from
        // empty.
        crate::emacs_core::intern::prepare_thread_symbol_caches();
        let (seed, current, case_table) = {
            let runtime = baton.green_runtime();
            let bootstrap = &runtime
                .as_ref()
                .expect("a started Green runtime issued this Baton")
                .bootstrap;
            bootstrap.install_native_tables();
            (
                bootstrap.seed(),
                bootstrap.current_buffer,
                runtime
                    .as_ref()
                    .and_then(|runtime| runtime.standard_case_table.get()),
            )
        };
        let views = baton.owner_views(current);
        views.tagged_heap.install_tagged_heap();
        let mut owner = Context::assemble(views, seed);
        owner.cached_standard_case_table = case_table;
        owner.gc_count = owner.tagged_heap.gc_collections() as u64;
        owner.ensure_owner_slab();
        owner.setup_thread_locals();
        owner.republish_owner_mirrors();
        owner
    }

    /// Retire a further owner on its own OS thread, holding the Baton: its
    /// thread-local installations go; the World's shared registries stay
    /// (a Context's drop clears its dynamic-module registry, which a
    /// further owner shares with the rest of the World).
    #[cold]
    #[inline(never)]
    pub(crate) fn retire_green_owner(mut self) {
        self.dynamic_module_registry =
            super::super::dynamic_module::DynamicModuleRegistryHandle::new(Vec::new());
        drop(self);
    }
}

#[cfg(test)]
#[path = "../tests/green_owner_test.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/green_switch_test.rs"]
mod switch_tests;

#[cfg(test)]
#[path = "../tests/carrier_pool_test.rs"]
mod carrier_pool_tests;

#[cfg(test)]
#[path = "../tests/green_parked_root_equivalence_test.rs"]
mod parked_root_equivalence_tests;

#[cfg(test)]
#[path = "../tests/green_root_lifecycle_test.rs"]
mod root_lifecycle_tests;

#[cfg(all(test, feature = "jit"))]
#[path = "../tests/green_jit_switch_test.rs"]
mod jit_switch_tests;

#[cfg(all(test, feature = "jit"))]
#[path = "../tests/green_jit_inline2_test.rs"]
mod jit_inline2_tests;

#[cfg(test)]
#[path = "../tests/green_signal_depth_test.rs"]
mod signal_depth_tests;
