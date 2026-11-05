//! Era fixtures re-homed from eval's cfg(test) world modules.
//!
//! Peer rulings 06:0xZ and 07:3xZ (Variant B / B4): the collection, mutator
//! and rooted constructors move into core's test tree as an extension trait
//! whose associated functions keep their names, so the re-homed call sites
//! stay `Context::with_test_collection_era(..)` and friends plus one `use`.
//! The symbol-fixture body stays in eval (B1); its twelve entry points are
//! delegated through `neovm_eval::test_support::authority`, the only
//! cross-crate route.
//!
//! Bodies stay identical except for the three signed changes: the boot is
//! `crate::test_utils::new_context()` (the REAL CORE_EVAL_HOOK_SET), the
//! proof era is admitted through the value TA2 fence, and the World identity
//! comparison uses the eval-fence observer (never an `Arc` clone). Fields
//! are read through the R-b accessors, never the Context fields.

use crate::test_utils::new_context;
use neovm_eval::emacs_core::eval::Baton;
use neovm_eval::emacs_core::eval::Context;
use neovm_eval::test_support::authority::{
    WorldIdentity, WorldProofEra, baton_world_identity, same_world,
};
use neovm_value::intern::SymId;
use neovm_value::symbol::Obarray;
use neovm_value::symbol::parallel::SymbolBacking;
use neovm_value::tagged::gc::{
    FacadeEpochRetention, PlainSymbolBacking, QuiescentCollector, RegistrationError, TaggedHeap,
};
use neovm_value::tagged::value::TaggedValue;

/// Test entry points of the era fixtures, keeping the pre-move call shape.
pub trait EraFixtureExt {
    /// Prepare a real frozen World and legacy Context before admitting the
    /// synthetic participants. Setup runs serially; body cannot return an era.
    fn with_test_collection_era<S, R>(
        setup: impl FnOnce(&mut Context) -> S,
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, S) -> R,
    ) -> R;

    /// Post-era Context access requires the body to return BOTH successful
    /// finish capabilities. A poisoned/abandoned era cannot produce this
    /// pair; there is no unconditional serial access after an arbitrary body.
    fn with_test_collection_era_then<S, R>(
        setup: impl FnOnce(&mut Context) -> S,
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, S) -> (QuiescentCollector<'heap>, Baton, R),
        after: impl FnOnce(&mut Context, R),
    );

    /// Build registration fixtures with a real World and its unique keeper.
    fn with_test_mutator_era<R>(f: impl for<'heap> FnOnce(WorldProofEra<'heap>) -> R) -> R;

    /// Real heap roots prepared by the sole serial owner before the synthetic
    /// parallel era. Participants receive SharedRoot handles, never raw Values.
    fn with_test_rooted_mutator_era<R>(
        f: impl for<'heap> FnOnce(
            WorldProofEra<'heap>,
            Vec<neovm_value::tagged::transport::SharedRoot>,
        ) -> R,
    ) -> R;
}

/// GNU's default weak window table is outside A5; no Lisp/window API executes
/// in the fixture. Reclaim it serially before caller setup, leaving the strict
/// production supported-type fence unchanged.
fn collection_fixture_context() -> Context {
    let mut context = new_context();
    context.set_variable("window-dead-windows-table", TaggedValue::NIL);
    context.gc_collect_exact();
    context
}

/// Retain the original epoch and derive the original backing at the lower
/// admission's existing preparation boundary, exactly as eval's production
/// helper does. The keeper is always this World's own `Baton`.
fn prepare_keeper_backing(
    keeper: &Baton,
) -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError> {
    let epoch = keeper
        .parallel_epoch_retention()
        .map_err(|_| RegistrationError::Exhausted)?;
    Ok((epoch, keeper.parallel_symbol_backing()))
}

