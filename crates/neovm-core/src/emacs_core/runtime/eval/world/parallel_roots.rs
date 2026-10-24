//! Cold proof-1 roots. The legacy owner is captured before its heap loan;
//! frozen World storage is visited only after the rendezvous has stopped all
//! participants. Neither walk re-enumerates another owner's private slab.

use super::*;
use crate::gc_trace::GcTrace;
use crate::tagged::gc::{AllStopped, HeapIdentity, RendezvousError};
use crate::tagged::value::TaggedValue;

#[path = "parallel_admission.rs"]
mod parallel_admission;
pub(crate) use parallel_admission::{OwnedParallelError, OwnedParallelProjection};

/// An immutable census of the original Context, whose execution is suspended
/// for the entire era. Construction is sealed to that Context before the
/// QuiescentCollector heap loan exists. Values never cross an owner thread.
#[derive(Debug)]
pub(crate) struct FrozenLegacyRoots {
    heap: HeapIdentity,
    words: Vec<TaggedValue>,
    functions_quiescent: bool,
}

impl FrozenLegacyRoots {
    pub(crate) fn permits_function_writes(&self) -> bool {
        self.functions_quiescent
    }
    pub(crate) fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }

    /// Copies words only. The consumer must hold stopped collector authority
    /// for this exact heap before decoding them or tracing the resulting set.
    pub(crate) fn append_words(&self, out: &mut Vec<TaggedValue>) {
        out.extend_from_slice(&self.words);
    }
}

static_assertions::assert_not_impl_any!(FrozenLegacyRoots: Send, Sync, Clone, Copy);

impl Context {
    /// C4 admits inactive compiled caches, but no suspended legacy Lisp/native
    /// activation or outstanding job from this owner. Its closed executor can
    /// create neither while this original Context remains suspended.
    #[cold]
    fn function_cells_quiescent(&self) -> bool {
        use crate::emacs_core::eval::SpecBinding;
        if self.jit_root_stack_top != 0
            || self.specpdl.iter().any(|entry| {
                matches!(
                    entry,
                    SpecBinding::Backtrace { .. }
                        | SpecBinding::Backtrace1 { .. }
                        | SpecBinding::Backtrace2 { .. }
                        | SpecBinding::BacktraceNative { .. }
                )
            })
        {
            return false;
        }
        #[cfg(feature = "jit")]
        if crate::emacs_core::jit::cache::native_depth() != 0
            || crate::emacs_core::jit::bg::pending_count() != 0
        {
            return false;
        }
        true
    }

    #[cold]
    #[inline(never)]
    // No external capture factory: a same-heap snapshot could become stale
    // after arbitrary Context execution. The production transaction is
    // `admit_owned_parallel` in the child module. It calls this capture and
    // then `admit_rooted` with no Context execution between them. The fixture
    // sequences below remain the test-only NIL preclean.
    fn capture_parallel_legacy_roots(
        &self,
        keeper: &Baton,
    ) -> Result<FrozenLegacyRoots, RendezvousError> {
        if !keeper.held_here() {
            return Err(crate::tagged::gc::RegistrationError::WrongThread.into());
        }
        if !Arc::ptr_eq(&keeper.world, &self.obarray.lease.0)
            || keeper.parallel_heap_identity() != self.tagged_heap.heap_identity()
        {
            return Err(RendezvousError::Identity);
        }
        // A5 has no admission for pre-existing Green Context private slices.
        // Refuse even an ended runtime rather than guessing at parked roots.
        if keeper
            .world
            .green
            .try_borrow()
            .map_err(|_| RendezvousError::Busy)?
            .is_some()
        {
            return Err(RendezvousError::Publication);
        }
        let mut words = Vec::new();
        self.trace_private_roots(&mut words);
        if let Some(slab) = self.owner.slab() {
            // Only this original owner; never owner_slabs.trace_roots.
            slab.trace_roots(&mut |value| words.push(value));
        }
        let heap_id = self.tagged_heap.identity();
        let epoch = self.tagged_heap.gc_collections();
        super::super::super::for_each_thread_local_root_source(
            heap_id,
            epoch,
            crate::tagged::gc::CacheRootScan::Parked,
            |_, collect| collect(&mut words),
        );
        // These registries belong to the suspended Context, not to World
        // storage. Its private walk separately includes in-flight Flow roots.
        crate::emacs_core::ccl::collect_ccl_registry_gc_roots(&self.ccl_registry, &mut words);
        crate::emacs_core::charset::collect_charset_registry_gc_roots(
            &self.charset_registry,
            &mut words,
        );
        crate::emacs_core::terminal::pure::collect_terminal_registry_gc_roots(
            &self.terminal_registry,
            &mut words,
        );
        crate::emacs_core::xfaces::collect_font_registry_gc_roots(&self.font_registry, &mut words);
        crate::emacs_core::builtins::collect_hash_table_test_registry_gc_roots(
            &self.hash_table_test_registry,
            &mut words,
        );
        crate::emacs_core::builtins::collect_file_notify_registry_gc_roots(
            &self.file_notify_registry,
            &mut words,
        );
        crate::emacs_core::dynamic_module::collect_dynamic_module_registry_gc_roots(
            &self.dynamic_module_registry,
            &mut words,
        );
        Ok(FrozenLegacyRoots {
            heap: keeper.parallel_heap_identity(),
            words,
            functions_quiescent: self.function_cells_quiescent(),
        })
    }
}

