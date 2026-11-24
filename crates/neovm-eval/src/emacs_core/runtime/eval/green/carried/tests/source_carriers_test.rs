use super::{PieceStage, SourceCarrierFailure, SourceRoute, SourceWorldCarriers};
use std::any::Any;

fn receipt() -> (
    neovm_value::tagged::gc::AliasableBox<neovm_value::tagged::gc::TaggedHeap>,
    neovm_value::tagged::collection_reads::SourceObservationsDetached,
) {
    // Carrier placeholders read the process hook table. The proof does not
    // allocate a Context, and dropping it leaves the publication in place.
    let _hooks = crate::test_utils::unit_harness();
    let heap =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    unsafe { neovm_value::tagged::gc::set_tagged_heap_root(heap.root()) };
    // SAFETY: the fixture owns this heap. No collector, mutator, or store view
    // is live, and the drain callback does not re-enter the heap.
    let mut collector = unsafe { (*heap.root().as_ptr()).drain_to_quiescent(|_| {}) }.unwrap();
    neovm_value::tagged::gc::clear_tagged_heap_root_if_installed(heap.root());
    let receipt = collector.detach_source_observations().unwrap();
    drop(collector);
    (heap, receipt)
}

#[test]
fn pentry_e2b_source_carriers_catalog_matches_original_green_table() {
    use std::collections::HashSet;
    assert_eq!(super::super::CARRIERS.len(), 13);
    assert_eq!(
        super::super::SOURCE_DECLARATIONS.len(),
        super::super::CARRIERS.len()
    );
    let mut ids = HashSet::new();
    let mut separate = Vec::new();
    let mut local = Vec::new();
    for (declaration, (placeholder, swap)) in super::super::SOURCE_DECLARATIONS
        .iter()
        .zip(super::super::CARRIERS)
    {
        assert!(ids.insert(declaration.id));
        assert!(std::ptr::fn_addr_eq(declaration.placeholder, *placeholder));
        assert!(std::ptr::fn_addr_eq(declaration.swap, *swap));
        match declaration.id.source_route() {
            SourceRoute::SeparateObservations => separate.push(declaration.id),
            SourceRoute::WithdrawLocal => local.push(declaration.id),
        }
    }
    assert_eq!(separate, vec![super::super::CarrierId::Observations]);
    assert_eq!(local.len(), 12);
}

#[test]
fn pentry_e2b_source_carriers_actual_native_lossage_value_is_retained() {
    let (_heap, observations) = receipt();
    let mut seed: Box<dyn Any> = Box::new(511_i64);
    crate::emacs_core::builtins::symbols::swap_world_state(&mut *seed);
    let carriers = SourceWorldCarriers::withdraw(&observations).unwrap();
    let piece = carriers
        .withdrawal
        .pieces
        .iter()
        .find(|piece| piece.declaration.id == super::super::CarrierId::Lossage)
        .unwrap();
    assert_eq!(piece.payload.downcast_ref::<i64>(), Some(&511));
    assert_eq!(piece.stage, PieceStage::Withdrawn);
    let mut probe: Box<dyn Any> = Box::new(300_i64);
    crate::emacs_core::builtins::symbols::swap_world_state(&mut *probe);
    assert_eq!(
        probe.downcast_ref::<i64>(),
        Some(&300),
        "actual native TLS holds the placeholder"
    );
    assert!(
        observations.placeholder_is_current_for_test(),
        "observation carrier must not swap"
    );
}

#[test]
fn pentry_e2b_source_carriers_real_native_loan_returns_owned_partial_failure() {
    let (_heap, observations) = receipt();
    let failure = crate::emacs_core::builtins::symbols::with_source_lossage_reader_for_test(|| {
        SourceWorldCarriers::withdraw(&observations)
            .err()
            .expect("actual RefCell swap refuses")
    });
    match failure {
        SourceCarrierFailure::SwapPanicked(failure) => {
            let pieces = &failure.withdrawal.pieces;
            assert_eq!(pieces.len(), 12);
            let position = pieces
                .iter()
                .position(|piece| piece.declaration.id == super::super::CarrierId::Lossage)
                .unwrap();
            assert!(position > 0);
            assert!(
                pieces[..position]
                    .iter()
                    .all(|piece| piece.stage == PieceStage::Withdrawn)
            );
            assert_eq!(pieces[position].stage, PieceStage::Swapping);
            assert!(
                pieces[position + 1..]
                    .iter()
                    .all(|piece| piece.stage == PieceStage::Prepared)
            );
            assert!(observations.placeholder_is_current_for_test());
            drop(failure);
        }
        other => panic!("intended real native swap failure: {other:?}"),
    }
}
