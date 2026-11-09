//! Actual minimal World/collector inputs; no bootstrap or setup collection.
//! The symbol-era boot and the keeper preparation live in the core shell.
//! Assertions stay here.
use super::super::{
    EraAdmissionFailure, EraCore, EraKeeper, InitialTablePreparation, RegistrationError,
};
use super::*;
use crate::symbol::parallel::SymbolBacking;
use crate::tagged::gc::{FacadeEpochRetention, Gen1Admission, Gen1AdmissionError, ProofEra};

type SymbolEra<'a, K> = dyn FnMut(&mut dyn for<'e> FnMut(ProofEra<'e, K>)) + 'a;

fn admit_production<'heap, K: EraKeeper>(
    admission: Gen1Admission<'heap>,
    keeper: K,
    prepare: &mut dyn FnMut(&K) -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError>,
) -> Result<super::super::ParallelHeapEra<'heap, K>, EraAdmissionFailure<'heap, K>> {
    // SAFETY: the core shell passes the World retained-keeper adapter. It
    // returns the original epoch and backing and does not execute Lisp,
    // resume owners, or leak a manager reference.
    unsafe {
        super::super::ParallelHeapEra::admit_with(admission, keeper, |keeper| prepare(keeper))
    }
}

fn admit_proof<'heap, K: EraKeeper>(
    collector: crate::tagged::gc::QuiescentCollector<'heap>,
    keeper: K,
    prepare: &mut dyn FnMut(&K) -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError>,
) -> Result<ProofEra<'heap, K>, EraAdmissionFailure<'heap, K>> {
    // SAFETY: same retained-keeper contract as the production admission.
    unsafe { ProofEra::admit_with(collector, keeper, |keeper| prepare(keeper)) }
}

#[derive(Debug)]
enum ActualMode {
    Gen0,
    Gen1,
}

fn nmjit_n2_actual_constructor_checks_mode_and_returns_complete_ownership<K: EraKeeper>(
    with_symbol_era: &mut SymbolEra<'_, K>,
    prepare: &mut dyn FnMut(&K) -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError>,
    held_here: &dyn Fn(&K) -> bool,
) {
    with_symbol_era(&mut |era| {
        let (mut collector, keeper) = era
            .finish()
            .expect("empty fixture ends before admission probe");
        let heap = collector.heap_identity();
        let expected = match collector
            .registration_heap_mut()
            .tagged_heap_generational_enabled()
        {
            false => ActualMode::Gen0,
            true => ActualMode::Gen1,
        };
        let (collector, keeper) = match (expected, Gen1Admission::check(collector)) {
            (ActualMode::Gen0, Err(failure)) => {
                assert_eq!(failure.reason, Gen1AdmissionError::GenerationalRequired);
                eprintln!("NMJIT_N2_GEN0_TYPED_REFUSAL_OWNERSHIP_RETURNED");
                (failure.collector, keeper)
            }
            (ActualMode::Gen1, Ok(admission)) => {
                let era = admit_production(admission, keeper, prepare)
                    .expect("production era consumes actual GEN1 admission");
                assert_eq!(era.heap_identity(), heap);
                let permit = era
                    .register(1, 4)
                    .expect("real production registration requires retained GEN1 evidence");
                drop(permit);
                let inputs = era
                    .finish()
                    .expect("cancelled permit returns production inputs");
                eprintln!("NMJIT_N2_GEN1_ACTUAL_COLLECTOR_CAPABILITY");
                inputs
            }
            (mode, result) => panic!("actual heap mode mismatch: {mode:?}: {result:?}"),
        };
        assert_eq!(collector.heap_identity(), heap);
        assert_eq!(keeper.parallel_heap_identity(), heap);
        assert!(held_here(&keeper));
        // Inputs remain usable, not merely pointer-identical. Re-admission
        // uses the nominal proof-only route, including under GEN0.
        let era = admit_proof(collector, keeper, prepare)
            .expect("complete ownership permits fixture re-admission");
        let (collector, keeper) = era.finish().expect("re-admitted fixture ends");
        drop(collector);
        drop(keeper);
    });
}

fn nmjit_n2_proof_inputs_preserve_collector_mode_without_production_capability<K: EraKeeper>(
    with_symbol_era: &mut SymbolEra<'_, K>,
    prepare: &mut dyn FnMut(&K) -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError>,
    held_here: &dyn Fn(&K) -> bool,
) {
    let _ = (prepare, held_here);
    with_symbol_era(&mut |era| {
        let (mut collector, keeper) = era.finish().expect("empty proof fixture ends");
        let heap = collector.heap_identity();
        let mode = collector
            .registration_heap_mut()
            .tagged_heap_generational_enabled();
        let proof = ProofAdmission::check(collector, keeper).expect("explicit proof admission");
        let (checked, _proof_witness) = proof.into_checked();
        let (mut collector, keeper) = checked.into_parts();
        assert_eq!(collector.heap_identity(), heap);
        assert_eq!(
            collector
                .registration_heap_mut()
                .tagged_heap_generational_enabled(),
            mode
        );
        drop(collector);
        drop(keeper);
    });
}