impl Baton {
    /// Enumerate frozen shared storage once, after all ACKs, with no Context
    /// view, heap borrow, owner-slab walk, lazy initialization or Lisp callback.
    /// Marker unlink slots are valid only for the caller's stopped phase.
    #[cold]
    #[inline(never)]
    pub(crate) fn collect_parallel_world_roots(
        &self,
        stopped: &AllStopped<'_>,
        out: &mut Vec<TaggedValue>,
    ) -> Result<Vec<*mut *mut crate::tagged::header::MarkerObj>, RendezvousError> {
        if !self.held_here() {
            return Err(crate::tagged::gc::RegistrationError::WrongThread.into());
        }
        stopped.check_world_roots(self.parallel_heap_identity())?;
        if self
            .world
            .green
            .try_borrow()
            .map_err(|_| RendezvousError::Busy)?
            .is_some()
        {
            return Err(RendezvousError::Publication);
        }
        let projections = self
            .world
            .thread_projections
            .try_borrow()
            .map_err(|_| RendezvousError::Busy)?;
        // SAFETY: the unique keeper retains the World; AllStopped checked this
        // heap, keeper and every participant's publication. No participant can
        // access shared managers, no owner/reference loan survives, and only
        // this keeper enumerates these cells. Derive every reference directly
        // from its original UnsafeCell. The heap cell is deliberately untouched.
        let marker_heads = unsafe {
            let world = &self.world;
            let mut visit = |value| out.push(value);
            (&*world.owned_roots.get()).trace_roots(&mut visit);
            projections.trace_roots(&mut visit);
            for value in (&*world.treesit.get()).roots() {
                visit(value);
            }
            (&*world.obarray.get()).trace_roots_with(&mut visit);
            (&*world.watchers.get()).trace_roots_with(&mut visit);
            (&*world.autoloads.get()).trace_roots_with(&mut visit);
            (&*world.frames.get()).trace_roots_with(&mut visit);
            (&*world.coding_systems.get()).trace_roots_with(&mut visit);
            let buffers = &*world.buffers.get();
            buffers.trace_parallel_roots(&mut visit);
            let global = &*world.global.get();
            visit(global.cached_system_name);
            visit(global.selected_global_map.value());
            for text in global.current_message.iter().chain(
                global
                    .last_redisplay_signature
                    .iter()
                    .flat_map(|signature| signature.current_message.iter()),
            ) {
                text.trace_roots_with(&mut visit);
            }
            for call in &global.pending_safe_funcalls {
                visit(call.function);
                for value in call.args.iter().copied() {
                    visit(value);
                }
            }
            for value in global.gstring_shape_cache.values().copied() {
                visit(value);
            }
            global.processes.trace_roots_with(&mut visit);
            global.registers.trace_roots_with(&mut visit);
            global.interactive.trace_roots_with(&mut visit);
            global.xwidgets.trace_roots_with(&mut visit);
            global.face_table.trace_roots_with(&mut visit);
            global.threads.trace_roots_with(&mut visit);
            global.kmacro.trace_roots_with(&mut visit);
            global.command_loop.trace_roots_with(&mut visit);
            global.modes.trace_roots_with(&mut visit);
            buffers.parallel_marker_chain_head_slots()
        };
        Ok(marker_heads)
    }
}