impl EraFixtureExt for Context {
    fn with_test_collection_era<S, R>(
        setup: impl FnOnce(&mut Context) -> S,
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, S) -> R,
    ) -> R {
        let mut context = collection_fixture_context();
        let input = setup(&mut context);
        let keeper = context
            .issue_green_baton()
            .expect("collection fixture keeper");
        let frozen = context
            .capture_parallel_legacy_roots(&keeper)
            .expect("collection fixture frozen legacy roots");
        let heap: *mut TaggedHeap = context.tagged_heap().heap_root().as_ptr();
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
        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(
            context.tagged_heap().heap_root(),
        );
        // SAFETY: the fixture built this World and holds its sole retention;
        // the prepare callback derives the original retained epoch/backing.
        let era = unsafe {
            neovm_value::test_support::authority::admit_rooted_for_test(
                collector,
                keeper,
                frozen,
                prepare_keeper_backing,
            )
        }
        .unwrap_or_else(|failure| panic!("collection fixture admission: {:?}", failure.reason()));
        body(era, input)
    }

    fn with_test_collection_era_then<S, R>(
        setup: impl FnOnce(&mut Context) -> S,
        body: impl for<'heap> FnOnce(WorldProofEra<'heap>, S) -> (QuiescentCollector<'heap>, Baton, R),
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
        let expected_world: WorldIdentity = baton_world_identity(&keeper);
        let heap: *mut TaggedHeap = context.tagged_heap().heap_root().as_ptr();
        // SAFETY: same sole-owner/capture-before-loan proof as the fixture above.
        let collector = unsafe {
            (*heap).drain_to_quiescent(|heap| {
                context.seed_registered_mutator_roots_world_stopped(heap);
            })
        }
        .expect("collection fixture drain");
        // Compare only the root address while the exclusive collector loan
        // is retained. Synthetic participants cannot use the serial TLS view.
        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(
            context.tagged_heap().heap_root(),
        );
        // SAFETY: same original keeper and frozen private census as above.
        let era = unsafe {
            neovm_value::test_support::authority::admit_rooted_for_test(
                collector,
                keeper,
                frozen,
                prepare_keeper_backing,
            )
        }
        .unwrap_or_else(|failure| panic!("collection fixture admission: {:?}", failure.reason()));
        let (collector, keeper, output) = body(era, input);
        assert_eq!(collector.heap_identity(), expected_heap);
        assert_eq!(keeper.parallel_heap_identity(), expected_heap);
        assert!(same_world(baton_world_identity(&keeper), expected_world));
        assert!(
            keeper.held_here(),
            "finished fixture keeper must remain on its owner"
        );
        drop(collector);
        drop(keeper);
        // Both finish capabilities returned; their loans ended above.
        context.tagged_heap().install_tagged_heap();
        after(&mut context, output);
    }

    fn with_test_mutator_era<R>(f: impl for<'heap> FnOnce(WorldProofEra<'heap>) -> R) -> R {
        let mut context = new_context();
        let keeper = context
            .issue_green_baton()
            .expect("fresh fixture World issues its keeper");
        let heap: *mut TaggedHeap = context.tagged_heap().heap_root().as_ptr();
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
        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(
            context.tagged_heap().heap_root(),
        );
        // SAFETY: the fresh sole-owner World above; same keeper preparation.
        let era = unsafe {
            neovm_value::test_support::authority::admit_for_test(
                collector,
                keeper,
                prepare_keeper_backing,
            )
        }
        .unwrap_or_else(|_| panic!("fresh fixture World admits its mutator era"));
        f(era)
    }

    fn with_test_rooted_mutator_era<R>(
        f: impl for<'heap> FnOnce(
            WorldProofEra<'heap>,
            Vec<neovm_value::tagged::transport::SharedRoot>,
        ) -> R,
    ) -> R {
        let mut context = new_context();
        let mut roots = Vec::new();
        for n in 0..4 {
            // SAFETY: sole serial owner; allocation invokes no collection or
            // callback, and the view is used in place and never swapped.
            let value = unsafe { context.tagged_heap_mut() }
                .alloc_cons(TaggedValue::fixnum(n), TaggedValue::NIL);
            // SAFETY: sole serial owner; allocation invokes no collection or
            // callback and the source word stays live through root admission.
            roots.push(unsafe {
                neovm_value::tagged::transport::SharedRoot::new(context.tagged_heap(), value)
            });
        }
        let keeper = context.issue_green_baton().expect("rooted fixture keeper");
        let heap: *mut TaggedHeap = context.tagged_heap().heap_root().as_ptr();
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
        neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(
            context.tagged_heap().heap_root(),
        );
        // SAFETY: same fresh keeper and frozen census as above.
        let era = unsafe {
            neovm_value::test_support::authority::admit_for_test(
                collector,
                keeper,
                prepare_keeper_backing,
            )
        }
        .unwrap_or_else(|_| panic!("rooted fixture era"));
        f(era, roots)
    }
}