fn nmjit_n3_initial_table_preparation_refusal_returns_complete_ownership<K: EraKeeper>(
    with_symbol_era: &mut SymbolEra<'_, K>,
    prepare: &mut dyn FnMut(&K) -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError>,
    held_here: &dyn Fn(&K) -> bool,
) {
    #[derive(Debug, PartialEq, Eq)]
    enum PreparationOutcome {
        Refused(RegistrationError),
        Admitted,
    }

    with_symbol_era(&mut |era| {
        let (mut collector, keeper) = era.finish().expect("empty proof fixture ends");
        let original_heap = collector.heap_identity();
        let original_mode = collector
            .registration_heap_mut()
            .tagged_heap_generational_enabled();
        let admission = ProofAdmission::check(collector, keeper)
            .expect("empty fixture returns valid exclusive admission inputs");
        let (checked, witness) = admission.into_checked();
        let (outcome, mut collector, keeper) = match EraCore::admit_checked(
            checked,
            witness,
            |keeper| prepare(keeper),
            InitialTablePreparation::RefuseOnce,
        ) {
            Err(failure) => (
                PreparationOutcome::Refused(failure.reason),
                failure.collector,
                failure.keeper,
            ),
            Ok((core, _witness)) => {
                let (collector, keeper) = core.finish().unwrap_or_else(|failure| {
                    panic!("unexpected admission cleanup refused: {:?}", failure.reason)
                });
                (PreparationOutcome::Admitted, collector, keeper)
            }
        };
        let returned_heap = collector.heap_identity();
        let returned_mode = collector
            .registration_heap_mut()
            .tagged_heap_generational_enabled();
        let returned_keeper_heap = keeper.parallel_heap_identity();
        let returned_keeper_held = held_here(&keeper);
        // The one-shot refusal must preserve usable ownership, not just scalar
        // identities. Finish the second admission before any semantic assertion.
        let era = admit_proof(collector, keeper, prepare)
            .expect("initial preparation refusal preserves re-admission inputs");
        let (mut collector, keeper) = era.finish().expect("re-admitted empty fixture ends");
        let recovered_heap = collector.heap_identity();
        let recovered_mode = collector
            .registration_heap_mut()
            .tagged_heap_generational_enabled();
        let recovered_keeper_heap = keeper.parallel_heap_identity();
        let recovered_keeper_held = held_here(&keeper);
        drop(collector);
        drop(keeper);
        assert_eq!(
            outcome,
            PreparationOutcome::Refused(RegistrationError::Capacity)
        );
        assert_eq!(returned_heap, original_heap);
        assert_eq!(returned_mode, original_mode);
        assert_eq!(returned_keeper_heap, original_heap);
        assert!(returned_keeper_held);
        assert_eq!(recovered_heap, original_heap);
        assert_eq!(recovered_mode, original_mode);
        assert_eq!(recovered_keeper_heap, original_heap);
        assert!(recovered_keeper_held);
        eprintln!("NMJIT_N3_INITIAL_TABLE_REFUSAL_OWNERSHIP_RETURNED");
    });
}

/// One moved construction test. The core shell opens the symbol era and
/// passes the World keeper preparation.
pub(crate) fn run_construction_nmjit_test<K: EraKeeper>(
    name: &str,
    with_symbol_era: &mut SymbolEra<'_, K>,
    prepare: &mut dyn FnMut(&K) -> Result<(FacadeEpochRetention, SymbolBacking), RegistrationError>,
    held_here: &dyn Fn(&K) -> bool,
) {
    match name {
        "nmjit_n2_actual_constructor_checks_mode_and_returns_complete_ownership" => {
            nmjit_n2_actual_constructor_checks_mode_and_returns_complete_ownership(
                with_symbol_era,
                prepare,
                held_here,
            )
        }
        "nmjit_n2_proof_inputs_preserve_collector_mode_without_production_capability" => {
            nmjit_n2_proof_inputs_preserve_collector_mode_without_production_capability(
                with_symbol_era,
                prepare,
                held_here,
            )
        }
        "nmjit_n3_initial_table_preparation_refusal_returns_complete_ownership" => {
            nmjit_n3_initial_table_preparation_refusal_returns_complete_ownership(
                with_symbol_era,
                prepare,
                held_here,
            )
        }
        other => panic!("unknown construction nmjit test {other}"),
    }
}