/// The full World root surface with one explicit synthetic-fixture setting.
/// GNU's default weak window table is outside A5; no Lisp/window API executes
/// in the fixture. Reclaim it serially before caller setup, leaving the strict
/// production supported-type fence unchanged.
#[cfg(test)]
fn collection_fixture_context() -> Context {
    let mut context = Context::new();
    context.set_variable("window-dead-windows-table", TaggedValue::NIL);
    context.gc_collect_exact();
    context
}

impl Context {
    /// Prepare a real frozen World and legacy Context before admitting the
    /// synthetic participants. Setup runs serially; body cannot return an era.
    #[cfg(test)]
    pub(crate) fn with_test_collection_era<S, R>(
        setup: impl FnOnce(&mut Context) -> S,
        body: impl for<'heap> FnOnce(crate::tagged::gc::ParallelHeapEra<'heap>, S) -> R,
    ) -> R {
        let mut context = collection_fixture_context();
        let input = setup(&mut context);
        let keeper = context
            .issue_green_baton()
            .expect("collection fixture keeper");
        let frozen = context
            .capture_parallel_legacy_roots(&keeper)
            .expect("collection fixture frozen legacy roots");
        let heap: *mut crate::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
        // SAFETY: the original Context is serial and suspended, has no live
        // manager loan/callback, and captured its private roots before this
        // exclusive heap loan. The retained keeper excludes concurrent mark.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                context.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .expect("collection fixture drain");
        // Compare only the root address while the exclusive collector loan
        // is retained. Synthetic participants cannot use the serial TLS view.
        crate::tagged::gc::clear_tagged_heap_root_if_installed(context.tagged_heap.heap_root());
        let era = crate::tagged::gc::ParallelHeapEra::admit_rooted(collector, keeper, frozen)
            .unwrap_or_else(|failure| panic!("collection fixture admission: {:?}", failure.reason));
        body(era, input)
    }

    /// Post-era Context access requires the body to return BOTH successful
    /// finish capabilities. A poisoned/abandoned era cannot produce this pair;
    /// there is no unconditional serial access after an arbitrary body return.
    #[cfg(test)]
    pub(crate) fn with_test_collection_era_then<S, R>(
        setup: impl FnOnce(&mut Context) -> S,
        body: impl for<'heap> FnOnce(
            crate::tagged::gc::ParallelHeapEra<'heap>,
            S,
        )
            -> (crate::tagged::gc::QuiescentCollector<'heap>, Baton, R),
        after: impl FnOnce(&mut Context, R),
    ) {
        let mut context = collection_fixture_context();
        let input = setup(&mut context);
        let keeper = context
            .issue_green_baton()
            .expect("collection fixture keeper");
        let frozen = context
            .capture_parallel_legacy_roots(&keeper)
            .expect("collection fixture frozen legacy roots");
        let expected_heap = keeper.parallel_heap_identity();
        let expected_world = Arc::clone(&keeper.world);
        let heap: *mut crate::tagged::gc::TaggedHeap = context.tagged_heap.heap_root().as_ptr();
        // SAFETY: same sole-owner/capture-before-loan proof as the fixture above.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                context.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .expect("collection fixture drain");
        // Compare only the root address while the exclusive collector loan
        // is retained. Synthetic participants cannot use the serial TLS view.
        crate::tagged::gc::clear_tagged_heap_root_if_installed(context.tagged_heap.heap_root());
        let era = crate::tagged::gc::ParallelHeapEra::admit_rooted(collector, keeper, frozen)
            .unwrap_or_else(|failure| panic!("collection fixture admission: {:?}", failure.reason));
        let (collector, keeper, output) = body(era, input);
        assert_eq!(collector.heap_identity(), expected_heap);
        assert_eq!(keeper.parallel_heap_identity(), expected_heap);
        assert!(Arc::ptr_eq(&keeper.world, &expected_world));
        assert!(
            keeper.held_here(),
            "finished fixture keeper must remain on its owner"
        );
        drop(collector);
        drop(keeper);
        // Both finish capabilities returned; their loans ended above.
        context.tagged_heap.install_tagged_heap();
        after(&mut context, output);
    }
}

#[cfg(test)]
#[path = "tests/parallel_roots_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/symbol_fixture.rs"]
mod symbol_fixture;

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
#[path = "tests/function_boundary.rs"]
mod function_boundary;

#[cfg(all(test, not(feature = "gc-memory-telemetry")))]
#[path = "tests/intern_boundary.rs"]
mod intern_boundary;
