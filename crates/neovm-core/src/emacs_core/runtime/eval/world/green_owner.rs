//! Green owner construction (Phase 0 step 2, round 3d): further Contexts
//! over one World, one per Lisp thread, each on its own OS thread.
//!
//! # Safety argument
//!
//! The sole-owner policy (`sole_owner`) proves World views safe for ONE
//! Context: each manager cell has exactly one mutable view, owned by that
//! Context. A Green World admits further owners, each with its own full set
//! of views over the same cells, so that proof no longer holds by itself.
//! It is replaced by the [`Baton`]:
//!
//! 1. A Green World has exactly one Baton, issued once
//!    ([`Context::issue_green_baton`]). It is `Send` and neither `Sync` nor
//!    `Clone`, so at any time exactly one OS thread holds it.
//! 2. Only the holder runs Lisp or touches any view of the World: an owner
//!    builds its views only from the Baton it holds
//!    ([`Baton::owner_views`]), and it runs only between receiving the Baton
//!    and handing it on. Every other owner is parked, blocked in its
//!    receive, holding views it does not dereference.
//! 3. Handing the Baton on is a channel send, received by the next holder:
//!    the send/receive pair orders every write the previous holder made
//!    through its views before every access the next holder makes. That is
//!    the only cross-thread edge any World cell ever crosses.
//! 4. The World's `Cell`s and `RefCell`s, its heap and its obarray are thus
//!    accessed by one thread at a time, in a total order, which is what
//!    their `!Sync` contracts require. World and Context stay `!Send`; only
//!    the Baton carries the World's allocation across threads.
//! 5. The Baton also retains the heap's facade epoch, so no concurrent
//!    marker runs while Green owners exist: every collection is synchronous,
//!    on the holder.
//!
//! Debug builds check point 2 at every view creation: the World records its
//! holder's OS thread ([`Baton::enter`]), and [`Baton::owner_views`] panics
//! off that thread.

use super::*;

#[path = "parallel_roots.rs"]
mod parallel_roots;
#[cfg(test)]
pub(crate) use parallel_roots::OwnedProofProjection;
pub(crate) use parallel_roots::{FrozenLegacyRoots, OwnedParallelError, OwnedParallelProjection};

impl World {
    /// Finish publications before automatic field destruction reaches the heap.
    /// World exclusivity proves that every former owner has terminated.
    #[cold]
    #[inline(never)]
    pub(super) fn finish_green_registry(&mut self) {
        let Some(runtime) = self.green.get_mut().as_mut() else {
            return;
        };
        if !runtime.parked.get_mut().is_empty() {
            let heap = self.tagged_heap.get_mut();
            // SAFETY: the final World owns every view exclusively; no owner,
            // callback or collection extent remains. Its last publications
            // still exclude concurrent launch. With no Context left, the
            // transport table contains all remaining external heap roots.
            let collector = unsafe {
                heap.drain_to_quiescent(|heap| {
                    let mut roots = Vec::new();
                    crate::tagged::transport::collect_shared_root_gc_roots(heap, &mut roots);
                    for root in roots {
                        heap.seed_root(root);
                    }
                })
            }
            .expect("the terminated Green World's collector drains completely");
            runtime.finish_terminated_roots(&collector);
        }
        drop(self.green.get_mut().take());
        debug_assert!(
            !self.tagged_heap.get_mut().facade_mark_is_excluded(),
            "Green publications, reservations and Baton epochs finish before the heap"
        );
    }
}

/// The right to run Lisp in a Green World: whoever holds it is the one OS
/// thread that may touch the World. See the module's safety argument.
#[must_use = "the World's Lisp stops while its Baton is dropped"]
pub(crate) struct Baton {
    /// Keeps concurrent marking excluded for the Green era (point 5).
    /// Release before `world`: this Baton can own the final World reference.
    _retention: crate::tagged::gc::FacadeEpochRetention,
    world: Arc<World>,
}

// SAFETY: the Baton is the World's single, unshareable token (point 1); the
// World behind it is touched only by the thread holding it, and the channel
// that moves it orders those accesses (points 2-4).
unsafe impl Send for Baton {}
static_assertions::assert_not_impl_any!(Baton: Sync, Clone, Copy);

impl fmt::Debug for Baton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Baton")
            .field("holder", &self.world.baton_holder.get())
            .finish_non_exhaustive()
    }
}

impl Baton {
    /// Cold proof-3 backing admission. Original allocation roots only; no
    /// Context view, heap reference or ordinary mutable manager borrow.
    pub(crate) fn parallel_symbol_backing(
        &self,
    ) -> crate::emacs_core::symbol::parallel::SymbolBacking {
        assert!(self.held_here(), "symbol backing keeper must be held here");
        // SAFETY: era admission has escrowed the serial Context and retains
        // this Baton for every owner loan, including abandoned-era retention.
        unsafe {
            crate::emacs_core::symbol::parallel::SymbolBacking::new(
                std::ptr::NonNull::new(self.world.obarray.get()).expect("World obarray root"),
                std::ptr::NonNull::from(&self.world.thread_projections),
                Arc::as_ptr(&self.world) as usize,
            )
        }
    }
    /// The calling OS thread has received the Baton: it is the holder.
    pub(crate) fn enter(&self) {
        self.world
            .baton_holder
            .set(Some(std::thread::current().id()));
    }

    /// Whether the calling OS thread holds the Baton.
    pub(crate) fn held_here(&self) -> bool {
        self.world.baton_holder.get() == Some(std::thread::current().id())
    }

    /// Identify the retained heap without borrowing its storage. Admission
    /// separately checks `held_here`; metadata alone grants no World access.
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_heap_identity(&self) -> crate::tagged::gc::HeapIdentity {
        self._retention.heap_identity()
    }

    /// Cold metadata retention for parallel root publications, with no heap loan.
    #[cold]
    #[inline(never)]
    pub(crate) fn parallel_epoch_retention(
        &self,
    ) -> Result<crate::tagged::gc::FacadeEpochRetention, crate::tagged::gc::FacadeMarkExclusionError>
    {
        self._retention.retain()
    }

    /// A further owner's views of the World, starting in CURRENT. Only the
    /// holder may build them (point 2, checked in debug builds).
    pub(in crate::emacs_core::eval) fn owner_views(&self, current: Option<BufferId>) -> ReadyWorld {
        debug_assert!(
            self.held_here(),
            "a World view made off the thread holding the World's Baton"
        );
        let lease = WorldLease(Arc::clone(&self.world));
        WorldStorageBuilder::publish_views(&lease, BufferSelection::for_owner(current))
    }

    /// Reserve a parked root publication under this World's held exclusion.
    pub(in crate::emacs_core::eval) fn prepare_parked_batch(
        &self,
        heap: &crate::tagged::gc::TaggedHeap,
        capacity: usize,
    ) -> Result<crate::tagged::transport::PreparedRootBatch, crate::tagged::transport::RootBatchError>
    {
        debug_assert!(self.held_here(), "root publication prepared off the Baton");
        crate::tagged::transport::PreparedRootBatch::reserve_retained(
            heap,
            self._retention.retain()?,
            capacity,
        )
    }

    /// The World's Green runtime.
    pub(crate) fn green_runtime(
        &self,
    ) -> std::cell::Ref<'_, Option<Box<super::super::green::GreenRuntime>>> {
        debug_assert!(self.held_here(), "the Green runtime read off the Baton");
        self.world.green.borrow()
    }
}

impl Context {
    /// Build registration fixtures with a real World and its unique keeper.
    /// The higher-ranked closure cannot return an era or collector heap loan.
    #[cfg(test)]
    pub(crate) fn with_test_mutator_era<R>(
        f: impl for<'heap> FnOnce(crate::tagged::gc::ProofEra<'heap>) -> R,
    ) -> R {
        let mut context = Self::new();
        let keeper = context
            .issue_green_baton()
            .expect("fresh fixture World issues its keeper");
        let heap: *mut crate::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
        // SAFETY: this fresh Context is the sole running owner and holds the
        // keeper. No manager loan, callback, safepoint or collection extent
        // survives keeper issuance. Reseeding only enumerates the complete
        // registered-root census; it cannot execute Lisp. The closure below
        // cannot outlive the Context, and admission escrows the actual keeper.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                context.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .expect("fresh fixture collector drains completely");
        // Compare only the root address while the exclusive collector loan
        // is retained. Synthetic participants cannot use the serial TLS view.
        crate::tagged::gc::clear_tagged_heap_root_if_installed(context.tagged_heap.heap_root());
        let era = crate::tagged::gc::ProofEra::admit(collector, keeper)
            .unwrap_or_else(|_| panic!("fresh fixture World admits its mutator era"));
        f(era)
    }

    /// Real heap roots prepared by its sole serial owner before the synthetic
    /// parallel era. Participants receive SharedRoot handles, never raw Values.
    #[cfg(test)]
    pub(crate) fn with_test_rooted_mutator_era<R>(
        f: impl for<'heap> FnOnce(
            crate::tagged::gc::ProofEra<'heap>,
            Vec<crate::tagged::transport::SharedRoot>,
        ) -> R,
    ) -> R {
        let mut context = Self::new();
        let mut roots = Vec::new();
        for n in 0..4 {
            let value = context.tagged_heap.alloc_cons(
                crate::tagged::value::TaggedValue::fixnum(n),
                crate::tagged::value::TaggedValue::NIL,
            );
            // SAFETY: sole serial owner; allocation invokes no collection or
            // callback and the source word stays live through root admission.
            roots.push(unsafe {
                crate::tagged::transport::SharedRoot::new(&context.tagged_heap, value)
            });
        }
        let keeper = context.issue_green_baton().expect("rooted fixture keeper");
        let heap: *mut crate::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
        // SAFETY: same complete frozen-World census as with_test_mutator_era.
        // SharedRoot table entries keep every synthetic input live as well.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                context.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .expect("rooted fixture collector");
        // Compare only the root address while the exclusive collector loan
        // is retained. Synthetic participants cannot use the serial TLS view.
        crate::tagged::gc::clear_tagged_heap_root_if_installed(context.tagged_heap.heap_root());
        let era = crate::tagged::gc::ProofEra::admit(collector, keeper)
            .unwrap_or_else(|_| panic!("rooted fixture era"));
        f(era, roots)
    }

    /// Issue this Green World's one Baton to the calling thread, the
    /// owner's: the collector is drained and its facade epoch retained for
    /// the Green era, so no concurrent marker runs while Green owners exist.
    #[cold]
    #[inline(never)]
    pub(in crate::emacs_core::eval) fn issue_green_baton(
        &mut self,
    ) -> Result<Baton, super::super::green::GreenStartError> {
        let heap: *mut crate::tagged::gc::TaggedHeap = self.tagged_heap.heap_root().as_ptr();
        // SAFETY: the sole running owner, at a safe point, with no manager
        // loan or callback live; the reseed enumerates the registered root
        // census and only adds mark roots (as admission's collector-closed
        // phase does).
        let exclusion = unsafe {
            (*heap)
                .drain_to_quiescent(|heap| {
                    self.seed_registered_mutator_roots_world_stopped(heap);
                })?
                .exclude_concurrent_mark()?
        };
        let retention = exclusion.retain()?;
        drop(exclusion);
        let world = Arc::clone(&self.obarray.lease.0);
        world.baton_holder.set(Some(std::thread::current().id()));
        Ok(Baton {
            world,
            _retention: retention,
        })
    }

    /// This World's Green runtime, if it started.
    pub(in crate::emacs_core::eval) fn green_runtime_ref(
        &self,
    ) -> std::cell::Ref<'_, Option<Box<super::super::green::GreenRuntime>>> {
        self.obarray.lease.0.green.borrow()
    }

    /// Install the World's Green runtime (once).
    pub(in crate::emacs_core::eval) fn install_green_runtime(
        &self,
        runtime: Box<super::super::green::GreenRuntime>,
    ) {
        let mut slot = self.obarray.lease.0.green.borrow_mut();
        debug_assert!(slot.is_none(), "a World starts its Green runtime once");
        *slot = Some(runtime);
    }
}
